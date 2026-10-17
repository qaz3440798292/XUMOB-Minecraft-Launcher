//! Minecraft and modpack installation into a new, isolated instance.
use crate::{
    core::{self, Service, R},
    curseforge::{a, s},
    modrinth::Context,
    net,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
static SERIAL: AtomicU64 = AtomicU64::new(1);
fn is_april_fools_version(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    matches!(
        id.as_str(),
        "14w14a"
            | "15w14a"
            | "1.rv-pre1"
            | "20w14infinite"
            | "20w14∞"
            | "22w13oneblockatatime"
            | "23w13a_or_b"
            | "24w14potato"
            | "25w14craftmine"
            | "26w14a"
    ) || id.starts_with("2.0.")
        || id == "2.0"
        || id.contains("shareware")
}
fn fabric_game_supported(versions: &Value, minecraft: &str) -> bool {
    a(versions).iter().any(|version| s(version, "version") == minecraft)
}
fn optifine_forge_requirement(requirement: &str, versions: &[Value]) -> String {
    let raw = requirement
        .strip_prefix("Forge ")
        .unwrap_or(requirement)
        .trim();
    let build = raw.strip_prefix('#').unwrap_or(raw);
    if !build.is_empty() && build.chars().all(|character| character.is_ascii_digit()) {
        let mut matches = versions
            .iter()
            .map(|version| s(version, "id"))
            .filter(|version| version.rsplit(['.', '-']).next() == Some(build));
        if let Some(version) = matches.next() {
            if matches.next().is_none() {
                return version.to_owned();
            }
        }
    }
    raw.to_owned()
}
#[cfg(test)]
mod fabric_support_tests {
    use super::*;

    #[test]
    fn detects_supported_and_unsupported_game_versions() {
        let versions = json!([
            {"version":"1.14","stable":true},
            {"version":"1.20.1","stable":true}
        ]);
        assert!(fabric_game_supported(&versions, "1.14"));
        assert!(!fabric_game_supported(&versions, "1.12.2"));
    }

    #[test]
    fn resolves_optifine_forge_build_number() {
        let versions = vec![
            json!({"id":"14.23.5.2847"}),
            json!({"id":"14.23.5.2864"}),
        ];
        assert_eq!(
            optifine_forge_requirement("#2847", &versions),
            "14.23.5.2847"
        );
    }
}
#[cfg(test)]
#[path = "pack_download_tests.rs"]
mod pack_download_tests;
pub struct Manager {
    cancel: AtomicBool,
    tickets: Mutex<HashMap<String, Ticket>>,
    cache: Mutex<HashMap<String, (Instant, Value)>>,
}
#[derive(Clone)]
struct Ticket {
    request: Value,
    metadata: Value,
    pack: Option<(PathBuf, Value, String)>,
    pack_info: Option<Value>,
    modify_version: bool,
    created: Instant,
}
struct PackDownloadGroup {
    urls: Vec<String>,
    sha1: String,
    sha512: String,
    size: u64,
    key: Option<String>,
    cache: PathBuf,
    destinations: Vec<PathBuf>,
}
impl Manager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            cancel: AtomicBool::new(false),
            tickets: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
        })
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed)
    }
    fn check(&self) -> R<()> {
        if self.cancel.load(Ordering::Relaxed) {
            Err("下载已取消。".into())
        } else {
            Ok(())
        }
    }
    fn cached(&self, url: &str) -> R<Value> {
        if let Some((t, v)) = self.cache.lock().unwrap().get(url) {
            if t.elapsed() < Duration::from_secs(300) {
                return Ok(v.clone());
            }
        }
        let v = net::json(url)?;
        let mut cache = self.cache.lock().unwrap();
        let bytes = serde_json::to_vec(&v).map_err(|_| "组件列表无效。")?.len();
        let used = cache
            .values()
            .map(|(_, v)| serde_json::to_vec(v).map(|v| v.len()).unwrap_or(0))
            .sum::<usize>();
        if cache.len() > 24 || used + bytes > 4 * 1024 * 1024 {
            cache.clear()
        }
        if bytes <= 4 * 1024 * 1024 {
            cache.insert(url.into(), (Instant::now(), v.clone()));
        }
        Ok(v)
    }
    fn manifest(&self) -> R<Value> {
        self.cached("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
    }
    pub fn catalog(&self) -> R<Value> {
        let v = self.manifest()?;
        Ok(json!({
            "latest": v["latest"],
            "versions": a(&v["versions"])
                .iter()
                .map(|version| {
                    let id = version["id"].as_str().unwrap_or_default();
                    let manifest_type = version["type"].as_str().unwrap_or_default();
                    let kind = if is_april_fools_version(id) {
                        "april_fools"
                    } else {
                        manifest_type
                    };
                    json!({"id": id, "type": kind, "date": version["releaseTime"]})
                })
                .collect::<Vec<_>>()
        }))
    }
    pub fn inspect_pack(&self, path: &str) -> R<Value> {
        let path = Path::new(path)
            .canonicalize()
            .map_err(|_| "无法读取这个整合包文件。")?;
        if !path.is_file()
            || !matches!(
                path.extension()
                    .and_then(|value| value.to_str())
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("zip" | "mrpack")
            )
        {
            return Err("请选择 ZIP 或 MRPACK 格式的整合包。".into());
        }
        let size = fs::metadata(&path)
            .map_err(|_| "无法检查整合包文件。")?
            .len();
        if size == 0 || size > 512 * 1024 * 1024 {
            return Err("整合包文件为空或超过 512 MB 限制。".into());
        }
        let (provider, index) = match read_pack(&path, "modrinth") {
            Ok(index) => ("modrinth", index),
            Err(_) => ("curseforge", read_pack(&path, "curseforge")?),
        };
        let (minecraft, loader, loader_version) = pack_game(&index, provider)?;
        simple(&minecraft)?;
        let title = if provider == "modrinth" {
            s(&index, "name")
        } else {
            s(&index, "name")
        };
        let title = if title.trim().is_empty() {
            path.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("我的整合包")
        } else {
            title
        };
        let icon = archive_pack_icon(&path, &index);
        Ok(json!({
            "provider": provider,
            "title": title,
            "iconData": icon,
            "minecraft": minecraft,
            "loader": loader,
            "loaderVersion": loader_version,
            "optionalCount": if provider == "modrinth" { a(&index["files"]).iter().filter(|file| file["env"]["client"] == "optional").count() } else { a(&index["files"]).iter().filter(|file| file["required"] == false).count() },
        }))
    }
    fn metadata(&self, id: &str) -> R<Value> {
        let v = self.manifest()?;
        let entry = a(&v["versions"])
            .iter()
            .find(|v| s(v, "id") == id)
            .ok_or("找不到这个 Minecraft 版本。")?;
        let bytes = net::bytes(s(entry, "url"), 8 * 1024 * 1024)?;
        use sha1::{Digest, Sha1};
        if hex::encode(Sha1::digest(&bytes)) != s(entry, "sha1") {
            return Err("Minecraft 版本配置校验失败。".into());
        }
        serde_json::from_slice(&bytes).map_err(|_| "Minecraft 版本配置无效。".into())
    }
    pub fn components(&self, mc: &str) -> R<Value> {
        simple(mc)?;
        let groups = std::thread::scope(|scope| {
            ["fabric", "forge", "neoforge", "optifine", "fabricApi"]
                .into_iter()
                .map(|kind| {
                    scope.spawn(move || {
                        let result = self.component(mc, kind);
                        (
                            kind,
                            match result {
                                Ok(versions) => json!({"versions":versions}),
                                Err(error) => json!({
                                    "versions": [],
                                    "error": error,
                                    "unsupported": kind == "fabric" && error.starts_with("Fabric 暂不支持 Minecraft ")
                                }),
                            },
                        )
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect::<Vec<_>>()
        });
        let mut value = serde_json::Map::new();
        for (kind, v) in groups {
            value.insert(kind.into(), v);
        }
        Ok(Value::Object(value))
    }
    pub fn version_info(&self, service: &Service, id: &str) -> R<Value> {
        let version = service
            .versions()
            .into_iter()
            .find(|item| s(item, "id") == id)
            .ok_or("找不到这个游戏版本。")?;
        if version.get("error").is_some() || version["supported"] != true {
            return Err("此客户端配置异常，暂时不能修改加载器。".into());
        }
        let minecraft = s(&version, "minecraft");
        Ok(json!({"version": version, "components": self.components(minecraft)?}))
    }
    fn component(&self, mc: &str, kind: &str) -> R<Vec<Value>> {
        simple(mc)?;
        match kind{
        "fabric"=>{
            let supported = self.cached("https://meta.fabricmc.net/v2/versions/game")?;
            if !fabric_game_supported(&supported, mc) {
                return Err(format!("Fabric 暂不支持 Minecraft {mc}，无法获取加载器版本。"));
            }
            Ok(a(&self.cached(&format!("https://meta.fabricmc.net/v2/versions/loader/{mc}"))?).iter().map(|v|json!({"id":v["loader"]["version"],"name":v["loader"]["version"],"stable":v["loader"]["stable"]})).collect())
        },
        "forge"=>{let text=String::from_utf8(net::bytes("https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml",8*1024*1024)?).map_err(|_|"Forge 版本列表无效。")?;let prefix=format!("{mc}-");let mut values=xml_versions(&text).into_iter().filter(|v|v.starts_with(&prefix)).map(|v|{let id=v[prefix.len()..].to_owned();json!({"id":id,"name":id,"stable":!id.contains("beta")})}).collect::<Vec<_>>();values.sort_by(|a,b|version_key(s(b,"id")).cmp(&version_key(s(a,"id"))));Ok(values)},
        "neoforge"=>{let old=mc=="1.20.1";let group=if old{"forge"}else{"neoforge"};let text=String::from_utf8(net::bytes(&format!("https://maven.neoforged.net/releases/net/neoforged/{group}/maven-metadata.xml"),8*1024*1024)?).map_err(|_|"NeoForge 版本列表无效。")?;let p=mc.split('.').collect::<Vec<_>>();let prefix=if old{format!("{mc}-")}else if p.first()==Some(&"1"){format!("{}.{}.",p.get(1).unwrap_or(&"0"),p.get(2).unwrap_or(&"0"))}else{format!("{mc}.")};let mut values=xml_versions(&text).into_iter().filter(|v|v.starts_with(&prefix)).map(|id|json!({"id":id,"name":id,"stable":!id.contains("beta")})).collect::<Vec<_>>();values.sort_by(|a,b|version_key(s(b,"id")).cmp(&version_key(s(a,"id"))));Ok(values)},
        "optifine"=>{let mut values=a(&self.cached(&format!("https://bmclapi2.bangbang93.com/optifine/{mc}"))?).iter().filter(|v|s(v,"mcversion")==mc).map(|v|json!({"id":format!("{}/{}",s(v,"type"),s(v,"patch")),"name":format!("{} {}",s(v,"type"),s(v,"patch")),"stable":!s(v,"patch").starts_with("pre"),"forge":s(v,"forge").strip_prefix("Forge ").unwrap_or("")})).collect::<Vec<_>>();values.sort_by(|a,b|b["stable"].as_bool().cmp(&a["stable"].as_bool()).then_with(||s(b,"name").cmp(s(a,"name"))));Ok(values)},
        "fabricApi"=>{let url=format!("https://api.modrinth.com/v2/project/P7dR8mSH/version?game_versions={}&loaders=%5B%22fabric%22%5D",encoded(&json!([mc]).to_string()));Ok(a(&self.cached(&url)?).iter().map(|v|json!({"id":v["id"],"name":v["version_number"],"stable":v["version_type"]=="release"})).collect())},
        _=>Err("组件类型无效。".into())}
    }
    pub fn prepare(&self, service: &Service, input: &Value, provider: &str) -> R<Value> {
        service.ensure_account_editable()?;
        let mut request = input.clone();
        let modify_version = input["modifyVersion"].as_bool().unwrap_or(false);
        if s(&request, "gameRoot").is_empty() {
            request["gameRoot"] = service.settings()["gameRoot"].clone()
        }
        if modify_version {
            request["gameRoot"] = service.settings()["gameRoot"].clone();
            validate_existing_target(&request)?;
            let existing = service
                .versions()
                .into_iter()
                .find(|item| s(item, "id") == s(&request, "name"))
                .ok_or("无法确定此版本的游戏数据目录。")?;
            request["instanceGameDir"] = existing["gameDir"].clone();
            request["removeOptifineSidecar"] = json!(s(&existing, "loader")
                .to_ascii_lowercase()
                .contains("optifine"));
            validate_existing_target(&request)?;
        } else {
            validate_target(&request)?;
        }
        let mut pack = None;
        let mut pack_info = None;
        if !s(input, "projectId").is_empty() {
            let content = if provider == "curseforge" {
                &service.curseforge.content
            } else if provider == "modrinth" {
                &service.modrinth
            } else {
                return Err("下载平台无效。".into());
            };
            let project = content.project(s(input, "projectId"))?;
            if s(&project, "project_type") != "modpack" {
                return Err("这个项目不是整合包。".into());
            }
            let version = content.raw_version(s(input, "versionId"))?;
            if s(&version, "project_id") != s(input, "projectId") {
                return Err("整合包版本不属于该项目。".into());
            }
            let f = a(&version["files"])
                .iter()
                .find(|v| v["primary"] == true)
                .or_else(|| a(&version["files"]).first())
                .ok_or("整合包没有下载文件。")?;
            let dir = service.data_dir.join("install-cache");
            fs::create_dir_all(&dir).map_err(|_| "无法创建整合包缓存。")?;
            let path = dir.join(format!(
                "{provider}-{}-{}.zip",
                s(input, "projectId"),
                s(input, "versionId")
            ));
            let size = f["size"]
                .as_u64()
                .filter(|n| *n > 0 && *n <= 512 * 1024 * 1024)
                .ok_or("整合包文件大小不受支持。")?;
            let key = if provider == "curseforge" {
                Some(service.curseforge.vault.get()?)
            } else {
                None
            };
            if s(f, "url").is_empty() {
                return Err("作者未允许第三方启动器下载这个整合包，请前往平台页面。".into());
            }
            net::fetch(
                s(f, "url"),
                &path,
                s(&f["hashes"], "sha1"),
                s(&f["hashes"], "sha512"),
                size,
                &AtomicBool::new(false),
                key.as_deref(),
                &mut |_| {},
            )?;
            let mut index = read_pack(&path, provider)?;
            if provider == "curseforge" {
                index["_xumobResolved"] = json!(service
                    .curseforge
                    .pack_entries(&index, input["optional"].as_bool().unwrap_or(false))?);
            }
            let (mc, loader, version) = pack_game(&index, provider)?;
            request["minecraft"] = json!(mc);
            request["loader"] = json!(loader);
            request["loaderVersion"] = json!(version);
            request["optifine"] = json!("");
            request["fabricApi"] = json!("");
            pack = Some((path, index, provider.into()));
            let icon = if provider == "curseforge" {
                crate::curseforge::safe_icon_url(s(&project, "icon_url"))
            } else {
                crate::modrinth::icon_url(s(&project, "icon_url"))
            };
            pack_info = Some(json!({
                "provider": provider,
                "projectId": s(input, "projectId"),
                "title": project["title"],
                "iconData": icon,
            }));
        } else if !s(input, "archivePath").is_empty() {
            let path = Path::new(s(input, "archivePath"))
                .canonicalize()
                .map_err(|_| "无法读取这个整合包文件。")?;
            let inspected = self.inspect_pack(&path.to_string_lossy())?;
            let pack_provider = s(&inspected, "provider");
            let mut index = read_pack(&path, pack_provider)?;
            if pack_provider == "curseforge" {
                index["_xumobResolved"] = json!(service
                    .curseforge
                    .pack_entries(&index, input["optional"].as_bool().unwrap_or(false))?);
            }
            let (mc, loader, version) = pack_game(&index, pack_provider)?;
            request["minecraft"] = json!(mc);
            request["loader"] = json!(loader);
            request["loaderVersion"] = json!(version);
            request["optifine"] = json!("");
            request["fabricApi"] = json!("");
            pack_info = Some(json!({
                "provider": pack_provider,
                "title": inspected["title"],
                "local": true,
            }));
            pack = Some((path, index, pack_provider.into()));
        }
        let mc = s(&request, "minecraft");
        simple(mc)?;
        let metadata = self.metadata(mc)?;
        let loader = s(&request, "loader");
        if !["vanilla", "fabric", "forge", "neoforge", "quilt"].contains(&loader) {
            return Err("请选择一个支持的加载器。".into());
        }
        if loader != "vanilla" {
            simple(s(&request, "loaderVersion"))?;
            if pack.is_none()
                && !self
                    .component(mc, loader)?
                    .iter()
                    .any(|v| s(v, "id") == s(&request, "loaderVersion"))
            {
                return Err("所选加载器版本不可用于这个 Minecraft 版本。".into());
            }
        }
        if !s(&request, "optifine").is_empty() {
            if !["vanilla", "forge"].contains(&loader) {
                return Err("OptiFine 不能与 Fabric 或 NeoForge 同时安装。".into());
            }
            let components = self.component(mc, "optifine")?;
            let of = components
                .iter()
                .find(|v| s(v, "id") == s(&request, "optifine"))
                .ok_or("OptiFine 版本不可用。")?;
            let required_forge = if loader == "forge" {
                optifine_forge_requirement(&s(of, "forge"), &self.component(mc, "forge")?)
            } else {
                String::new()
            };
            if loader == "forge" && required_forge != s(&request, "loaderVersion") {
                return Err(format!(
                    "这个 OptiFine 版本需要 Forge {}，请调整加载器版本。",
                    required_forge
                ));
            }
        }
        if !s(&request, "fabricApi").is_empty() {
            if loader != "fabric" {
                return Err("Fabric API 需要先选择 Fabric。".into());
            }
            if !self
                .component(mc, "fabricApi")?
                .iter()
                .any(|v| s(v, "id") == s(&request, "fabricApi"))
            {
                return Err("Fabric API 版本与游戏版本不匹配。".into());
            }
        }
        // A loader installer runs with the Java required by the selected game.
        let java = service.select_java(&metadata)?;
        let ticket = Ticket {
            request: request.clone(),
            metadata,
            pack,
            pack_info,
            modify_version,
            created: Instant::now(),
        };
        let id = format!(
            "game-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        );
        let summary = json!({"planId":id,"name":request["name"],"minecraft":mc,"loader":loader,"loaderVersion":request["loaderVersion"],"optifine":request["optifine"],"fabricApi":request["fabricApi"],"gameRoot":request["gameRoot"],"directory":Path::new(s(&request,"gameRoot")).join("versions").join(s(&request,"name")),"java":java["version"],"isPack":ticket.pack.is_some()});
        let mut tickets = self.tickets.lock().unwrap();
        tickets.retain(|_, t| t.created.elapsed() < Duration::from_secs(1800));
        // Only one installation preview is active in the interface.
        tickets.clear();
        tickets.insert(id, ticket);
        Ok(summary)
    }
    pub fn install(&self, service: &Arc<Service>, id: &str) -> R<Value> {
        let ticket = self
            .tickets
            .lock()
            .unwrap()
            .remove(id)
            .ok_or("安装清单已过期，请重新确认。")?;
        if ticket.created.elapsed() > Duration::from_secs(1800) {
            return Err("安装清单已过期。".into());
        }
        self.cancel.store(false, Ordering::Relaxed);
        service.begin("downloading", "正在安装游戏…")?;
        let result = self.run(service, &ticket);
        service.end();
        match &result {
            Ok(v) => {
                if !ticket.modify_version {
                    if let Err(error)=service.save(json!({"gameRoot":ticket.request["gameRoot"],"versionId":ticket.request["name"],"isolated":true})){
                    let error=format!("游戏已安装，但启动器设置未保存：{error}");service.state_to("error",&error,None);service.log(&error,"error");return Err(error)
                  }
                }
                let message = if ticket.modify_version {
                    "客户端加载器已更新"
                } else {
                    "新游戏实例已安装完成"
                };
                service.state_to("idle", message, None);
                service.log(&format!("{message}：{}", v["versionId"]), "info");
            }
            Err(e) => {
                service.state_to(
                    if e == "下载已取消。" {
                        "idle"
                    } else {
                        "error"
                    },
                    e,
                    None,
                );
                service.log(e, "error");
            }
        }
        result
    }
    fn run(&self, service: &Arc<Service>, ticket: &Ticket) -> R<Value> {
        let req = &ticket.request;
        if ticket.modify_version {
            validate_existing_target(req)?;
        } else {
            validate_target(req)?;
        }
        self.check()?;
        let root = PathBuf::from(s(req, "gameRoot"));
        fs::create_dir_all(&root).map_err(|_| "无法创建游戏目录。")?;
        let root = fs::canonicalize(&root).map_err(|_| "无法读取游戏目录。")?;
        let stage = root.join(format!(
            ".xumob-install-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&stage).map_err(|_| "无法创建安装暂存目录。")?;
        let _cleanup = Cleanup(stage.clone());
        let mc = s(req, "minecraft");
        let base = stage.join("versions").join(mc);
        fs::create_dir_all(&base).map_err(|_| "无法创建版本目录。")?;
        write_json(&base.join(format!("{mc}.json")), &ticket.metadata)?;
        write_json(
            &stage.join("launcher_profiles.json"),
            &json!({"profiles":{}}),
        )?;
        let scratch = Service::new(stage.join(".launcher-state"))?;
        scratch.save(
            json!({"gameRoot":stage,"versionId":mc,"isolated":true,"username":"Installer"}),
        )?;
        self.complete_files(service, &scratch, &root, &stage)?;
        let loader = s(req, "loader");
        let lv = s(req, "loaderVersion");
        let mut selected = mc.to_string();
        if loader == "fabric" || loader == "quilt" {
            self.check()?;
            service.state_to("downloading", "正在配置模组加载器…", None);
            let api = if loader == "fabric" {
                "https://meta.fabricmc.net/v2"
            } else {
                "https://meta.quiltmc.org/v3"
            };
            let mut profile = net::json(&format!("{api}/versions/loader/{mc}/{lv}/profile/json"))?;
            selected = s(&profile, "id").to_owned();
            simple(&selected)?;
            profile["inheritsFrom"] = json!(mc);
            let dir = stage.join("versions").join(&selected);
            fs::create_dir_all(&dir).map_err(|_| "无法创建加载器目录。")?;
            write_json(&dir.join(format!("{selected}.json")), &profile)?;
        }
        if loader == "forge" || loader == "neoforge" {
            let coordinate = if loader == "forge" {
                format!("net/minecraftforge/forge/{mc}-{lv}/forge-{mc}-{lv}-installer.jar")
            } else if mc == "1.20.1" {
                format!("net/neoforged/forge/{lv}/forge-{lv}-installer.jar")
            } else {
                format!("net/neoforged/neoforge/{lv}/neoforge-{lv}-installer.jar")
            };
            let maven = if loader == "forge" {
                "https://maven.minecraftforge.net"
            } else {
                "https://maven.neoforged.net/releases"
            };
            let url = format!("{maven}/{coordinate}");
            let sha = sidecar(&url)?;
            let installer = stage.join("loader-installer.jar");
            net::fetch(
                &url,
                &installer,
                &sha,
                "",
                0,
                &self.cancel,
                None,
                &mut |_| {},
            )?;
            let profile = zip_json(&installer, "install_profile.json")?;
            if profile["versionInfo"].is_object() {
                selected = install_legacy_forge(&installer, &profile, &stage, mc)?;
            } else {
                selected = s(&profile, "version").to_owned();
                simple(&selected)?;
                let java = service.select_java(&ticket.metadata)?;
                let mut command = Command::new(s(&java, "path"));
                command
                    .args(["-Djava.awt.headless=true", "-jar"])
                    .arg(java_compatible_path(&installer))
                    .arg("--installClient")
                    .arg(java_compatible_path(&stage));
                self.process(
                    service,
                    command,
                    &stage,
                    &format!("正在安装 {loader}，处理依赖可能需要数分钟…"),
                )?;
                if !stage
                    .join("versions")
                    .join(&selected)
                    .join(format!("{selected}.json"))
                    .is_file()
                {
                    return Err("加载器安装器没有生成完整的版本配置。".into());
                }
                verify_processor_outputs(&profile, &stage)?;
            }
        }
        if !s(req, "optifine").is_empty() {
            self.check()?;
            service.state_to("downloading", "正在安装 OptiFine…", None);
            let pair = s(req, "optifine").split('/').collect::<Vec<_>>();
            if pair.len() != 2 {
                return Err("OptiFine 版本无效。".into());
            }
            for part in &pair {
                simple(part)?
            }
            let installer = stage.join("OptiFine.jar");
            let filename = optifine_filename(mc, pair[0], pair[1]);
            let official_page = format!("https://optifine.net/adloadx?f={filename}");
            let mirror = format!(
                "https://bmclapi2.bangbang93.com/optifine/{mc}/{}/{}",
                pair[0], pair[1]
            );
            let mut sources = vec![mirror];
            let mut source_errors = Vec::new();
            match resolve_optifine_download_url(&official_page, &filename) {
                Ok(url) => sources.push(url),
                Err(error) => source_errors.push(format!("官方地址解析失败：{error}")),
            }
            download_optifine(&sources, &installer, &self.cancel, source_errors)?;
            if loader == "forge" {
                let mods = stage.join("versions").join(&selected).join("mods");
                fs::create_dir_all(&mods).map_err(|_| "无法创建模组目录。")?;
                fs::copy(
                    &installer,
                    mods.join(format!("OptiFine_{mc}_{}_{}.jar", pair[0], pair[1])),
                )
                .map_err(|_| "无法安装 OptiFine 模组。")?;
            } else {
                fs::write(
                    stage.join("XumobOptiFineInstall.class"),
                    include_bytes!("../helpers/XumobOptiFineInstall.class"),
                )
                .map_err(|_| "无法准备 OptiFine 安装程序。")?;
                let java = service.select_java(&ticket.metadata)?;
                let mut command = Command::new(s(&java, "path"));
                command
                    .args(["-Djava.awt.headless=true", "-cp"])
                    .arg(format!(
                        "{};{}",
                        java_compatible_path(&stage).display(),
                        java_compatible_path(&installer).display()
                    ))
                    .arg("XumobOptiFineInstall")
                    .arg(java_compatible_path(&stage))
                    .arg(mc);
                self.process(service, command, &stage, "正在应用 OptiFine 补丁…")?;
                selected = fs::read_dir(stage.join("versions"))
                    .map_err(|_| "无法读取安装结果。")?
                    .filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .find(|id| id.contains("OptiFine"))
                    .ok_or("OptiFine 未生成游戏版本。")?;
            }
        }
        scratch.save(json!({"versionId":selected}))?;
        self.complete_files(service, &scratch, &root, &stage)?;
        let name = s(req, "name");
        let final_dir = stage.join("final-instance");
        fs::create_dir(&final_dir).map_err(|_| "无法创建最终实例。")?;
        // Copy loader-side mods (OptiFine for Forge) into the isolated final instance.
        let generated = stage.join("versions").join(&selected);
        if generated.join("mods").is_dir() {
            copy_tree(&generated.join("mods"), &final_dir.join("mods"), false)?;
        }
        if !s(req, "fabricApi").is_empty() {
            let version = service.modrinth.raw_version(s(req, "fabricApi"))?;
            let f = a(&version["files"])
                .iter()
                .find(|v| v["primary"] == true)
                .or_else(|| a(&version["files"]).first())
                .ok_or("Fabric API 文件不存在。")?;
            let path = safe_relative(s(f, "filename"))?;
            net::fetch(
                s(f, "url"),
                &final_dir.join("mods").join(path),
                s(&f["hashes"], "sha1"),
                s(&f["hashes"], "sha512"),
                f["size"].as_u64().unwrap_or(0),
                &self.cancel,
                None,
                &mut |_| {},
            )?;
        }
        if let Some((archive, index, provider)) = &ticket.pack {
            self.pack_files(
                service,
                archive,
                index,
                provider,
                &final_dir,
                req["optional"].as_bool().unwrap_or(false),
            )?;
        }
        let mut version = core::resolve_version(&stage, &selected)?;
        let jar = s(&version, "jarId").to_owned();
        version.as_object_mut().unwrap().remove("inheritsFrom");
        version["id"] = json!(name);
        version["jar"] = json!(name);
        version["minecraftVersion"] = json!(mc);
        version["clientVersion"] = json!(mc);
        write_json(&final_dir.join(format!("{name}.json")), &version)?;
        if let Some(pack_info) = &ticket.pack_info {
            write_json(&final_dir.join(".xumob-pack.json"), pack_info)?;
        }
        fs::copy(
            stage.join("versions").join(&jar).join(format!("{jar}.jar")),
            final_dir.join(format!("{name}.jar")),
        )
        .map_err(|_| "无法保存 Minecraft 游戏文件。")?;
        self.check()?;
        service.state_to("downloading", "正在保存已校验的游戏文件…", None);
        for folder in ["libraries", "assets"] {
            if stage.join(folder).is_dir() {
                copy_tree(&stage.join(folder), &root.join(folder), true)?
            }
        }
        let versions = root.join("versions");
        fs::create_dir_all(&versions).map_err(|_| "无法创建版本目录。")?;
        if !fs::canonicalize(&versions)
            .map_err(|_| "无法读取版本目录。")?
            .starts_with(&root)
        {
            return Err("版本目录指向游戏目录之外。".into());
        }
        let target = versions.join(name);
        self.check()?;
        if ticket.modify_version {
            let mods = PathBuf::from(s(req, "instanceGameDir")).join("mods");
            if final_dir.join("mods").is_dir() {
                copy_tree(&final_dir.join("mods"), &mods, true)?;
            }
            replace_instance_launch_files(&final_dir, &target, name)?;
            if req["removeOptifineSidecar"] == true {
                remove_optifine_sidecar(&mods, &final_dir.join("mods"));
            }
        } else {
            if target.exists() {
                return Err("同名游戏版本已存在，未覆盖任何实例。".into());
            }
            fs::rename(&final_dir, &target).map_err(|_| "无法保存新游戏实例。")?;
        }
        Ok(json!({"versionId":name,"directory":target,"minecraft":mc}))
    }
    fn complete_files(
        &self,
        service: &Service,
        scratch: &Service,
        reuse: &Path,
        stage: &Path,
    ) -> R<()> {
        for _ in 0..2 {
            self.check()?;
            let plan = scratch.build_plan()?;
            let mut files = plan
                .files
                .iter()
                .map(|f| json!({"file":f.file,"url":f.url,"sha1":f.sha1,"label":f.label}))
                .collect::<Vec<_>>();
            let inspection = scratch.inspect_files(&plan, true)?;
            files.extend(
                a(&inspection["missing"])
                    .iter()
                    .filter(|f| s(f, "label").starts_with("资源 "))
                    .cloned(),
            );
            let seen = Mutex::new(HashSet::new());
            files.retain(|f| seen.lock().unwrap().insert(s(f, "file").to_string()));
            let index = AtomicUsize::new(0);
            let done = AtomicUsize::new(0);
            let error = Mutex::new(None::<String>);
            std::thread::scope(|scope| {
                for _ in 0..4 {
                    let files = &files;
                    let index = &index;
                    let done = &done;
                    let error = &error;
                    scope.spawn(move||{loop{if error.lock().unwrap().is_some(){break}let i=index.fetch_add(1,Ordering::Relaxed);if i>=files.len(){break}let f=&files[i];let result=(||{self.check()?;let path=PathBuf::from(s(f,"file"));let sha=s(f,"sha1");if path.is_file()&&(sha.is_empty()||net::hash(&path,false)?.eq_ignore_ascii_case(sha)){return Ok(())}if !sha.is_empty(){if let Ok(relative)=path.strip_prefix(stage){let old=reuse.join(relative);if old.is_file()&&net::hash(&old,false)?.eq_ignore_ascii_case(sha){fs::create_dir_all(path.parent().unwrap()).map_err(|_|"无法创建缓存目录。")?;fs::copy(old,&path).map_err(|_|"无法复用已有游戏文件。")?;return Ok(())}}}let url=s(f,"url");if url.is_empty(){return Err(format!("安装器未生成必要文件：{}",s(f,"label")))}let sha=if sha.is_empty(){sidecar(url)?}else{sha.to_owned()};net::fetch(url,&path,&sha,"",0,&self.cancel,None,&mut |_|{})})();if let Err(e)=result{*error.lock().unwrap()=Some(e);break}let n=done.fetch_add(1,Ordering::Relaxed)+1;service.content_progress(json!({"filename":s(f,"label"),"bytes":n,"totalBytes":files.len(),"fileIndex":n,"fileCount":files.len()}));}});
                }
            });
            if let Some(error) = error.into_inner().unwrap() {
                return Err(error);
            }
        }
        Ok(())
    }
    fn process(
        &self,
        service: &Service,
        mut command: Command,
        stage: &Path,
        message: &str,
    ) -> R<()> {
        self.check()?;
        service.state_to("downloading", message, None);
        let log = stage.join("installer-output.log");
        let out = File::create(&log).map_err(|_| "无法创建安装日志。")?;
        command
            .env_remove("XUMOB_CURSEFORGE_API_KEY")
            .env_remove("XUMOB_BUILD_CURSEFORGE_API_KEY")
            .current_dir(java_compatible_path(stage))
            .stdout(Stdio::from(
                out.try_clone().map_err(|_| "无法创建安装日志。")?,
            ))
            .stderr(Stdio::from(out));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|_| "无法启动 Java 安装器。")?;
        let started = Instant::now();
        loop {
            if self.cancel.load(Ordering::Relaxed) || started.elapsed() > Duration::from_secs(1200)
            {
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    let _ = Command::new("taskkill.exe")
                        .args(["/PID", &child.id().to_string(), "/T", "/F"])
                        .creation_flags(0x08000000)
                        .status();
                }
                let _ = child.kill();
                let _ = child.wait();
                return Err(if self.cancel.load(Ordering::Relaxed) {
                    "下载已取消。"
                } else {
                    "加载器安装超时，请检查网络后重试。"
                }
                .into());
            }
            if let Some(status) = child.try_wait().map_err(|_| "无法读取安装器状态。")? {
                if !status.success() {
                    let tail = fs::read(&log).unwrap_or_default();
                    let text = String::from_utf8_lossy(&tail[tail.len().saturating_sub(6000)..]);
                    service.log(&text, "error");
                    return Err("加载器安装失败，详细信息请查看运行日志。".into());
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        Ok(())
    }
    fn pack_files(
        &self,
        service: &Service,
        archive: &Path,
        index: &Value,
        provider: &str,
        target: &Path,
        optional: bool,
    ) -> R<()> {
        let files = a(&index[if provider == "curseforge" {
            "_xumobResolved"
        } else {
            "files"
        }]);
        if files.len() > 10000 {
            return Err("整合包文件数量超过限制。".into());
        }
        let mut total = 0u64;
        let mut destinations = HashSet::new();
        let key = if provider == "curseforge" {
            Some(service.curseforge.vault.get()?)
        } else {
            None
        };
        let mut groups = Vec::<PackDownloadGroup>::new();
        let mut grouped = HashMap::<String, usize>::new();
        let mut total_files = 0usize;
        for f in files.iter() {
            self.check()?;
            if s(&f["env"], "client") == "unsupported"
                || (s(&f["env"], "client") == "optional" && !optional)
            {
                continue;
            }
            let path = safe_relative(s(f, "path"))?;
            let urls = a(&f["downloads"])
                .iter()
                .filter_map(Value::as_str)
                .filter(|u| net::allowed(u).is_ok())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if urls.is_empty() {
                return Err("整合包包含不受支持的文件源，请前往作者页面确认。".into());
            }
            let sha1 = s(&f["hashes"], "sha1");
            let sha512 = s(&f["hashes"], "sha512");
            let size = f["fileSize"].as_u64().unwrap_or(0);
            if size == 0 || size > 512 * 1024 * 1024 {
                return Err("整合包文件大小不受支持。".into());
            }
            total += size;
            if total > 16 * 1024 * 1024 * 1024 {
                return Err("整合包内容超过 16 GB 限制。".into());
            }
            let dest = target.join(path);
            if !destinations.insert(dest.to_string_lossy().to_ascii_lowercase()) {
                return Err("整合包包含重复的下载路径。".into());
            }
            let (algorithm, digest, hash_length) = if sha512.is_empty() {
                ("sha1", sha1, 40)
            } else {
                ("sha512", sha512, 128)
            };
            if digest.len() != hash_length || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("整合包文件缺少有效的校验信息。".into());
            }
            // Keep verified content outside the staging instance so a failed
            // installation can reuse completed files on the next attempt.
            let cached = service
                .data_dir
                .join("install-cache")
                .join("content")
                .join(algorithm)
                .join(digest.to_ascii_lowercase());
            total_files += 1;
            let group_id = format!("{algorithm}:{}", digest.to_ascii_lowercase());
            if let Some(index) = grouped.get(&group_id).copied() {
                let group = &mut groups[index];
                if group.size != size {
                    return Err("整合包中相同文件校验值对应了不同大小。".into());
                }
                for url in urls {
                    if !group.urls.contains(&url) {
                        group.urls.push(url);
                    }
                }
                group.destinations.push(dest);
            } else {
                grouped.insert(group_id, groups.len());
                groups.push(PackDownloadGroup {
                    urls,
                    sha1: sha1.to_owned(),
                    sha512: sha512.to_owned(),
                    size,
                    key: key.clone(),
                    cache: cached,
                    destinations: vec![dest],
                });
            }
        }

        // Modpacks often contain hundreds of small independent files. Fetch a
        // bounded number concurrently, while hash-addressed grouping prevents
        // duplicate files from racing on the same verified cache entry.
        let next = AtomicUsize::new(0);
        let completed = AtomicUsize::new(0);
        let downloaded_bytes = std::sync::atomic::AtomicU64::new(0);
        let failure = Mutex::new(None::<String>);
        std::thread::scope(|scope| {
            for _ in 0..8.min(groups.len()) {
                scope.spawn(|| loop {
                    if failure.lock().unwrap().is_some() {
                        break;
                    }
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(group) = groups.get(index) else {
                        break;
                    };
                    let result = (|| {
                        self.check()?;
                        let mut file_bytes = 0u64;
                        net::fetch_sources(
                            &group.urls,
                            &group.cache,
                            &group.sha1,
                            &group.sha512,
                            group.size,
                            &self.cancel,
                            group.key.as_deref(),
                            &mut |bytes| {
                                let delta = bytes.saturating_sub(file_bytes);
                                file_bytes = bytes;
                                let transferred = downloaded_bytes
                                    .fetch_add(delta, Ordering::Relaxed)
                                    + delta;
                                let done = completed.load(Ordering::Relaxed);
                                service.content_progress(json!({
                                    "filename": group.cache.file_name().unwrap_or_default().to_string_lossy(),
                                    "bytes": done,
                                    "totalBytes": total_files,
                                    "fileIndex": done,
                                    "fileCount": total_files,
                                    "downloadedBytes": transferred,
                                    "unit": "files"
                                }));
                            },
                        )
                        .map_err(|error| {
                            format!("整合包文件下载失败：{error} 已下载完成的文件会在重试时复用。")
                        })?;
                        self.check()?;
                        for dest in &group.destinations {
                            if let Some(parent) = dest.parent() {
                                fs::create_dir_all(parent)
                                    .map_err(|_| "无法创建整合包内容目录。")?;
                            }
                            fs::copy(&group.cache, dest)
                                .map_err(|_| "无法保存整合包内容文件，请检查磁盘空间。")?;
                        }
                        let done = completed.fetch_add(group.destinations.len(), Ordering::Relaxed)
                            + group.destinations.len();
                        if let Some(dest) = group.destinations.last() {
                            service.content_progress(json!({"filename":dest.file_name().unwrap_or_default().to_string_lossy(),"bytes":done,"totalBytes":total_files,"fileIndex":done,"fileCount":total_files,"downloadedBytes":downloaded_bytes.load(Ordering::Relaxed),"unit":"files"}));
                        }
                        Ok::<(), String>(())
                    })();
                    if let Err(error) = result {
                        if self.cancel.load(Ordering::Relaxed) {
                            *failure.lock().unwrap() = Some("下载已取消。".into());
                        } else {
                            *failure.lock().unwrap() = Some(error);
                        }
                        break;
                    }
                });
            }
        });
        if let Some(error) = failure.into_inner().unwrap() {
            return Err(error);
        }
        let overrides = if provider == "modrinth" {
            vec!["overrides/".to_string(), "client-overrides/".to_string()]
        } else {
            let folder = s(index, "overrides");
            safe_relative(folder)?;
            vec![format!("{}/", folder.trim_end_matches('/'))]
        };
        extract_overrides(archive, target, &overrides, &self.cancel)?;
        Ok(())
    }
}
fn encoded(v: &str) -> String {
    reqwest::Url::parse_with_params("https://localhost", &[("v", v)])
        .unwrap()
        .query()
        .unwrap()
        .trim_start_matches("v=")
        .to_string()
}
fn xml_versions(v: &str) -> Vec<String> {
    regex::Regex::new(r"<version>([^<]+)</version>")
        .unwrap()
        .captures_iter(v)
        .map(|c| c[1].to_string())
        .collect()
}
fn version_key(v: &str) -> Vec<u32> {
    v.split(['.', '-', '_'])
        .map(|s| s.parse().unwrap_or(0))
        .collect()
}
// Rust's Windows canonicalize() returns extended-length paths (\\?\...). Java 8
// turns such paths into file:// URI URLs, where the drive/UNC prefix is parsed
// as a URI authority by Forge's SimpleInstaller. Give Java conventional paths.
fn java_compatible_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{}", rest));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path.to_path_buf()
}

fn simple(v: &str) -> R<()> {
    if v.is_empty()
        || v.len() > 100
        || !v
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
        || v.contains("..")
    {
        Err("版本标识无效。".into())
    } else {
        Ok(())
    }
}
fn optifine_filename(mc: &str, kind: &str, patch: &str) -> String {
    let preview = if patch.to_ascii_lowercase().contains("pre") {
        "preview_"
    } else {
        ""
    };
    format!("{preview}OptiFine_{mc}_{kind}_{patch}.jar")
}
fn resolve_optifine_download_url(page_url: &str, filename: &str) -> R<String> {
    let page = net::browser_page(page_url, "https://optifine.net/downloads", 1024 * 1024)?;
    parse_optifine_download_url(&page, filename)
}
fn parse_optifine_download_url(page: &str, filename: &str) -> R<String> {
    let pattern = regex::Regex::new(r#"(?i)href\s*=\s*["']([^"']*downloadx\?f=[^"']+)["']"#)
        .map_err(|_| "无法解析 OptiFine 官方下载页面。")?;
    let link = pattern
        .captures(page)
        .and_then(|captures| captures.get(1))
        .ok_or("官方页面没有提供可用的安装包链接。")?
        .as_str()
        .replace("&amp;", "&");
    let url = reqwest::Url::parse("https://optifine.net/")
        .and_then(|base| base.join(&link))
        .map_err(|_| "官方页面中的安装包链接无效。")?;
    let url = net::allowed(url.as_str())?;
    if url.host_str() != Some("optifine.net") || url.path() != "/downloadx" {
        return Err("官方页面中的安装包来源无效。".into());
    }
    let requested_file = url
        .query_pairs()
        .find(|(key, _)| key == "f")
        .map(|(_, value)| value.into_owned());
    if requested_file.as_deref() != Some(filename) {
        return Err("官方页面返回的安装包与所选版本不一致。".into());
    }
    Ok(url.to_string())
}
fn download_optifine(
    urls: &[String],
    target: &Path,
    cancel: &AtomicBool,
    mut errors: Vec<String>,
) -> R<()> {
    for url in urls {
        if cancel.load(Ordering::Relaxed) {
            return Err("下载已取消。".into());
        }
        let result: R<()> = (|| {
            let mut file = File::create(target).map_err(|_| "无法保存 OptiFine。")?;
            net::stream_optifine(url, &mut file, cancel, &mut |_| {})?;
            file.sync_all().map_err(|_| "无法保存 OptiFine。")?;
            drop(file);
            validate_optifine_installer(target)?;
            Ok(())
        })();
        match result {
            Ok(()) => return Ok(()),
            Err(error) => {
                let _ = fs::remove_file(target);
                errors.push(error);
            }
        }
    }
    Err(format!("OptiFine 官方源与镜像均下载失败：{}", errors.join("；")).into())
}
fn validate_optifine_installer(path: &Path) -> R<()> {
    let file = File::open(path).map_err(|_| "无法读取 OptiFine 文件。")?;
    if file
        .metadata()
        .map_err(|_| "无法检查 OptiFine 文件。")?
        .len()
        < 64 * 1024
    {
        return Err("OptiFine 下载文件过小，可能是错误页面。".into());
    }
    let mut archive =
        zip::ZipArchive::new(file).map_err(|_| "OptiFine 下载内容不是有效安装文件。")?;
    let mut installer = archive
        .by_name("optifine/Installer.class")
        .map_err(|_| "OptiFine 文件缺少安装器组件，下载内容可能不完整。")?;
    let mut header = [0; 8];
    installer
        .read_exact(&mut header)
        .map_err(|_| "OptiFine 安装器组件不完整。")?;
    if header[..4] != [0xCA, 0xFE, 0xBA, 0xBE] {
        return Err("OptiFine 安装器组件格式无效。".into());
    }
    Ok(())
}
#[cfg(test)]
mod optifine_tests {
    use super::parse_optifine_download_url;

    #[test]
    fn official_page_link_is_resolved_and_bound_to_selected_file() {
        let html = r#"<a href="downloadx?f=OptiFine_1.16.5_HD_U_G8.jar&amp;x=abc">Download</a>"#;
        assert_eq!(
            parse_optifine_download_url(html, "OptiFine_1.16.5_HD_U_G8.jar").unwrap(),
            "https://optifine.net/downloadx?f=OptiFine_1.16.5_HD_U_G8.jar&x=abc"
        );
        assert!(parse_optifine_download_url(html, "OptiFine_1.20.1_HD_U_I6.jar").is_err());
    }

    #[test]
    fn official_page_cannot_select_another_download_host() {
        let html = r#"<a href="https://example.invalid/downloadx?f=OptiFine_1.16.5_HD_U_G8.jar">Download</a>"#;
        assert!(parse_optifine_download_url(html, "OptiFine_1.16.5_HD_U_G8.jar").is_err());
    }
}
fn validate_target(v: &Value) -> R<()> {
    let name = s(v, "name");
    let p = safe_relative(name)?;
    if p.components().count() != 1 || name.encode_utf16().count() > 100 {
        return Err("实例名称过长或包含路径字符。".into());
    }
    let root = Path::new(s(v, "gameRoot"));
    if !root.is_absolute() {
        return Err("请选择游戏安装目录。".into());
    }
    if root.join("versions").join(name).exists() {
        return Err("同名游戏版本已存在，请换一个实例名称。".into());
    }
    Ok(())
}
fn validate_existing_target(v: &Value) -> R<()> {
    let name = s(v, "name");
    let p = safe_relative(name)?;
    if p.components().count() != 1 || name.encode_utf16().count() > 100 {
        return Err("游戏版本名称无效。".into());
    }
    let root = Path::new(s(v, "gameRoot"));
    if !root.is_absolute() {
        return Err("请选择游戏安装目录。".into());
    }
    let root = fs::canonicalize(root).map_err(|_| "无法读取游戏安装目录。")?;
    let versions = fs::canonicalize(root.join("versions")).map_err(|_| "无法读取版本目录。")?;
    if !versions.starts_with(&root) {
        return Err("版本目录指向游戏目录之外，未进行修改。".into());
    }
    let version_dir =
        fs::canonicalize(versions.join(name)).map_err(|_| "找不到这个已安装的游戏版本。")?;
    if !version_dir.starts_with(&versions) {
        return Err("游戏版本目录指向外部位置，未进行修改。".into());
    }
    let manifest = version_dir.join(format!("{name}.json"));
    if !version_dir.is_dir() || !manifest.is_file() {
        return Err("找不到这个已安装的游戏版本。".into());
    }
    if !fs::canonicalize(&manifest)
        .map_err(|_| "无法读取版本配置。")?
        .starts_with(&version_dir)
    {
        return Err("版本配置指向外部位置，未进行修改。".into());
    }
    let current = core::resolve_version(&root, name)?;
    if s(&current, "mainClass").is_empty() {
        return Err("此版本缺少启动主类，无法安全修改加载器。".into());
    }
    if s(v, "minecraft")
        != core::describe_version(&current)["minecraft"]
            .as_str()
            .unwrap_or("")
    {
        return Err("加载器的 Minecraft 版本与所选客户端不一致。".into());
    }
    let game_dir = s(v, "instanceGameDir");
    if !game_dir.is_empty() {
        let game_dir = Path::new(game_dir);
        if !game_dir.is_absolute() || !game_dir.is_dir() {
            return Err("此版本的游戏数据目录无效，未进行修改。".into());
        }
        let canonical_game_dir =
            fs::canonicalize(game_dir).map_err(|_| "无法读取游戏数据目录。")?;
        if canonical_game_dir != root && canonical_game_dir != version_dir {
            return Err("此版本的数据目录不在受支持的位置，未进行修改。".into());
        }
    }
    Ok(())
}

fn replace_instance_launch_files(source: &Path, target: &Path, name: &str) -> R<()> {
    let suffix = SERIAL.fetch_add(1, Ordering::Relaxed);
    let mut staged = Vec::new();
    for filename in [format!("{name}.json"), format!("{name}.jar")] {
        let from = source.join(&filename);
        let to = target.join(&filename);
        let existed = to.is_file();
        if !from.is_file() || (filename.ends_with(".json") && !existed) {
            return Err("目标客户端缺少启动配置或新游戏文件，未修改实例。".into());
        }
        let temporary = target.join(format!(".{filename}.xumob-{suffix}.tmp"));
        let backup = target.join(format!(".{filename}.xumob-{suffix}.bak"));
        fs::copy(&from, &temporary).map_err(|_| "无法暂存新的客户端配置。")?;
        staged.push((to, temporary, backup, existed));
    }
    for (target, _, backup, existed) in &staged {
        if *existed {
            if let Err(error) = fs::rename(target, backup) {
                for (old, _, saved, had_old) in &staged {
                    if *had_old && saved.exists() {
                        let _ = fs::rename(saved, old);
                    }
                }
                for (_, temporary, _, _) in &staged {
                    let _ = fs::remove_file(temporary);
                }
                return Err(format!("无法备份原客户端文件，修改已取消：{error}"));
            }
        }
    }
    for (target, temporary, _, _) in &staged {
        if let Err(error) = fs::rename(temporary, target) {
            for (old, _, backup, had_old) in &staged {
                let _ = fs::remove_file(old);
                if *had_old {
                    let _ = fs::rename(backup, old);
                }
            }
            for (_, temporary, _, _) in &staged {
                let _ = fs::remove_file(temporary);
            }
            return Err(format!("无法写入新的客户端配置，已恢复原版本：{error}"));
        }
    }
    for (_, _, backup, existed) in staged {
        if existed {
            let _ = fs::remove_file(backup);
        }
    }
    Ok(())
}

fn remove_optifine_sidecar(mods: &Path, keep: &Path) {
    let Ok(entries) = fs::read_dir(mods) else {
        return;
    };
    let keep = fs::read_dir(keep)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase())
        .collect::<HashSet<_>>();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.starts_with("optifine_") && name.ends_with(".jar") && !keep.contains(&name) {
            let _ = fs::remove_file(entry.path());
        }
    }
}
fn safe_relative(value: &str) -> R<PathBuf> {
    if value.is_empty() || value.starts_with(['/', '\\']) || value.contains('\\') {
        return Err("整合包路径无效。".into());
    }
    for part in value.trim_end_matches('/').split('/') {
        let base = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|c| c.is_control() || "<>:\"|?*".contains(c))
            || ["CON", "PRN", "AUX", "NUL"].contains(&base.as_str())
            || (base.len() == 4
                && (base.starts_with("COM") || base.starts_with("LPT"))
                && base.as_bytes()[3].is_ascii_digit())
        {
            return Err("文件路径包含不安全的字符。".into());
        }
    }
    Ok(PathBuf::from(value))
}
fn write_json(path: &Path, v: &Value) -> R<()> {
    fs::write(
        path,
        serde_json::to_vec_pretty(v).map_err(|_| "无法编码版本配置。")?,
    )
    .map_err(|_| "无法保存版本配置。".into())
}
fn sidecar(url: &str) -> R<String> {
    let data = net::bytes(&format!("{url}.sha1"), 4096)?;
    let value = String::from_utf8_lossy(&data)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if value.len() != 40 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("组件没有有效的 SHA-1 校验文件。".into());
    }
    Ok(value)
}
fn zip_json(path: &Path, name: &str) -> R<Value> {
    let mut zip = zip::ZipArchive::new(File::open(path).map_err(|_| "无法打开安装包。")?)
        .map_err(|_| "安装包不是有效 ZIP 文件。")?;
    let f = zip
        .by_name(name)
        .map_err(|_| "安装包缺少必要的配置文件。")?;
    if f.size() > 16 * 1024 * 1024 {
        return Err("安装包配置文件过大。".into());
    }
    let mut bytes = Vec::new();
    f.take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "无法读取安装包配置。")?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("安装包配置文件过大。".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "安装包配置无效。".into())
}
fn install_legacy_forge(installer: &Path, profile: &Value, root: &Path, mc: &str) -> R<String> {
    if profile["install"]["transform"].is_array() && !a(&profile["install"]["transform"]).is_empty()
    {
        return Err("此旧版 Forge 需要额外转换，暂不支持自动安装。".into());
    }
    if s(&profile["install"], "minecraft") != mc {
        return Err("旧版 Forge 安装包与游戏版本不一致。".into());
    }
    let mut version = profile["versionInfo"].clone();
    let id = s(&version, "id").to_owned();
    simple(&id)?;
    if s(&profile["install"], "target") != id {
        return Err("旧版 Forge 安装目标不一致。".into());
    }
    let coordinate = s(&profile["install"], "path");
    let relative = core::maven_path(coordinate)?;
    let target = core::inside(root, &format!("libraries/{relative}"))?;
    let member = s(&profile["install"], "filePath");
    safe_relative(member)?;
    let mut zip =
        zip::ZipArchive::new(File::open(installer).map_err(|_| "无法读取旧版 Forge 安装包。")?)
            .map_err(|_| "Forge 安装包损坏。")?;
    let mut payload = zip
        .by_name(member)
        .map_err(|_| "旧版安装包缺少 Forge 主文件。")?;
    if payload.size() > 256 * 1024 * 1024 {
        return Err("Forge 主文件过大。".into());
    }
    fs::create_dir_all(target.parent().unwrap()).map_err(|_| "无法创建 Forge 库目录。")?;
    let mut out = File::create(&target).map_err(|_| "无法创建 Forge 库文件。")?;
    let expected = payload.size();
    let count = std::io::copy(&mut payload.by_ref().take(expected + 1), &mut out)
        .map_err(|_| "无法提取 Forge 主文件。")?;
    if count != expected {
        return Err("Forge 主文件大小不一致。".into());
    }
    drop(out);
    let hash = net::hash(&target, false)?;
    let size = fs::metadata(&target)
        .map_err(|_| "无法读取 Forge 主文件。")?
        .len();
    let libraries = version["libraries"]
        .as_array_mut()
        .ok_or("旧版 Forge 库配置无效。")?;
    let mut found = false;
    for lib in libraries {
        let url = s(lib, "url");
        if url.starts_with("http://files.minecraftforge.net/maven/")
            || url == "http://maven.minecraftforge.net/"
        {
            lib["url"] = json!("https://maven.minecraftforge.net/");
        } else if url == "http://libraries.minecraft.net/" {
            lib["url"] = json!("https://libraries.minecraft.net/");
        }
        if s(lib, "name") == coordinate {
            found = true;
            lib["downloads"] = json!({"artifact":{"path":relative,"sha1":hash,"size":size,"url":format!("https://maven.minecraftforge.net/{}",core::maven_path(&format!("{coordinate}:universal"))?)}})
        }
    }
    if !found {
        return Err("Forge 主文件没有对应的运行依赖。".into());
    }
    version["inheritsFrom"] = json!(mc);
    let dir = root.join("versions").join(&id);
    fs::create_dir_all(&dir).map_err(|_| "无法创建旧版 Forge 版本目录。")?;
    write_json(&dir.join(format!("{id}.json")), &version)?;
    Ok(id)
}
fn read_pack(path: &Path, provider: &str) -> R<Value> {
    let v = zip_json(
        path,
        if provider == "modrinth" {
            "modrinth.index.json"
        } else {
            "manifest.json"
        },
    )?;
    if provider == "modrinth" && (v["formatVersion"] != 1 || v["game"] != "minecraft") {
        return Err("不支持这个 Modrinth 整合包格式。".into());
    }
    if provider == "curseforge"
        && (v["manifestType"] != "minecraftModpack" || v["manifestVersion"] != 1)
    {
        return Err("不支持这个 CurseForge 整合包格式。".into());
    }
    Ok(v)
}
fn archive_pack_icon(path: &Path, index: &Value) -> String {
    use base64::Engine as _;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return String::new(),
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(value) => value,
        Err(_) => return String::new(),
    };
    let mut candidates = Vec::new();
    if !s(index, "icon").is_empty() {
        candidates.push(s(index, "icon").to_string());
    }
    candidates.extend(
        [
            "icon.png",
            "icon.jpg",
            "icon.jpeg",
            "icon.webp",
            "overrides/icon.png",
        ]
        .into_iter()
        .map(str::to_string),
    );
    for name in candidates {
        if safe_relative(&name).is_err() {
            continue;
        }
        let Ok(entry) = archive.by_name(&name) else {
            continue;
        };
        if entry.size() == 0 || entry.size() > 2 * 1024 * 1024 {
            continue;
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        if entry
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .is_err()
        {
            continue;
        }
        let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else if bytes.starts_with(b"\xff\xd8\xff") {
            "image/jpeg"
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            "image/webp"
        } else {
            continue;
        };
        return format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
    }
    String::new()
}
fn pack_game(v: &Value, provider: &str) -> R<(String, String, String)> {
    if provider == "modrinth" {
        let deps = v["dependencies"]
            .as_object()
            .ok_or("整合包缺少游戏依赖。")?;
        let mut loader = "vanilla";
        let mut version = "";
        for (k, val) in deps {
            match k.as_str() {
                "minecraft" => {}
                "fabric-loader" | "quilt-loader" | "forge" | "neoforge" => {
                    if loader != "vanilla" {
                        return Err("整合包含有多个互斥加载器。".into());
                    }
                    loader = match k.as_str() {
                        "fabric-loader" => "fabric",
                        "quilt-loader" => "quilt",
                        v => v,
                    };
                    version = val.as_str().ok_or("加载器版本无效。")?
                }
                _ => return Err(format!("整合包含有尚不支持的依赖：{k}")),
            }
        }
        return Ok((
            s(&v["dependencies"], "minecraft").into(),
            loader.into(),
            version.into(),
        ));
    }
    let loaders = a(&v["minecraft"]["modLoaders"]);
    let primary = loaders
        .iter()
        .find(|l| l["primary"] == true)
        .or_else(|| loaders.first());
    if let Some(loader) = primary {
        let id = s(loader, "id");
        for prefix in ["neoforge", "forge", "fabric", "quilt"] {
            if let Some(version) = id.strip_prefix(&format!("{prefix}-")) {
                return Ok((
                    s(&v["minecraft"], "version").into(),
                    prefix.into(),
                    version.into(),
                ));
            }
        }
        return Err("整合包使用了不支持的加载器。".into());
    }
    Ok((
        s(&v["minecraft"], "version").into(),
        "vanilla".into(),
        "".into(),
    ))
}
fn extract_overrides(
    archive: &Path,
    target: &Path,
    layers: &[String],
    cancel: &AtomicBool,
) -> R<()> {
    let mut zip = zip::ZipArchive::new(File::open(archive).map_err(|_| "无法打开整合包。")?)
        .map_err(|_| "整合包损坏。")?;
    if zip.len() > 10000 {
        return Err("整合包文件数量超过限制。".into());
    }
    let mut size = 0u64;
    for prefix in layers {
        let mut names = HashSet::new();
        for i in 0..zip.len() {
            if cancel.load(Ordering::Relaxed) {
                return Err("下载已取消。".into());
            }
            let mut f = zip.by_index(i).map_err(|_| "整合包文件损坏。")?;
            let Some(name) = f.name().strip_prefix(prefix) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let rel = safe_relative(name)?;
            if f.is_dir() {
                continue;
            }
            if !names.insert(name.to_ascii_lowercase())
                || f.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
            {
                return Err("整合包包含重复路径或符号链接。".into());
            }
            size += f.size();
            if f.size() > 512 * 1024 * 1024 || size > 16 * 1024 * 1024 * 1024 {
                return Err("整合包解压内容超过大小限制。".into());
            }
            let dest = target.join(rel);
            fs::create_dir_all(dest.parent().unwrap()).map_err(|_| "无法创建整合包目录。")?;
            let mut out = File::create(dest).map_err(|_| "无法写入整合包文件。")?;
            let expected = f.size();
            let count = std::io::copy(&mut f.by_ref().take(expected + 1), &mut out)
                .map_err(|_| "整合包解压失败。")?;
            if count != expected {
                return Err("整合包文件大小与清单不一致。".into());
            }
        }
    }
    Ok(())
}
fn copy_tree(source: &Path, target: &Path, cache: bool) -> R<()> {
    fs::create_dir_all(target).map_err(|_| "无法创建游戏文件夹。")?;
    if fs::symlink_metadata(target)
        .map_err(|_| "无法检查游戏文件夹。")?
        .file_type()
        .is_symlink()
    {
        return Err("游戏文件夹不能是符号链接。".into());
    }
    for e in fs::read_dir(source).map_err(|_| "无法读取安装文件。")? {
        let e = e.map_err(|_| "无法读取安装文件。")?;
        let p = e.path();
        if e.file_type()
            .map_err(|_| "无法读取文件类型。")?
            .is_symlink()
        {
            return Err("安装文件包含符号链接。".into());
        }
        let dest = target.join(e.file_name());
        if p.is_dir() {
            copy_tree(&p, &dest, cache)?
        } else if dest.exists() {
            if fs::symlink_metadata(&dest)
                .map_err(|_| "无法检查游戏缓存。")?
                .file_type()
                .is_symlink()
                || !dest.is_file()
            {
                return Err("缓存目标不是普通文件。".into());
            }
            if net::hash(&dest, false)? != net::hash(&p, false)? {
                if !cache {
                    return Err("实例文件已存在，未覆盖它。".into());
                }
                let temp = dest.with_extension(format!(
                    "xumob-{}-tmp",
                    SERIAL.fetch_add(1, Ordering::Relaxed)
                ));
                let result = (|| {
                    let mut input = File::open(&p).map_err(|_| "无法读取已校验缓存。")?;
                    let mut out = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&temp)
                        .map_err(|_| "无法更新游戏缓存。")?;
                    std::io::copy(&mut input, &mut out).map_err(|_| "游戏缓存写入失败。")?;
                    out.sync_all().map_err(|_| "无法保存游戏缓存。")?;
                    drop(out);
                    fs::rename(&temp, &dest).map_err(|_| "无法替换已有游戏缓存。")?;
                    Ok::<(), String>(())
                })();
                if result.is_err() {
                    let _ = fs::remove_file(&temp);
                }
                result?;
            }
        } else {
            let mut input = File::open(&p).map_err(|_| "无法读取已安装文件。")?;
            let mut out = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&dest)
                .map_err(|_| "无法保存游戏缓存。")?;
            if std::io::copy(&mut input, &mut out).is_err() {
                drop(out);
                let _ = fs::remove_file(dest);
                return Err("游戏文件保存失败，请检查磁盘空间。".into());
            }
        }
    }
    Ok(())
}
fn verify_processor_outputs(profile: &Value, root: &Path) -> R<()> {
    fn expand(value: &str, profile: &Value, root: &Path) -> R<String> {
        let mut value = value.replace("{ROOT}", &root.to_string_lossy());
        for _ in 0..8 {
            if value.starts_with('{') && value.ends_with('}') {
                let name = &value[1..value.len() - 1];
                let next = s(&profile["data"][name], "client");
                if next.is_empty() {
                    return Err("安装器输出引用无效。".into());
                }
                value = next.to_owned();
            } else {
                break;
            }
        }
        if value.starts_with('[') && value.ends_with(']') {
            value = root
                .join("libraries")
                .join(core::maven_path(&value[1..value.len() - 1])?)
                .to_string_lossy()
                .into_owned();
        }
        Ok(value)
    }
    for processor in a(&profile["processors"]) {
        if processor["sides"].is_array() && !a(&processor["sides"]).iter().any(|v| v == "client") {
            continue;
        }
        if let Some(outputs) = processor["outputs"].as_object() {
            for (path, hash) in outputs {
                let path = expand(path, profile, root)?;
                let hash = expand(hash.as_str().unwrap_or(""), profile, root)?;
                let p = if Path::new(&path).is_absolute() {
                    PathBuf::from(path)
                } else {
                    root.join(safe_relative(&path)?)
                };
                if !p.starts_with(root) {
                    return Err("安装器输出路径超出暂存目录。".into());
                }
                if !p.is_file() {
                    return Err("加载器处理器未生成必要文件。".into());
                }
                if hash.len() == 40
                    && hash.bytes().all(|b| b.is_ascii_hexdigit())
                    && net::hash(&p, false)? != hash.to_ascii_lowercase()
                {
                    return Err("加载器处理产物校验失败。".into());
                }
            }
        }
    }
    let patched = s(&profile["data"]["PATCHED"], "client");
    if !patched.is_empty() {
        let path = expand(patched, profile, root)?;
        if !Path::new(&path).is_file() {
            return Err("加载器没有生成最终客户端文件。".into());
        }
    }
    Ok(())
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl Service {
    pub fn download_request(
        self: &Arc<Self>,
        action: &str,
        provider: &str,
        args: &Value,
    ) -> R<Value> {
        let manager = match provider {
            "curseforge" => &self.curseforge.content,
            "modrinth" => &self.modrinth,
            _ => &self.modrinth,
        };
        let context = Context::from_settings(&self.settings()).ok();
        match action {
            "mods_check_updates" => self.check_mod_updates(),
            "mod_update" => self.update_mod(provider, s(args, "updateId")),
            "mods_metadata" => self.mod_metadata(),
            "key_status" => Ok(self.curseforge.vault.status()),
            "key_configure" => {
                self.ensure_account_editable()?;
                self.curseforge.vault.configure(s(args, "key"))
            }
            "catalog" => manager.catalog(),
            "search" => manager.search_filtered(
                s(args, "query"),
                s(args, "kind"),
                args["offset"].as_u64().unwrap_or(0) as u32,
                s(args, "sort"),
                &args["filters"],
                context.as_ref(),
            ),
            "versions" => manager.versions_filtered(
                s(args, "projectId"),
                s(args, "kind"),
                &args["filters"],
                context.as_ref(),
            ),
            "prepare" => {
                self.ensure_account_editable()?;
                manager.prepare(
                    s(args, "projectId"),
                    s(args, "versionId"),
                    s(args, "kind"),
                    context.ok_or("请先选择一个已安装的游戏实例。")?,
                )
            }
            "install" => {
                let context = Context::from_settings(&self.settings())?;
                manager.reset_cancellation();
                self.begin("downloading", "正在下载内容…")?;
                let result = manager.install(s(args, "planId"), context, &mut |p| {
                    self.content_progress(p)
                });
                self.end();
                self.state_to(
                    if result.is_ok() { "idle" } else { "error" },
                    result
                        .as_ref()
                        .map(|_| "内容已安装完成")
                        .unwrap_or_else(|e| e),
                    None,
                );
                result
            }
            "cancel" => {
                manager.cancel();
                Ok(Value::Null)
            }
            "game_catalog" => self.installer.catalog(),
            "game_inspect_pack" => self.installer.inspect_pack(s(args, "archivePath")),
            "game_components" => self.installer.components(s(args, "minecraft")),
            "game_version_info" => self.installer.version_info(self, s(args, "versionId")),
            "game_prepare" => self.installer.prepare(self, args, provider),
            "game_install" => self.installer.install(self, s(args, "planId")),
            "game_cancel" => {
                self.installer.cancel();
                Ok(Value::Null)
            }
            _ => Err("下载操作无效。".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifies_april_fools_versions_without_relabeling_regular_snapshots() {
        for id in ["26w14a", "20w14∞", "3D Shareware v1.34", "2.0.3"] {
            assert!(is_april_fools_version(id), "{id}");
        }
        for id in ["26w13a", "1.20.1", "1.RV-Pre1x"] {
            assert!(!is_april_fools_version(id), "{id}");
        }
    }
    #[test]
    fn java_paths_strip_windows_verbatim_prefixes() {
        assert_eq!(
            java_compatible_path(Path::new(r"\\?\D:\Games\instance")),
            PathBuf::from(r"D:\Games\instance")
        );
        assert_eq!(
            java_compatible_path(Path::new(r"\\?\UNC\server\share\instance")),
            PathBuf::from(r"\\server\share\instance")
        );
        assert_eq!(
            java_compatible_path(Path::new(r"D:\Games\instance")),
            PathBuf::from(r"D:\Games\instance")
        );
    }
    #[test]
    fn legacy_forge_embedded_universal_uses_the_runtime_coordinate() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!(
            "xumob-forge-test-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let _cleanup = Cleanup(root.clone());
        let installer = root.join("installer.jar");
        let mut zip = zip::ZipWriter::new(File::create(&installer).unwrap());
        zip.start_file(
            "forge-universal.jar",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"embedded-forge-library").unwrap();
        zip.finish().unwrap();
        let coordinate = "net.minecraftforge:forge:1.8.9-11.15.1.2318-1.8.9";
        let profile = json!({"install":{"minecraft":"1.8.9","target":"legacy-forge","path":coordinate,"filePath":"forge-universal.jar"},"versionInfo":{"id":"legacy-forge","libraries":[{"name":coordinate},{"name":"net.minecraft:launchwrapper:1.12"}]}});
        assert_eq!(
            install_legacy_forge(&installer, &profile, &root, "1.8.9").unwrap(),
            "legacy-forge"
        );
        let target = root
            .join("libraries")
            .join(core::maven_path(coordinate).unwrap());
        assert_eq!(fs::read(&target).unwrap(), b"embedded-forge-library");
        assert!(!target
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("universal"));
        let meta: Value = serde_json::from_slice(
            &fs::read(root.join("versions/legacy-forge/legacy-forge.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(meta["libraries"].as_array().unwrap().len(), 2);
        assert_eq!(
            meta["libraries"][0]["downloads"]["artifact"]["sha1"],
            net::hash(&target, false).unwrap()
        );
        assert_eq!(meta["inheritsFrom"], "1.8.9");
    }
    #[test]
    fn paths_reject_windows_escape() {
        for v in [
            "../mods/a.jar",
            "C:/Windows/a",
            "/mods/a",
            "mods\\a.jar",
            "mods/CON.jar",
            "mods/a:evil",
            "mods/a. ",
        ] {
            assert!(safe_relative(v).is_err(), "{v}")
        }
        assert!(safe_relative("mods/a-b_1.2.jar").is_ok())
    }
    #[test]
    fn packs_pin_their_loader() {
        assert_eq!(
            pack_game(
                &json!({"dependencies":{"minecraft":"1.20.1","fabric-loader":"0.16.14"}}),
                "modrinth"
            )
            .unwrap(),
            ("1.20.1".into(), "fabric".into(), "0.16.14".into())
        );
        assert!(pack_game(
            &json!({"dependencies":{"minecraft":"1.20.1","mystery":"1"}}),
            "modrinth"
        )
        .is_err());
        assert!(pack_game(
            &json!({"dependencies":{"minecraft":"1.20.1","forge":"1","fabric-loader":"2"}}),
            "modrinth"
        )
        .is_err());
    }
    #[test]
    fn cf_primary_loader() {
        assert_eq!(pack_game(&json!({"minecraft":{"version":"1.21.1","modLoaders":[{"id":"neoforge-21.1.172","primary":true}]}}),"curseforge").unwrap(),("1.21.1".into(),"neoforge".into(),"21.1.172".into()));
    }
    #[test]
    fn archive_traversal_is_rejected_before_extracting() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!(
            "xumob-zip-test-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let _cleanup = Cleanup(root.clone());
        let archive = root.join("pack.zip");
        let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
        zip.start_file(
            "overrides/../../escape.txt",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"escape").unwrap();
        zip.finish().unwrap();
        let target = root.join("instance");
        fs::create_dir(&target).unwrap();
        assert!(extract_overrides(
            &archive,
            &target,
            &["overrides/".into()],
            &AtomicBool::new(false)
        )
        .is_err());
        assert!(!root.join("escape.txt").exists());
    }
    #[test]
    fn client_overrides_win_and_server_overrides_are_skipped() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!(
            "xumob-layer-test-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let _cleanup = Cleanup(root.clone());
        let archive = root.join("pack.zip");
        let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
        for (name, content) in [
            ("overrides/options.txt", "shared"),
            ("client-overrides/options.txt", "client"),
            ("server-overrides/server.properties", "server"),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let target = root.join("instance");
        fs::create_dir(&target).unwrap();
        extract_overrides(
            &archive,
            &target,
            &["overrides/".into(), "client-overrides/".into()],
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(target.join("options.txt")).unwrap(),
            "client"
        );
        assert!(!target.join("server.properties").exists());
    }
    #[test]
    fn only_shared_cache_files_may_be_replaced() {
        let root = std::env::temp_dir().join(format!(
            "xumob-cache-test-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let _cleanup = Cleanup(root.clone());
        let (source, target) = (root.join("source"), root.join("target"));
        fs::create_dir(&source).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(source.join("index.json"), b"verified").unwrap();
        fs::write(target.join("index.json"), b"old").unwrap();
        assert!(copy_tree(&source, &target, false).is_err());
        assert_eq!(fs::read(target.join("index.json")).unwrap(), b"old");
        copy_tree(&source, &target, true).unwrap();
        assert_eq!(fs::read(target.join("index.json")).unwrap(), b"verified");
    }
}
