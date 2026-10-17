//! Identify installed mods by file hash and replace verified updates transactionally.
use super::*;

#[derive(Clone)]
pub(super) struct LocalMod {
    name: String,
    path: PathBuf,
    sha1: String,
}

pub(super) struct Candidate {
    context: Context,
    local: LocalMod,
    version: Value,
    expires: Instant,
}

pub(super) fn scan(context: &Context) -> R<Vec<LocalMod>> {
    let directory = context.game_dir.join("mods");
    if !directory.exists() {
        return Ok(vec![]);
    }
    let game = fs::canonicalize(&context.game_dir).map_err(|_| "无法读取游戏目录。")?;
    if !fs::canonicalize(&directory)
        .map_err(|_| "无法读取模组目录。")?
        .starts_with(game)
    {
        return Err("模组目录指向游戏实例之外，无法检查更新。".into());
    }
    let mut files = Vec::new();
    let mut total = 0;
    for entry in fs::read_dir(directory).map_err(|_| "无法读取模组目录。")? {
        let entry = entry.map_err(|_| "无法读取模组文件。")?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();
        if !lower.ends_with(".jar") && !lower.ends_with(".jar.disabled") {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|_| "无法读取模组文件。")?;
        if !metadata.file_type().is_file() {
            return Err(format!("模组不是普通文件：{name}"));
        }
        total += metadata.len();
        if files.len() >= 1500 || metadata.len() > MAX_FILE || total > 8 * 1024 * 1024 * 1024 {
            return Err("模组文件数量或大小超出更新检查范围。".into());
        }
        files.push(LocalMod {
            name,
            sha1: file_hash(&path, false)?,
            path,
        });
    }
    Ok(files)
}

fn newer_release(current: &Value, latest: &Value, context: &Context) -> bool {
    let published =
        |v: &Value| chrono::DateTime::parse_from_rfc3339(field(v, "date_published")).ok();
    field(current, "project_id") == field(latest, "project_id")
        && !field(latest, "id").is_empty()
        && field(current, "id") != field(latest, "id")
        && field(latest, "version_type") == "release"
        && context.compatible(latest, "mod")
        && !matches!(
            field(latest, "environment"),
            "server_only" | "dedicated_server_only"
        )
        && matches!((published(current), published(latest)), (Some(old), Some(new)) if new > old)
}

impl Manager {
    pub(super) fn check_updates(
        &self,
        context: &Context,
        files: &[LocalMod],
        provider: &str,
    ) -> R<Vec<Value>> {
        self.update_candidates.lock().unwrap().clear();
        let paths = files
            .iter()
            .map(|f| (f.sha1.clone(), f.path.clone()))
            .collect();
        let identified = self.http.identify(&paths)?;
        let mut latest_by_project = HashMap::<String, Value>::new();
        for version in identified
            .as_object()
            .into_iter()
            .flat_map(|map| map.values())
        {
            let project_id = field(version, "project_id");
            if project_id.is_empty() || latest_by_project.contains_key(project_id) {
                continue;
            }
            let latest = match self.raw_versions(project_id, "mod", Some(context)) {
                Ok(versions) => versions.into_iter().next().unwrap_or(Value::Null),
                Err(error) => json!({"error": error}),
            };
            latest_by_project.insert(project_id.to_owned(), latest);
        }
        let latest = identified
            .as_object()
            .into_iter()
            .flat_map(|map| map.iter())
            .map(|(hash, current)| {
                let project_id = field(current, "project_id");
                (
                    hash.clone(),
                    latest_by_project
                        .get(project_id)
                        .cloned()
                        .unwrap_or(Value::Null),
                )
            })
            .collect::<HashMap<_, _>>();
        let mut candidates = self.update_candidates.lock().unwrap();
        Ok(files
            .iter()
            .filter_map(|local| {
                let current = identified.get(&local.sha1)?;
                let next = &latest[&local.sha1];
                let mut row = json!({
                    "name": local.name,
                    "provider": provider,
                    "currentVersion": current["version_number"],
                    "status": "current",
                    "message": "已是当前游戏版本可用的最新正式版",
                });
                if !field(next, "error").is_empty() {
                    row["status"] = json!("error");
                    row["message"] = next["error"].clone();
                } else if next.is_null() {
                    row["status"] = json!("noCompatible");
                    row["message"] = json!("平台暂无兼容的正式版");
                } else if newer_release(current, next, context) {
                    let id = format!(
                        "mod-{}-{}",
                        std::process::id(),
                        SERIAL.fetch_add(1, Ordering::Relaxed)
                    );
                    row["status"] = json!("available");
                    row["message"] = json!("发现兼容更新");
                    row["latestVersion"] = next["version_number"].clone();
                    row["updateId"] = json!(id);
                    candidates.insert(
                        id,
                        Candidate {
                            context: context.clone(),
                            local: local.clone(),
                            version: next.clone(),
                            expires: Instant::now() + Duration::from_secs(900),
                        },
                    );
                } else if field(current, "id") != field(next, "id")
                    && chrono::DateTime::parse_from_rfc3339(field(current, "date_published"))
                        .is_err()
                {
                    row["status"] = json!("error");
                    row["message"] = json!("平台缺少当前文件的发布时间，无法判断更新顺序");
                }
                Some(row)
            })
            .collect())
    }

