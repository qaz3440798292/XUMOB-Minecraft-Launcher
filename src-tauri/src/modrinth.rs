//! Public Modrinth content browsing and verified, instance-scoped installation.
use crate::core::{describe_version, game_directory, resolve_version, Service, R};
use serde::Serialize;
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::{Digest, Sha512};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const API: &str = "https://api.modrinth.com/v2";
const AGENT: &str = "XUMOB-Minecraft-Launcher/0.5.0";
const MAX_FILE: u64 = 512 * 1024 * 1024;
const MAX_TOTAL: u64 = 2 * 1024 * 1024 * 1024;
static SERIAL: AtomicU64 = AtomicU64::new(1);
fn field<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn items(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn contains(v: &Value, value: &str) -> bool {
    items(v).iter().any(|x| x.as_str() == Some(value))
}
fn identifier(value: &str) -> R<()> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("Modrinth 项目标识无效。".into());
    }
    Ok(())
}
fn kind_folder(kind: &str) -> R<&'static str> {
    match kind {
        "mod" => Ok("mods"),
        "resourcepack" => Ok("resourcepacks"),
        "shader" => Ok("shaderpacks"),
        _ => Err("请选择模组、资源包或光影。".into()),
    }
}
pub fn project_url(id: &str, kind: &str) -> R<String> {
    identifier(id)?;
    kind_folder(kind)?;
    Ok(format!("https://modrinth.com/{kind}/{id}"))
}
fn safe_filename(value: &str, kind: &str) -> R<()> {
    let bad = value.is_empty()
        || value.encode_utf16().count() > 180
        || value.starts_with('.')
        || value.ends_with(['.', ' '])
        || value
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c));
    let base = value.split('.').next().unwrap_or("").to_ascii_uppercase();
    let device = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && base.as_bytes()[3].is_ascii_digit());
    let extension = if kind == "mod" { ".jar" } else { ".zip" };
    if bad || device || !value.to_ascii_lowercase().ends_with(extension) {
        return Err("Modrinth 返回了不受支持的文件名。".into());
    }
    Ok(())
}
fn cdn_url(value: &str) -> R<reqwest::Url> {
    let url = reqwest::Url::parse(value).map_err(|_| "下载地址无效。")?;
    if url.scheme() != "https"
        || url.host_str() != Some("cdn.modrinth.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || !url.path().starts_with("/data/")
        || url.fragment().is_some()
    {
        return Err("下载地址不是 Modrinth 官方文件源。".into());
    }
    Ok(url)
}
fn hex_hash(value: &str, len: usize) -> bool {
    value.len() == len && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn status_error(status: u16) -> String {
    match status {
        404 => "这个项目或版本已不可用。",
        429 => "Modrinth 请求次数较多，请稍后再试。",
        410 => "Modrinth 接口已更新，请更新启动器。",
        401 | 403 => "Modrinth 拒绝了请求，请稍后重试或检查网络。",
        _ => "Modrinth 服务暂时不可用，请稍后重试。",
    }
    .into()
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub game_root: String,
    pub version_id: String,
    pub game_dir: PathBuf,
    pub minecraft: String,
    pub loader: String,
}
impl Context {
    pub fn from_settings(settings: &Value) -> R<Self> {
        let root = field(settings, "gameRoot");
        let id = field(settings, "versionId");
        if root.is_empty() || id.is_empty() || !Path::new(root).is_absolute() {
            return Err("请先在游戏库选择一个已安装的游戏版本。".into());
        }
        let version = describe_version(&resolve_version(Path::new(root), id)?);
        let label = field(&version, "loader").to_ascii_lowercase();
        let loader = ["neoforge", "fabric", "quilt", "forge"]
            .into_iter()
            .find(|x| label.contains(x))
            .unwrap_or("minecraft");
        Ok(Self {
            game_root: root.into(),
            version_id: id.into(),
            game_dir: game_directory(Path::new(root), id, &settings["isolated"])?,
            minecraft: field(&version, "minecraft").into(),
            loader: loader.into(),
        })
    }
    fn compatible(&self, version: &Value, kind: &str) -> bool {
        contains(&version["game_versions"], &self.minecraft)
            && match kind {
                "mod" => self.loader != "minecraft" && contains(&version["loaders"], &self.loader),
                "resourcepack" => contains(&version["loaders"], "minecraft"),
                "shader" => {
                    contains(&version["loaders"], "iris")
                        || contains(&version["loaders"], "optifine")
                }
                _ => false,
            }
    }
}
trait Transport: Send + Sync {
    fn api(&self, route: &str, query: &[(String, String)], body: Option<Value>) -> R<Value>;
    fn download(
        &self,
        file: &ContentFile,
        out: &mut File,
        cancelled: &AtomicBool,
        progress: &mut dyn FnMut(u64),
    ) -> R<()>;
}
struct Network {
    client: reqwest::blocking::Client,
    files: reqwest::Client,
}
impl Network {
    fn new() -> R<Self> {
        Ok(Self {
            client: reqwest::blocking::Client::builder()
                .user_agent(AGENT)
                .connect_timeout(Duration::from_secs(12))
                .timeout(Duration::from_secs(90))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| "无法初始化 Modrinth 连接。")?,
            files: reqwest::Client::builder()
                .user_agent(AGENT)
                .connect_timeout(Duration::from_secs(12))
                .read_timeout(Duration::from_secs(20))
                .timeout(Duration::from_secs(600))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| "无法初始化 Modrinth 下载连接。")?,
        })
    }
}
impl Transport for Network {
    fn api(&self, route: &str, query: &[(String, String)], body: Option<Value>) -> R<Value> {
        let url = format!("{API}{route}");
        let request = if let Some(body) = body {
            self.client.post(url).json(&body)
        } else {
            self.client.get(url).query(query)
        };
        let response = request
            .header("Accept", "application/json")
            .timeout(Duration::from_secs(25))
            .send()
            .map_err(|_| "无法连接 Modrinth，请检查网络后重试。")?;
        if !response.status().is_success() {
            return Err(status_error(response.status().as_u16()));
        }
        let mut bytes = Vec::new();
        response
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "无法读取 Modrinth 响应。")?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("Modrinth 响应过大，请缩小筛选范围。".into());
        }
        serde_json::from_slice(&bytes).map_err(|_| "Modrinth 返回了无法读取的数据。".into())
    }
    fn download(
        &self,
        file: &ContentFile,
        out: &mut File,
        cancelled: &AtomicBool,
        progress: &mut dyn FnMut(u64),
    ) -> R<()> {
        let url = cdn_url(&file.url)?;
        let client = self.files.clone();
        let size = file.size;
        let mut output = out.try_clone().map_err(|_| "无法写入下载文件。")?;
        let (sender, receiver) = std::sync::mpsc::sync_channel(16);
        let task = tauri::async_runtime::spawn(async move {
            let mut response = client
                .get(url)
                .send()
                .await
                .map_err(|_| "无法连接 Modrinth 文件源，请重试。".to_string())?;
            if !response.status().is_success() {
                return Err(status_error(response.status().as_u16()));
            }
            if response.content_length().is_some_and(|n| n != size) {
                return Err("下载文件大小与 Modrinth 记录不一致。".into());
            }
            let mut total = 0;
            while let Some(bytes) = response
                .chunk()
                .await
                .map_err(|_| "文件下载中断，请重试。".to_string())?
            {
                total += bytes.len() as u64;
                if total > size {
                    return Err("下载内容超过了预期大小。".into());
                }
                output
                    .write_all(&bytes)
                    .map_err(|_| "无法写入游戏目录，请检查磁盘空间与权限。".to_string())?;
                let _ = sender.try_send(total);
            }
            if total != size {
                return Err("下载文件不完整，请重试。".into());
            }
            Ok(())
        });
        loop {
            if cancelled.load(Ordering::Relaxed) {
                task.abort();
                let _ = tauri::async_runtime::block_on(task);
                return Err("下载已取消。".into());
            }
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(bytes) => progress(bytes),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        tauri::async_runtime::block_on(task).map_err(|_| "下载任务异常结束。".to_string())??;
        progress(size);
        Ok(())
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContentFile {
    project_id: String,
    version_id: String,
    title: String,
    filename: String,
    folder: String,
    size: u64,
    sha512: String,
    sha1: String,
    dependency: bool,
    #[serde(skip)]
    url: String,
}
#[derive(Clone)]
struct Ticket {
    context: Context,
    files: Vec<ContentFile>,
    expires: Instant,
    incompatible: Vec<Value>,
}
#[derive(Clone)]
struct Installed {
    version: Value,
    filename: String,
}
pub struct Manager {
    http: Arc<dyn Transport>,
    tickets: Mutex<HashMap<String, Ticket>>,
    cache: Mutex<HashMap<String, (Instant, Value, usize)>>,
    cancelled: AtomicBool,
}
impl Manager {
    pub fn new() -> R<Arc<Self>> {
        Ok(Self::with_transport(Arc::new(Network::new()?)))
    }
    fn with_transport(http: Arc<dyn Transport>) -> Arc<Self> {
        Arc::new(Self {
            http,
            tickets: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
            cancelled: AtomicBool::new(false),
        })
    }
    fn get(&self, route: &str, query: Vec<(String, String)>) -> R<Value> {
        let key = format!("{route}:{query:?}");
        if let Some((time, value, _)) = self.cache.lock().unwrap().get(&key) {
            if time.elapsed() < Duration::from_secs(90) {
                return Ok(value.clone());
            }
        }
        let value = self.http.api(route, &query, None)?;
        let bytes = serde_json::to_vec(&value)
            .map_err(|_| "无法读取 Modrinth 数据。")?
            .len();
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= 48 || cache.values().map(|v| v.2).sum::<usize>() + bytes > 4 * 1024 * 1024
        {
            cache.clear();
        }
        if bytes <= 4 * 1024 * 1024 {
            cache.insert(key, (Instant::now(), value.clone(), bytes));
        }
        Ok(value)
    }
    pub fn search(
        &self,
        query: &str,
        kind: &str,
        offset: u32,
        sort: &str,
        context: Option<&Context>,
    ) -> R<Value> {
        kind_folder(kind)?;
        if query.chars().count() > 120 || offset > 10000 {
            return Err("搜索范围过大。".into());
        }
        if !["relevance", "downloads", "updated", "newest"].contains(&sort) {
            return Err("排序方式无效。".into());
        }
        let mut facets = vec![vec![format!("project_type:{kind}")]];
        if let Some(ctx) = context {
            facets.push(vec![format!("versions:{}", ctx.minecraft)]);
            if kind == "mod" && ctx.loader != "minecraft" {
                facets.push(vec![format!("categories:{}", ctx.loader)]);
            }
        }
        let result = self.get(
            "/search",
            vec![
                ("query".into(), query.into()),
                ("facets".into(), json!(facets).to_string()),
                ("index".into(), sort.into()),
                ("offset".into(), offset.to_string()),
                ("limit".into(), "20".into()),
            ],
        )?;
        let hits = items(&result["hits"]).iter().map(|v| json!({"id":v["project_id"],"title":v["title"],"description":v["description"],"author":v["author"],"downloads":v["downloads"],"categories":v["categories"],"kind":kind})).collect::<Vec<_>>();
        Ok(
            json!({"hits":hits,"total":result["total_hits"],"offset":offset,"limit":20,"context":context}),
        )
    }
    fn project(&self, id: &str) -> R<Value> {
        identifier(id)?;
        self.get(&format!("/project/{id}"), vec![])
    }
    fn raw_version(&self, id: &str) -> R<Value> {
        identifier(id)?;
        self.get(&format!("/version/{id}"), vec![])
    }
    fn raw_versions(&self, id: &str, kind: &str, context: Option<&Context>) -> R<Vec<Value>> {
        identifier(id)?;
        kind_folder(kind)?;
        let mut query = vec![("include_changelog".into(), "false".into())];
        if let Some(ctx) = context {
            query.push(("game_versions".into(), json!([ctx.minecraft]).to_string()));
            if kind == "mod" && ctx.loader != "minecraft" {
                query.push(("loaders".into(), json!([ctx.loader]).to_string()));
            }
            if kind == "resourcepack" {
                query.push(("loaders".into(), json!(["minecraft"]).to_string()));
            }
        }
        let result = self.get(&format!("/project/{id}/version"), query)?;
        let mut versions = result.as_array().ok_or("Modrinth 版本列表无效。")?.clone();
        versions.retain(|v| {
            context.is_none_or(|ctx| ctx.compatible(v, kind))
                && !matches!(
                    field(v, "environment"),
                    "server_only" | "dedicated_server_only"
                )
        });
        versions.sort_by(|a, b| field(b, "date_published").cmp(field(a, "date_published")));
        Ok(versions)
    }
    pub fn versions(&self, id: &str, kind: &str, context: Option<&Context>) -> R<Value> {
        let project = self.project(id)?;
        if field(&project, "project_type") != kind {
            return Err("项目类型与选择的下载类型不一致。".into());
        }
        let versions = self.raw_versions(id, kind, context)?.into_iter().take(100).map(|v| json!({"id":v["id"],"name":v["name"],"number":v["version_number"],"channel":v["version_type"],"date":v["date_published"],"loaders":v["loaders"],"gameVersions":v["game_versions"],"size":items(&v["files"]).iter().find(|f| f["primary"]==true).or_else(|| items(&v["files"]).first()).map(|f|f["size"].clone())})).collect::<Vec<_>>();
        Ok(
            json!({"project":{"id":project["id"],"title":project["title"],"description":project["description"],"kind":kind},"versions":versions,"context":context}),
        )
    }
    fn installed(&self, context: &Context, cancellable: bool) -> R<HashMap<String, Installed>> {
        let mut by_hash = HashMap::new();
        let mut total = 0;
        let game = fs::canonicalize(&context.game_dir).map_err(|_| "无法读取当前游戏实例目录。")?;
        for folder in ["mods", "resourcepacks", "shaderpacks"] {
            let dir = context.game_dir.join(folder);
            if dir.exists()
                && !fs::canonicalize(&dir)
                    .map_err(|_| "无法读取内容目录。")?
                    .starts_with(&game)
            {
                return Err("内容目录指向游戏实例之外，已停止安装。".into());
            }
            let entries = match fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => return Err("无法读取当前实例的内容目录。".into()),
            };
            for entry in entries {
                if cancellable && self.cancelled.load(Ordering::Relaxed) {
                    return Err("下载已取消。".into());
                }
                let entry = entry.map_err(|_| "无法读取当前实例的文件。")?;
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if !path.is_file()
                    || !(name.to_ascii_lowercase().ends_with(".jar")
                        || name.to_ascii_lowercase().ends_with(".zip"))
                {
                    continue;
                }
                if fs::symlink_metadata(&path)
                    .map_err(|_| "无法读取内容文件。")?
                    .file_type()
                    .is_symlink()
                {
                    return Err("内容目录包含链接文件，请先检查该文件。".into());
                }
                let size = entry
                    .metadata()
                    .map_err(|_| "无法读取内容文件大小。")?
                    .len();
                total += size;
                if by_hash.len() >= 1500 || size > MAX_FILE || total > 8 * 1024 * 1024 * 1024 {
                    return Err("实例内容过多或包含超大文件，暂时无法安全检查前置冲突。".into());
                }
                by_hash.insert(file_hash(&path, false)?, name);
            }
        }
        let mut installed: HashMap<String, Installed> = HashMap::new();
        let hashes = by_hash.keys().cloned().collect::<Vec<_>>();
        for chunk in hashes.chunks(100) {
            if cancellable && self.cancelled.load(Ordering::Relaxed) {
                return Err("下载已取消。".into());
            }
            let result = self.http.api(
                "/version_files",
                &[],
                Some(json!({"hashes":chunk,"algorithm":"sha1"})),
            )?;
            let map = result.as_object().ok_or("Modrinth 文件识别响应无效。")?;
            for (hash, version) in map {
                if let Some(name) = by_hash.get(hash) {
                    let id = field(version, "project_id");
                    identifier(id)?;
                    if installed
                        .get(id)
                        .is_some_and(|old| field(&old.version, "id") != field(version, "id"))
                    {
                        return Err(format!(
                            "已安装同一项目的多个版本，请先处理重复文件：{name}"
                        ));
                    }
                    installed.insert(
                        id.into(),
                        Installed {
                            version: version.clone(),
                            filename: name.clone(),
                        },
                    );
                }
            }
        }
        Ok(installed)
    }
    pub fn prepare(&self, id: &str, version_id: &str, kind: &str, context: Context) -> R<Value> {
        identifier(id)?;
        identifier(version_id)?;
        kind_folder(kind)?;
        if kind == "mod" && context.loader == "minecraft" {
            return Err(
                "当前实例没有可匹配的模组加载器，请选择 Fabric、Forge、NeoForge 或 Quilt 实例。"
                    .into(),
            );
        }
        let installed = self.installed(&context, false)?;
        let version = self.raw_version(version_id)?;
        if field(&version, "project_id") != id {
            return Err("选择的文件不属于当前项目。".into());
        }
        let mut resolver = Resolver {
            manager: self,
            context: &context,
            installed: &installed,
            files: vec![],
            projects: HashMap::new(),
            incompatible: vec![],
            visiting: HashSet::new(),
        };
        resolver.visit(version, kind, false)?;
        for dep in &resolver.incompatible {
            let project = field(dep, "project_id");
            let version = field(dep, "version_id");
            let conflict = if !version.is_empty() {
                resolver.projects.values().any(|id| id == version)
                    || installed
                        .values()
                        .any(|i| field(&i.version, "id") == version)
            } else {
                resolver.projects.contains_key(project) || installed.contains_key(project)
            };
            if conflict {
                return Err("该版本声明了与已选或已安装内容不兼容的依赖，请选择其他版本。".into());
            }
        }
        let incompatible = resolver.incompatible;
        let files = resolver.files;
        let total: u64 = files.iter().map(|f| f.size).sum();
        if total > MAX_TOTAL {
            return Err("本次下载超过 2 GB，请分批安装。".into());
        }
        let mut names = HashSet::new();
        for f in &files {
            if !names.insert(format!("{}/{}", f.folder, f.filename).to_lowercase()) {
                return Err("多个项目使用了相同文件名，无法安全安装。".into());
            }
            check_destination(&context.game_dir.join(&f.folder).join(&f.filename), f)?;
        }
        let id = format!(
            "{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        );
        let result = json!({"planId":id,"context":context,"files":files,"totalBytes":total,"dependencyCount":files.iter().filter(|f|f.dependency).count()});
        let mut tickets = self.tickets.lock().unwrap();
        tickets.retain(|_, t| t.expires > Instant::now());
        if tickets.len() >= 8 {
            tickets.clear();
        }
        tickets.insert(
            id,
            Ticket {
                context,
                files,
                expires: Instant::now() + Duration::from_secs(900),
                incompatible,
            },
        );
        Ok(result)
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
    pub(crate) fn reset_cancellation(&self) {
        self.cancelled.store(false, Ordering::Relaxed);
    }
    fn install(&self, id: &str, context: Context, progress: &mut dyn FnMut(Value)) -> R<Value> {
        let ticket = self
            .tickets
            .lock()
            .unwrap()
            .remove(id)
            .ok_or("安装预览已失效，请重新选择版本。")?;
        if ticket.expires < Instant::now() || ticket.context != context {
            return Err("游戏实例或设置已改变，请重新生成安装预览。".into());
        }
        let installed = self.installed(&context, true)?;
        for file in &ticket.files {
            if installed
                .get(&file.project_id)
                .is_some_and(|old| field(&old.version, "id") != file.version_id)
            {
                return Err("内容目录在预览后发生了版本冲突，请重新生成安装预览。".into());
            }
        }
        for dep in &ticket.incompatible {
            let conflict = if !field(dep, "version_id").is_empty() {
                installed
                    .values()
                    .any(|old| field(&old.version, "id") == field(dep, "version_id"))
            } else {
                installed.contains_key(field(dep, "project_id"))
            };
            if conflict {
                return Err("内容目录在预览后新增了不兼容项目，请重新生成安装预览。".into());
            }
        }
        fs::create_dir_all(&context.game_dir).map_err(|_| "无法创建游戏内容目录。")?;
        let game = fs::canonicalize(&context.game_dir).map_err(|_| "无法读取安装目录。")?;
        let total: u64 = ticket.files.iter().map(|f| f.size).sum();
        let mut completed = 0;
        let mut staged = Staged {
            temporary: vec![],
            committed: vec![],
            complete: false,
        };
        let mut skipped = 0;
        for (index, file) in ticket.files.iter().enumerate() {
            if self.cancelled.load(Ordering::Relaxed) {
                return Err("下载已取消。".into());
            }
            let dir = context.game_dir.join(&file.folder);
            fs::create_dir_all(&dir).map_err(|_| "无法创建内容文件夹。")?;
            if !fs::canonicalize(&dir)
                .map_err(|_| "无法读取内容目录。")?
                .starts_with(&game)
            {
                return Err("内容目录指向游戏实例之外，已停止安装。".into());
            }
            let target = dir.join(&file.filename);
            if check_destination(&target, file)? {
                completed += file.size;
                skipped += 1;
                continue;
            }
            let temp = dir.join(format!(
                ".xumob-{}-{}.part",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            let mut out = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)
                .map_err(|_| "无法创建下载临时文件。")?;
            staged.temporary.push((temp.clone(), target));
            let mut last = Instant::now() - Duration::from_secs(1);
            self.http.download(file, &mut out, &self.cancelled, &mut |bytes| { if last.elapsed() >= Duration::from_millis(120) || bytes == file.size { progress(json!({"filename":file.filename,"bytes":completed+bytes,"totalBytes":total,"fileIndex":index+1,"fileCount":ticket.files.len()})); last = Instant::now(); } })?;
            out.sync_all().map_err(|_| "无法保存下载文件。")?;
            drop(out);
            if file_hash(&temp, true)? != file.sha512
                || fs::metadata(&temp).map_err(|_| "下载文件丢失。")?.len() != file.size
            {
                return Err(format!("{} 校验失败，未安装任何新文件。", file.filename));
            }
            completed += file.size;
        }
        if self.cancelled.load(Ordering::Relaxed) {
            return Err("下载已取消。".into());
        }
        // Every file is complete and verified before commit. Commit never replaces
        // an existing path; rollback owns only
        // new paths created in this operation, never preexisting user content.
        for (temp, target) in &staged.temporary {
            if self.cancelled.load(Ordering::Relaxed) {
                return Err("下载已取消。".into());
            }
            commit_new(temp, target)
                .map_err(|_| "目标文件已存在或无法写入，安装已撤回，请检查目录后重新预览。")?;
            staged.committed.push(target.clone());
        }
        staged.complete = true;
        Ok(
            json!({"installed":staged.committed.len(),"alreadyInstalled":skipped,"directory":context.game_dir}),
        )
    }
}

struct Resolver<'a> {
    manager: &'a Manager,
    context: &'a Context,
    installed: &'a HashMap<String, Installed>,
    files: Vec<ContentFile>,
    projects: HashMap<String, String>,
    incompatible: Vec<Value>,
    visiting: HashSet<String>,
}
impl Resolver<'_> {
    fn visit(&mut self, version: Value, kind: &str, dependency: bool) -> R<()> {
        let id = field(&version, "id").to_string();
        let project_id = field(&version, "project_id").to_string();
        identifier(&id)?;
        identifier(&project_id)?;
        if !self.context.compatible(&version, kind) {
            return Err("这个版本与当前 Minecraft 版本或加载器不兼容。".into());
        }
        if let Some(previous) = self.projects.get(&project_id) {
            if previous == &id {
                return Ok(());
            }
            return Err("多个前置要求同一项目的不同版本，请选择其他版本。".into());
        }
        if self.projects.len() >= 64 || !self.visiting.insert(id.clone()) {
            return Err("前置关系过深或存在循环，无法自动安装。".into());
        }
        let project = self.manager.project(&project_id)?;
        if field(&project, "project_type") != kind
            || matches!(
                field(&version, "environment"),
                "server_only" | "dedicated_server_only"
            )
            || field(&project, "client_side") == "unsupported"
        {
            return Err("这个项目不适用于当前客户端内容类型。".into());
        }
        if let Some(existing) = self.installed.get(&project_id) {
            if field(&existing.version, "id") != id {
                return Err(format!(
                    "已存在该项目的其他版本：{}。请先在内容文件夹处理旧版，再重新安装。",
                    existing.filename
                ));
            }
        }
        let file = items(&version["files"])
            .iter()
            .find(|f| f["primary"] == true)
            .or_else(|| items(&version["files"]).first())
            .ok_or("该版本没有可下载文件。")?;
        let filename = field(file, "filename");
        safe_filename(filename, kind)?;
        cdn_url(field(file, "url"))?;
        let size = file["size"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= MAX_FILE)
            .ok_or("文件大小超出支持范围（单文件最大 512 MB）。")?;
        let sha512 = field(&file["hashes"], "sha512").to_ascii_lowercase();
        let sha1 = field(&file["hashes"], "sha1").to_ascii_lowercase();
        if !hex_hash(&sha512, 128) || !hex_hash(&sha1, 40) {
            return Err("文件缺少有效的 Modrinth 校验信息。".into());
        }
        self.projects.insert(project_id.clone(), id.clone());
        self.files.push(ContentFile {
            project_id,
            version_id: id.clone(),
            title: field(&project, "title").into(),
            filename: filename.into(),
            folder: kind_folder(kind)?.into(),
            size,
            sha512,
            sha1,
            url: field(file, "url").into(),
            dependency,
        });
        for dep in items(&version["dependencies"]) {
            match field(dep, "dependency_type") {
                "incompatible" => self.incompatible.push(dep.clone()),
                "required" => {
                    let version_id = field(dep, "version_id");
                    let project_id = field(dep, "project_id");
                    let next = if !version_id.is_empty() {
                        self.manager.raw_version(version_id)?
                    } else if !project_id.is_empty() {
                        if let Some(existing) = self.installed.get(project_id) {
                            existing.version.clone()
                        } else {
                            let project = self.manager.project(project_id)?;
                            let dependency_kind = field(&project, "project_type");
                            kind_folder(dependency_kind)?;
                            let versions = self.manager.raw_versions(
                                project_id,
                                dependency_kind,
                                Some(self.context),
                            )?;
                            versions
                                .iter()
                                .find(|v| field(v, "version_type") == "release")
                                .or_else(|| versions.first())
                                .cloned()
                                .ok_or("找不到兼容的必要前置版本。")?
                        }
                    } else {
                        return Err(
                            "项目包含无法自动获取的外部必要前置，请先在 Modrinth 项目页确认。"
                                .into(),
                        );
                    };
                    if !project_id.is_empty() && field(&next, "project_id") != project_id {
                        return Err("前置项目与版本标识不一致。".into());
                    }
                    let project = self.manager.project(field(&next, "project_id"))?;
                    let next_kind = field(&project, "project_type").to_string();
                    self.visit(next, &next_kind, true)?;
                }
                _ => {}
            }
        }
        self.visiting.remove(&id);
        Ok(())
    }
}
fn file_hash(path: &Path, sha512: bool) -> R<String> {
    let mut file = File::open(path).map_err(|_| "无法读取内容文件进行校验。")?;
    let mut buffer = [0u8; 65536];
    let mut long = Sha512::new();
    let mut short = Sha1::new();
    loop {
        let n = file.read(&mut buffer).map_err(|_| "无法校验内容文件。")?;
        if n == 0 {
            break;
        }
        if sha512 {
            long.update(&buffer[..n]);
        } else {
            short.update(&buffer[..n]);
        }
    }
    Ok(if sha512 {
        hex::encode(long.finalize())
    } else {
        hex::encode(short.finalize())
    })
}
fn check_destination(path: &Path, file: &ContentFile) -> R<bool> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("无法检查目标文件。".into()),
        Ok(meta)
            if meta.is_file()
                && !meta.file_type().is_symlink()
                && meta.len() == file.size
                && file_hash(path, true)? == file.sha512 =>
        {
            Ok(true)
        }
        _ => Err(format!(
            "目标已存在不同内容：{}。请先处理该文件，启动器不会覆盖它。",
            file.filename
        )),
    }
}
struct Staged {
    temporary: Vec<(PathBuf, PathBuf)>,
    committed: Vec<PathBuf>,
    complete: bool,
}
#[cfg(windows)]
fn commit_new(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(source: *const u16, target: *const u16, flags: u32) -> i32;
    }
    // Canonical paths include the extended Windows prefix for long filenames.
    // Omitting MOVEFILE_REPLACE_EXISTING makes a concurrent target collision fail.
    let source = fs::canonicalize(source)?;
    let parent = target
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing parent"))?;
    let name = target
        .file_name()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing filename"))?;
    let target = fs::canonicalize(parent)?.join(name);
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), 8) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
#[cfg(not(windows))]
fn commit_new(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::hard_link(source, target)
}
impl Drop for Staged {
    fn drop(&mut self) {
        if !self.complete {
            for file in &self.committed {
                let _ = fs::remove_file(file);
            }
        }
        for (temp, _) in &self.temporary {
            let _ = fs::remove_file(temp);
        }
    }
}

