use super::*;
#[derive(Default)]
struct Mock {
    projects: HashMap<String, Value>,
    versions: HashMap<String, Value>,
    bytes: HashMap<String, Vec<u8>>,
    known: HashMap<String, Value>,
    calls: Mutex<Vec<(String, Vec<(String, String)>)>>,
    corrupt: bool,
    cancel: bool,
    collision: Option<PathBuf>,
}
impl Transport for Mock {
    fn api(&self, route: &str, query: &[(String, String)], body: Option<Value>) -> R<Value> {
        self.calls
            .lock()
            .unwrap()
            .push((route.into(), query.to_vec()));
        if route == "/search" {
            return Ok(json!({"hits":[],"total_hits":0}));
        }
        if route == "/version_files" {
            let body = body.unwrap();
            let mut out = serde_json::Map::new();
            for hash in items(&body["hashes"]) {
                if let Some(v) = self.known.get(hash.as_str().unwrap()) {
                    out.insert(hash.as_str().unwrap().into(), v.clone());
                }
            }
            return Ok(Value::Object(out));
        }
        let parts = route.trim_matches('/').split('/').collect::<Vec<_>>();
        if parts.len() == 3 && parts[0] == "project" && parts[2] == "version" {
            return Ok(json!(self
                .versions
                .values()
                .filter(|v| field(v, "project_id") == parts[1])
                .cloned()
                .collect::<Vec<_>>()));
        }
        match parts.as_slice() {
            ["project", id] => self
                .projects
                .get(*id)
                .cloned()
                .ok_or("missing project".into()),
            ["version", id] => self
                .versions
                .get(*id)
                .cloned()
                .ok_or("missing version".into()),
            _ => Err(format!("Unexpected route: {route}")),
        }
    }
    fn download(
        &self,
        file: &ContentFile,
        out: &mut File,
        cancelled: &AtomicBool,
        progress: &mut dyn FnMut(u64),
    ) -> R<()> {
        if self.cancel {
            cancelled.store(true, Ordering::Relaxed);
            return Err("下载已取消。".into());
        }
        let bytes = if self.corrupt {
            b"wrong".to_vec()
        } else {
            self.bytes[&file.url].clone()
        };
        out.write_all(&bytes).unwrap();
        progress(bytes.len() as u64);
        if let Some(path) = &self.collision {
            if file.project_id == "B" {
                fs::write(path, b"external content").unwrap();
            }
        }
        Ok(())
    }
}
fn temp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "xumob-modrinth-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn context() -> Context {
    Context {
        game_root: "C:/fixture".into(),
        version_id: "fabric-1.20.1".into(),
        game_dir: temp(),
        minecraft: "1.20.1".into(),
        loader: "fabric".into(),
    }
}
fn add(mock: &mut Mock, project: &str, id: &str, kind: &str) -> Value {
    mock.projects.insert(project.into(),json!({"id":project,"project_type":kind,"title":format!("Project {project}"),"client_side":"required"}));
    let bytes = format!("fixture bytes for {id}").into_bytes();
    let url = format!("https://cdn.modrinth.com/data/{project}/versions/{id}/file.jar");
    let version = json!({"id":id,"project_id":project,"game_versions":["1.20.1"],"loaders":if kind=="mod"{vec!["fabric"]}else if kind=="resourcepack"{vec!["minecraft"]}else{vec!["iris"]},"version_type":"release","date_published":"2025-01-01T00:00:00Z","dependencies":[],"files":[{"primary":true,"filename":format!("{project}-{}.{}",id,if kind=="mod"{"jar"}else{"zip"}),"url":url,"size":bytes.len(),"hashes":{"sha512":hex::encode(Sha512::digest(&bytes)),"sha1":hex::encode(Sha1::digest(&bytes))}}]});
    mock.bytes.insert(url, bytes);
    mock.versions.insert(id.into(), version.clone());
    version
}
fn fixture() -> Mock {
    let mut mock = Mock::default();
    let mut root = add(&mut mock, "A", "A1", "mod");
    add(&mut mock, "B", "B1", "mod");
    root["dependencies"] = json!([{"project_id":"B","version_id":null,"dependency_type":"required"},{"project_id":"NoOptional","dependency_type":"optional"},{"project_id":"Embedded","dependency_type":"embedded"}]);
    mock.versions.insert("A1".into(), root);
    mock
}
#[test]
fn filenames_urls_and_types_stay_inside_official_content_paths() {
    for name in [
        "../a.jar",
        "x\\a.jar",
        "C:evil.jar",
        "CON.jar",
        "aux.zip",
        "file.jar ",
        "test.exe",
        "a?.jar",
    ] {
        assert!(safe_filename(name, "mod").is_err());
    }
    assert!(safe_filename("中文模组-1.0.jar", "mod").is_ok());
    for url in [
        "http://cdn.modrinth.com/data/a.jar",
        "https://evil.test/data/a.jar",
        "https://cdn.modrinth.com@evil.test/data/a.jar",
        "https://cdn.modrinth.com:9443/data/a.jar",
        "https://cdn.modrinth.com/other/a.jar",
    ] {
        assert!(cdn_url(url).is_err());
    }
    assert!(kind_folder("modpack").is_err());
    assert!(project_url("../bad", "mod").is_err());
}
#[test]
fn cancellation_on_initial_download_state_is_not_lost() {
    let manager = Manager::with_transport(Arc::new(fixture()));
    let ctx = context();
    let plan = manager.prepare("A", "A1", "mod", ctx.clone()).unwrap();
    let mut service = Service::new(temp()).unwrap();
    Arc::get_mut(&mut service).unwrap().modrinth = manager.clone();
    let cancel = manager.clone();
    service.set_sink(Arc::new(move |event, value| {
        if event == "state" && value["phase"] == "downloading" {
            cancel.cancel();
        }
    }));
    service.begin("downloading", "starting").unwrap();
    assert_eq!(
        manager.install(field(&plan, "planId"), ctx.clone(), &mut |_| {}),
        Err("下载已取消。".into())
    );
    assert!(!ctx.game_dir.join("mods/A-A1.jar").exists());
    service.end();
}
#[test]
fn search_uses_instance_version_loader_and_type_facets() {
    let mock = Arc::new(Mock::default());
    let manager = Manager::with_transport(mock.clone());
    manager
        .search("test", "mod", 20, "downloads", Some(&context()))
        .unwrap();
    let calls = mock.calls.lock().unwrap();
    let query = &calls[0].1;
    let facets: Value =
        serde_json::from_str(&query.iter().find(|p| p.0 == "facets").unwrap().1).unwrap();
    assert_eq!(
        facets,
        json!([
            ["project_type:mod"],
            ["versions:1.20.1"],
            ["categories:fabric"]
        ])
    );
    assert!(query.contains(&("offset".into(), "20".into())));
}
#[test]
fn required_dependencies_are_resolved_and_install_is_idempotent() {
    let manager = Manager::with_transport(Arc::new(fixture()));
    let ctx = context();
    let plan = manager.prepare("A", "A1", "mod", ctx.clone()).unwrap();
    assert_eq!(plan["dependencyCount"], 1);
    assert_eq!(items(&plan["files"]).len(), 2);
    assert!(!plan.to_string().contains("https://"));
    let installed = manager
        .install(field(&plan, "planId"), ctx.clone(), &mut |_| {})
        .unwrap();
    assert_eq!(installed["installed"], 2);
    assert!(ctx.game_dir.join("mods/A-A1.jar").is_file());
    assert!(ctx.game_dir.join("mods/B-B1.jar").is_file());
    let plan = manager.prepare("A", "A1", "mod", ctx.clone()).unwrap();
    let again = manager
        .install(field(&plan, "planId"), ctx, &mut |_| {})
        .unwrap();
    assert_eq!(again["installed"], 0);
    assert_eq!(again["alreadyInstalled"], 2);
}
#[test]
fn hash_failure_and_cancel_leave_no_partial_files() {
    for cancel in [false, true] {
        let mut mock = fixture();
        mock.corrupt = !cancel;
        mock.cancel = cancel;
        let manager = Manager::with_transport(Arc::new(mock));
        let ctx = context();
        let plan = manager.prepare("A", "A1", "mod", ctx.clone()).unwrap();
        assert!(manager
            .install(field(&plan, "planId"), ctx.clone(), &mut |_| {})
            .is_err());
        assert_eq!(fs::read_dir(ctx.game_dir.join("mods")).unwrap().count(), 0);
    }
}
#[test]
fn commit_collision_rolls_back_new_files_without_removing_existing_data() {
    let ctx = context();
    let collision = ctx.game_dir.join("mods/B-B1.jar");
    let mut mock = fixture();
    mock.collision = Some(collision.clone());
    let manager = Manager::with_transport(Arc::new(mock));
    let plan = manager.prepare("A", "A1", "mod", ctx.clone()).unwrap();
    assert!(manager
        .install(field(&plan, "planId"), ctx.clone(), &mut |_| {})
        .is_err());
    assert!(!ctx.game_dir.join("mods/A-A1.jar").exists());
    assert_eq!(fs::read(collision).unwrap(), b"external content");
    assert_eq!(fs::read_dir(ctx.game_dir.join("mods")).unwrap().count(), 1);
}
#[test]
fn changed_instance_and_incompatible_versions_are_rejected() {
    let manager = Manager::with_transport(Arc::new(fixture()));
    let ctx = context();
    let plan = manager.prepare("A", "A1", "mod", ctx.clone()).unwrap();
    let mut other = ctx.clone();
    other.loader = "forge".into();
    assert!(manager
        .install(field(&plan, "planId"), other.clone(), &mut |_| {})
        .unwrap_err()
        .contains("已改变"));
    assert!(manager.prepare("A", "A1", "mod", other).is_err());
    assert!(manager.prepare("B", "A1", "mod", ctx.clone()).is_err());
    let mut vanilla = ctx;
    vanilla.loader = "minecraft".into();
    assert!(manager
        .prepare("A", "A1", "mod", vanilla)
        .unwrap_err()
        .contains("加载器"));
}
#[test]
fn conflicting_installed_project_is_not_duplicated() {
    let mut mock = fixture();
    let old = add(&mut mock, "B", "OldB", "mod");
    let ctx = context();
    fs::create_dir_all(ctx.game_dir.join("mods")).unwrap();
    let bytes = b"already installed old dependency";
    fs::write(ctx.game_dir.join("mods/old-b.jar"), bytes).unwrap();
    mock.known.insert(hex::encode(Sha1::digest(bytes)), old);
    let mut root = mock.versions["A1"].clone();
    root["dependencies"] =
        json!([{"project_id":"B","version_id":"B1","dependency_type":"required"}]);
    mock.versions.insert("A1".into(), root);
    let manager = Manager::with_transport(Arc::new(mock));
    assert!(manager
        .prepare("A", "A1", "mod", ctx)
        .unwrap_err()
        .contains("old-b.jar"));
}
#[test]
fn declared_incompatible_dependencies_block_installation() {
    let mut mock = fixture();
    let root = mock.versions.get_mut("A1").unwrap();
    root["dependencies"]
        .as_array_mut()
        .unwrap()
        .push(json!({"project_id":"B","dependency_type":"incompatible"}));
    let manager = Manager::with_transport(Arc::new(mock));
    assert!(manager
        .prepare("A", "A1", "mod", context())
        .unwrap_err()
        .contains("不兼容"));
}
#[test]
fn resource_packs_and_shaders_install_into_their_own_folders() {
    for (kind, folder) in [("resourcepack", "resourcepacks"), ("shader", "shaderpacks")] {
        let mut mock = Mock::default();
        add(&mut mock, "P", "P1", kind);
        let manager = Manager::with_transport(Arc::new(mock));
        let ctx = context();
        let plan = manager.prepare("P", "P1", kind, ctx.clone()).unwrap();
        manager
            .install(field(&plan, "planId"), ctx.clone(), &mut |_| {})
            .unwrap();
        assert!(ctx.game_dir.join(folder).join("P-P1.zip").is_file());
        assert!(!ctx.game_dir.join("mods").exists());
    }
}