    pub(crate) fn update_mod_file(
        &self,
        id: &str,
        context: Context,
        progress: &mut dyn FnMut(Value),
    ) -> R<Value> {
        let candidate = self
            .update_candidates
            .lock()
            .unwrap()
            .remove(id)
            .ok_or("更新检查已失效，请重新检查更新。")?;
        if candidate.expires < Instant::now() || candidate.context != context {
            return Err("游戏实例或设置已改变，请重新检查更新。".into());
        }
        let local = &candidate.local;
        let unchanged = || -> R<()> {
            let metadata = fs::symlink_metadata(&local.path)
                .map_err(|_| "原模组已被移动，请重新检查更新。")?;
            if !metadata.file_type().is_file() || file_hash(&local.path, false)? != local.sha1 {
                return Err("原模组在检查后发生变化，请重新检查更新。".into());
            }
            Ok(())
        };
        unchanged()?;
        // Re-scan dependencies so a stale check cannot silently overwrite another mod.
        if scan(&context)?
            .iter()
            .filter(|f| f.sha1 == local.sha1)
            .count()
            > 1
        {
            return Err("同一模组存在多个副本，请先处理重复文件再更新。".into());
        }
        let mut installed = self.installed(&context, true)?;
        installed.remove(field(&candidate.version, "project_id"));
        let mut resolver = Resolver {
            manager: self,
            context: &context,
            installed: &installed,
            files: vec![],
            projects: HashMap::new(),
            incompatible: vec![],
            visiting: HashSet::new(),
        };
        resolver.visit(candidate.version.clone(), "mod", 0)?;
        for dep in &resolver.incompatible {
            let project = field(dep, "project_id");
            let version = field(dep, "version_id");
            if if version.is_empty() {
                installed.contains_key(project) || resolver.projects.contains_key(project)
            } else {
                installed
                    .values()
                    .any(|v| field(&v.version, "id") == version)
                    || resolver.projects.values().any(|v| v == version)
            } {
                return Err("更新版本与当前已安装模组存在不兼容关系。".into());
            }
        }
        let disabled = local.name.to_ascii_lowercase().ends_with(".disabled");
        let mut files = resolver.files;
        if disabled {
            // Dependencies of a disabled mod must not become active during its update.
            for file in &mut files {
                let active = context.game_dir.join(&file.folder).join(&file.filename);
                if (file.project_id == field(&candidate.version, "project_id") || !active.exists())
                    && !file.filename.ends_with(".disabled")
                {
                    file.filename.push_str(".disabled");
                }
            }
        }
        // Preserve the disabled state of an already installed dependency as well.
        for file in &mut files {
            if let Some(existing) = installed.get(&file.project_id) {
                file.filename = existing.filename.clone();
            }
        }
        let total: u64 = files.iter().map(|f| f.size).sum();
        if total > MAX_TOTAL {
            return Err("本次更新及必要前置超过 2 GB。".into());
        }
        let game = fs::canonicalize(&context.game_dir).map_err(|_| "无法读取游戏目录。")?;
        let mut staged = UpdateStaging::default();
        let mut completed = 0;
        let mut targets = HashSet::new();
        for file in &files {
            if self.cancelled.load(Ordering::Relaxed) {
                return Err("更新已取消。".into());
            }
            let directory = context.game_dir.join(&file.folder);
            fs::create_dir_all(&directory).map_err(|_| "无法创建模组目录。")?;
            if !fs::canonicalize(&directory)
                .map_err(|_| "无法读取内容目录。")?
                .starts_with(&game)
            {
                return Err("内容目录指向游戏实例之外，已停止更新。".into());
            }
            let target = directory.join(&file.filename);
            if !targets.insert(target.to_string_lossy().to_lowercase()) {
                return Err("更新文件存在同名冲突。".into());
            }
            if target != local.path && check_destination(&target, file)? {
                completed += file.size;
                continue;
            }
            let temp = directory.join(format!(
                ".xumob-update-{}.part",
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            let mut out = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)
                .map_err(|_| "无法创建更新临时文件。")?;
            staged.temporary.push((temp.clone(), target));
            let mut last = Instant::now() - Duration::from_secs(1);
            self.http.download(file, &mut out, &self.cancelled, &mut |bytes| {
                if last.elapsed() >= Duration::from_millis(120) || bytes == file.size {
                    progress(json!({"filename": file.filename, "bytes": completed + bytes, "totalBytes": total}));
                    last = Instant::now();
                }
            })?;
            out.sync_all().map_err(|_| "无法保存更新文件。")?;
            drop(out);
            let expected = if file.sha512.is_empty() {
                &file.sha1
            } else {
                &file.sha512
            };
            if file_hash(&temp, !file.sha512.is_empty())? != *expected
                || fs::metadata(&temp).map_err(|_| "更新文件丢失。")?.len() != file.size
            {
                return Err("更新文件校验失败，原模组保持不变。".into());
            }
            completed += file.size;
        }
        unchanged()?;
        if self.cancelled.load(Ordering::Relaxed) {
            return Err("更新已取消。".into());
        }
        let backup = local.path.with_file_name(format!(
            ".xumob-update-{}.bak",
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        commit_new(&local.path, &backup).map_err(|_| "无法暂存原模组，更新未应用。")?;
        #[cfg(not(windows))]
        fs::remove_file(&local.path).map_err(|_| "无法移走原模组。")?;
        staged.original = Some((backup.clone(), local.path.clone()));
        for (temp, target) in &staged.temporary {
            if self.cancelled.load(Ordering::Relaxed) {
                return Err("更新已取消。".into());
            }
            commit_new(temp, target).map_err(|_| "更新目标已存在或无法写入，正在恢复原模组。")?;
            staged.committed.push(target.clone());
        }
        staged.complete = true;
        let backup_retained = fs::remove_file(&backup).is_err();
        Ok(json!({
            "updated": true,
            "filename": files.first().map(|f| &f.filename),
            "version": candidate.version["version_number"],
            "backupRetained": backup_retained,
        }))
    }
}

#[derive(Default)]
struct UpdateStaging {
    temporary: Vec<(PathBuf, PathBuf)>,
    committed: Vec<PathBuf>,
    original: Option<(PathBuf, PathBuf)>,
    complete: bool,
}
impl Drop for UpdateStaging {
    fn drop(&mut self) {
        if !self.complete {
            for target in &self.committed {
                let _ = fs::remove_file(target);
            }
            if let Some((backup, original)) = &self.original {
                let _ = commit_new(backup, original);
            }
        }
        for (temp, _) in &self.temporary {
            let _ = fs::remove_file(temp);
        }
    }
}

impl Service {
    pub(crate) fn mod_metadata(&self) -> R<Value> {
        let context = Context::from_settings(&self.settings())?;
        let files = scan(&context)?;
        let paths = files
            .iter()
            .map(|file| (file.sha1.clone(), file.path.clone()))
            .collect::<HashMap<_, _>>();
        let mut found = HashMap::<String, Value>::new();
        let mut warnings = Vec::new();
        let mut remaining = paths.clone();
        match self.modrinth.identify_metadata(&paths) {
            Ok(metadata) => {
                if let Some(entries) = metadata.as_object() {
                    for (hash, item) in entries {
                        if !field(item, "id").is_empty() {
                            found.insert(hash.clone(), item.clone());
                            remaining.remove(hash);
                        }
                    }
                }
            }
            Err(error) => warnings.push(format!("Modrinth：{error}")),
        }
        if !remaining.is_empty() && self.curseforge.vault.status()["configured"] == true {
            match self.curseforge.identify_metadata(&remaining) {
                Ok(metadata) => {
                    if let Some(entries) = metadata.as_object() {
                        found.extend(
                            entries
                                .iter()
                                .map(|(hash, item)| (hash.clone(), item.clone())),
                        );
                    }
                }
                Err(error) => warnings.push(format!("CurseForge：{error}")),
            }
        }
        let mods = files
            .iter()
            .map(|file| {
                let mut row = json!({"name": file.name});
                if let Some(item) = found.get(&file.sha1) {
                    row["title"] = item["title"].clone();
                    row["modId"] = item["id"].clone();
                    row["iconData"] = item["iconUrl"].clone();
                }
                row
            })
            .collect::<Vec<_>>();
        Ok(json!({"mods": mods, "warnings": warnings}))
    }

    pub(crate) fn check_mod_updates(&self) -> R<Value> {
        self.begin("checking", "正在识别模组并检查兼容更新…")?;
        let result = (|| {
            let context = Context::from_settings(&self.settings())?;
            if context.loader == "minecraft" {
                return Err("请先选择已安装模组加载器的游戏版本。".into());
            }
            let files = scan(&context)?;
            let mut rows = HashMap::new();
            let mut warnings = Vec::new();
            match self.modrinth.check_updates(&context, &files, "modrinth") {
                Ok(found) => {
                    for row in found {
                        rows.insert(field(&row, "name").to_owned(), row);
                    }
                }
                Err(error) => warnings.push(format!("Modrinth：{error}")),
            }
            let remaining = files
                .iter()
                .filter(|f| !rows.contains_key(&f.name))
                .cloned()
                .collect::<Vec<_>>();
            if !remaining.is_empty() {
                if self.curseforge.vault.status()["configured"] == true {
                    match self
                        .curseforge
                        .content
                        .check_updates(&context, &remaining, "curseforge")
                    {
                        Ok(found) => {
                            for row in found {
                                rows.insert(field(&row, "name").to_owned(), row);
                            }
                        }
                        Err(error) => warnings.push(format!("CurseForge：{error}")),
                    }
                } else {
                    warnings.push("此构建未配置 CurseForge，未识别文件只查询了 Modrinth。".into());
                }
            }
            let updates = files.iter().map(|f| rows.remove(&f.name).unwrap_or_else(|| {
                json!({"name": f.name, "status": if warnings.is_empty() { "unrecognized" } else { "error" },
                    "message": if warnings.is_empty() { "平台未识别此文件，无法自动检查更新" } else { "平台检查未完成，请重试" }})
            })).collect::<Vec<_>>();
            Ok(json!({"mods": updates, "warnings": warnings, "context": context}))
        })();
        self.end();
        self.state_to(
            if result.is_ok() { "idle" } else { "error" },
            result
                .as_ref()
                .map(|_| "模组更新检查完成")
                .unwrap_or_else(|e: &String| e.as_str()),
            None,
        );
        result
    }

    pub(crate) fn update_mod(&self, provider: &str, id: &str) -> R<Value> {
        let manager = match provider {
            "modrinth" => &self.modrinth,
            "curseforge" => &self.curseforge.content,
            _ => return Err("模组更新平台无效。".into()),
        };
        self.begin("downloading", "正在更新模组…")?;
        let result = Context::from_settings(&self.settings()).and_then(|context| {
            manager.update_mod_file(id, context, &mut |p| self.content_progress(p))
        });
        self.end();
        self.state_to(
            if result.is_ok() { "idle" } else { "error" },
            result
                .as_ref()
                .map(|_| "模组更新完成")
                .unwrap_or_else(|e| e.as_str()),
            None,
        );
        result
    }
}