impl Service {
    pub fn modrinth_search(&self, query: &str, kind: &str, offset: u32, sort: &str) -> R<Value> {
        let context = Context::from_settings(&self.settings()).ok();
        self.modrinth
            .search(query, kind, offset, sort, context.as_ref())
    }
    pub fn modrinth_versions(&self, id: &str, kind: &str) -> R<Value> {
        let context = Context::from_settings(&self.settings()).ok();
        self.modrinth.versions(id, kind, context.as_ref())
    }
    pub fn modrinth_prepare(&self, id: &str, version: &str, kind: &str) -> R<Value> {
        self.ensure_account_editable()?;
        self.modrinth
            .prepare(id, version, kind, Context::from_settings(&self.settings())?)
    }
    pub fn modrinth_install(&self, id: &str) -> R<Value> {
        self.begin("downloading", "正在下载 Modrinth 内容…")?;
        let result = (|| {
            let context = Context::from_settings(&self.settings())?;
            self.modrinth
                .install(id, context, &mut |progress| self.content_progress(progress))
        })();
        match &result {
            Ok(value) => {
                self.log(
                    &format!(
                        "Modrinth 安装完成：新增 {} 个文件，{} 个已存在。",
                        value["installed"], value["alreadyInstalled"]
                    ),
                    "info",
                );
                self.state_to("idle", "Modrinth 内容已安装完成", None);
            }
            Err(error) => {
                let cancelled = error == "下载已取消。";
                self.log(error, if cancelled { "info" } else { "error" });
                self.state_to(if cancelled { "idle" } else { "error" }, error, None);
            }
        }
        self.end();
        result
    }
}

#[cfg(test)]
#[path = "modrinth_tests.rs"]
mod tests;
