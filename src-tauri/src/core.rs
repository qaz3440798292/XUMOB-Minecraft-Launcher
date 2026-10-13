use chrono::{SecondsFormat, Utc};
use md5::Md5;
use regex::Regex;
use serde_json::{json, Map, Value};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::{
    collections::{HashSet, VecDeque},
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use sysinfo::System;

pub type R<T> = Result<T, String>;
pub type Sink = Arc<dyn Fn(&str, Value) + Send + Sync>;
fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
fn text(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        s.into()
    } else {
        v.to_string()
    }
}
fn list(v: &Value) -> Vec<Value> {
    v.as_array().cloned().unwrap_or_default()
}
fn merge(a: &Value, b: &Value) -> Value {
    let mut result = a.as_object().cloned().unwrap_or_default();
    if let Some(map) = b.as_object() {
        result.extend(map.clone());
    }
    Value::Object(result)
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn path_text(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}
fn abs(p: &str) -> PathBuf {
    let input = PathBuf::from(p);
    let input = if input.is_absolute() {
        input
    } else {
        std::env::current_dir().unwrap_or_default().join(input)
    };
    let mut normalized = PathBuf::new();
    for part in input.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(part.as_os_str()),
        }
    }
    normalized
}
pub fn inside(root: &Path, relative: &str) -> R<PathBuf> {
    if Path::new(relative).is_absolute()
        || relative.contains('\0')
        || relative
            .split(['/', '\\'])
            .any(|part| part == ".." || part.contains(':'))
    {
        return Err("配置中的文件路径超出游戏目录。".into());
    }
    Ok(relative
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .fold(root.to_path_buf(), |path, part| path.join(part)))
}
fn valid_id(id: &str) -> R<()> {
    if id.is_empty()
        || id.encode_utf16().count() > 200
        || id == "."
        || id == ".."
        || id.contains(['/', '\\', '\0', ':'])
    {
        Err("版本名称无效。".into())
    } else {
        Ok(())
    }
}
fn read_json(p: &Path) -> R<Value> {
    if fs::metadata(p).map_err(err)?.len() > 8 * 1024 * 1024 {
        return Err("版本配置文件过大。".into());
    }
    let data = fs::read_to_string(p).map_err(err)?;
    serde_json::from_str(data.trim_start_matches('\u{feff}')).map_err(err)
}
fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
fn system() -> Value {
    let mut sys = System::new();
    sys.refresh_memory();
    let total = sys.total_memory() / 1024 / 1024;
    json!({"totalMB":total,"freeMB":sys.available_memory()/1024/1024,"maxMB":65536u64.min((total as f64 * 0.85 / 256.).floor() as u64 * 256),"platform":"win32","arch":"x64"})
}
fn default_settings() -> Value {
    let mut v: Value = serde_json::from_str(include_str!("../defaults.json")).unwrap();
    let total = system()["totalMB"].as_u64().unwrap_or(8192);
    v["memoryMB"] =
        json!(4096u64.min(1024u64.max((total as f64 * 0.5 / 256.).floor() as u64 * 256)));
    v["extraJvmArgs"] = json!("");
    v["extraGameArgs"] = json!("");
    v
}
fn profile(v: &Value) -> Value {
    let mut result = Map::new();
    for key in [
        "javaPath",
        "javaMode",
        "memoryMB",
        "isolated",
        "width",
        "height",
        "extraJvmArgs",
        "extraGameArgs",
    ] {
        result.insert(key.into(), v[key].clone());
    }
    Value::Object(result)
}
pub fn profile_key(root: &str, id: &str) -> String {
    hex::encode(Sha256::digest(
        format!("{}\0{}", path_text(&abs(root)).to_lowercase(), id).as_bytes(),
    ))
}
pub fn validate(input: &Value) -> R<Value> {
    let max = system()["maxMB"].as_u64().unwrap();
    let memory = input["memoryMB"].as_u64().unwrap_or(0);
    if memory < 1024 || memory > max {
        return Err(format!("内存应在 1024–{max} MB 之间。"));
    }
    if !Regex::new(r"^[A-Za-z0-9_]{3,16}$")
        .unwrap()
        .is_match(s(input, "username"))
    {
        return Err("离线用户名请使用 3–16 位字母、数字或下划线。".into());
    }
    let root = s(input, "gameRoot");
    if !root.is_empty() && !Path::new(root).is_absolute() {
        return Err("请选择完整的游戏目录。".into());
    }
    let id = s(input, "versionId");
    if !id.is_empty() {
        valid_id(id)?;
    }
    let width = input["width"].as_u64().unwrap_or(0);
    let height = input["height"].as_u64().unwrap_or(0);
    if !(640..=7680).contains(&width) || !(360..=4320).contains(&height) {
        return Err("窗口尺寸无效。".into());
    }
    let mut directories = Vec::new();
    let mut seen = HashSet::new();
    for item in list(&input["directories"]).into_iter().take(100) {
        let p = s(&item, "path");
        if p.is_empty() || !Path::new(p).is_absolute() || !seen.insert(p.to_lowercase()) {
            continue;
        }
        let last = s(&item, "lastVersionId");
        if !last.is_empty() {
            valid_id(last)?;
        }
        directories
            .push(json!({"path":path_text(&abs(p)),"name":s(&item,"name"),"lastVersionId":last}));
    }
    let paths = list(&input["knownJavaPaths"])
        .into_iter()
        .filter(|item| {
            item.as_str()
                .is_some_and(|file| Path::new(file).is_absolute())
        })
        .take(30)
        .collect::<Vec<_>>();
    Ok(
        json!({"gameRoot":if root.is_empty(){String::new()}else{path_text(&abs(root))},"versionId":id,"javaPath":s(input,"javaPath"),"javaMode":if s(input,"javaMode")=="manual"{"manual"}else{"auto"},"username":s(input,"username"),"memoryMB":memory,"isolated":if input["isolated"].is_boolean(){input["isolated"].clone()}else{json!("auto")},"width":width,"height":height,"directories":directories,"profiles":if input["profiles"].is_object(){input["profiles"].clone()}else{json!({})},"knownJavaPaths":paths,"extraJvmArgs":s(input,"extraJvmArgs").chars().take(16000).collect::<String>(),"extraGameArgs":s(input,"extraGameArgs").chars().take(16000).collect::<String>()}),
    )
}

pub fn resolve_version(root: &Path, id: &str) -> R<Value> {
    fn visit(root: &Path, id: &str, seen: &mut HashSet<String>) -> R<Value> {
        valid_id(id)?;
        if seen.len() > 12 || !seen.insert(id.into()) {
            return Err("版本配置存在循环继承。".into());
        }
        let mut child = read_json(&inside(root, &format!("versions/{id}/{id}.json"))?)?;
        let parent_id = s(&child, "inheritsFrom").to_string();
        if parent_id.is_empty() {
            let jar = if s(&child, "jar").is_empty() {
                id
            } else {
                s(&child, "jar")
            }
            .to_string();
            let mc = [
                s(&child, "clientVersion"),
                s(&child, "minecraftVersion"),
                id,
            ]
            .into_iter()
            .find(|x| !x.is_empty())
            .unwrap()
            .to_string();
            child["id"] = json!(id);
            child["jarId"] = json!(jar);
            child["minecraftId"] = json!(mc);
            return Ok(child);
        }
        let parent = visit(root, &parent_id, seen)?;
        let mut result = merge(&parent, &child);
        result["id"] = json!(id);
        result["jarId"] = if s(&child, "jar").is_empty() {
            parent["jarId"].clone()
        } else {
            child["jar"].clone()
        };
        result["minecraftId"] = [
            s(&child, "clientVersion"),
            s(&child, "minecraftVersion"),
            s(&parent, "minecraftId"),
        ]
        .into_iter()
        .find(|x| !x.is_empty())
        .map(|x| json!(x))
        .unwrap_or(json!(id));
        let mut libs = list(&parent["libraries"]);
        for lib in list(&child["libraries"]) {
            let key = library_key(s(&lib, "name"));
            if let Some(i) = libs.iter().position(|v| library_key(s(v, "name")) == key) {
                libs[i] = lib;
            } else {
                libs.push(lib);
            }
        }
        result["libraries"] = json!(libs);
        for kind in ["jvm", "game"] {
            let mut values = list(&parent["arguments"][kind]);
            values.extend(list(&child["arguments"][kind]));
            if !result["arguments"].is_object() {
                result["arguments"] = json!({});
            }
            result["arguments"][kind] = json!(values);
        }
        Ok(result)
    }
    visit(root, id, &mut HashSet::new())
}
fn library_key(name: &str) -> String {
    let (coord, ext) = name.split_once('@').unwrap_or((name, "jar"));
    let p = coord.split(':').collect::<Vec<_>>();
    format!(
        "{}:{}:{}@{}",
        p.first().unwrap_or(&""),
        p.get(1).unwrap_or(&""),
        p.get(3).unwrap_or(&""),
        ext
    )
}
pub fn maven_path(name: &str) -> R<String> {
    let (coord, ext) = name.split_once('@').unwrap_or((name, "jar"));
    let p = coord.split(':').collect::<Vec<_>>();
    if p.len() < 3 || p[..3].iter().any(|p| p.is_empty()) {
        return Err(format!("依赖坐标无效：{name}"));
    }
    Ok(format!(
        "{}/{}/{}/{}-{}{}.{}",
        p[0].replace('.', "/"),
        p[1],
        p[2],
        p[1],
        p[2],
        p.get(3).map(|s| format!("-{s}")).unwrap_or_default(),
        ext
    ))
}
fn coordinate(v: &Value, prefix: &str) -> Option<String> {
    list(&v["libraries"]).iter().find_map(|lib| {
        let n = s(lib, "name");
        if n.starts_with(prefix) {
            n.split(':').nth(2).map(str::to_string)
        } else {
            None
        }
    })
}
pub fn describe_version(v: &Value) -> Value {
    let quilt = coordinate(v, "org.quiltmc:quilt-loader:");
    let fabric = coordinate(v, "net.fabricmc:fabric-loader:");
    let neo =
        coordinate(v, "net.neoforged:neoforge:").or_else(|| coordinate(v, "net.neoforged:forge:"));
    let forge = coordinate(v, "net.minecraftforge:forge:")
        .or_else(|| coordinate(v, "net.minecraftforge:minecraftforge:"));
    let opti =
        coordinate(v, "optifine:OptiFine:").or_else(|| coordinate(v, "net.optifine:optifine:"));
    let lite = coordinate(v, "com.mumfrey:liteloader:");
    let mut labels = Vec::new();
    if let Some(x) = &quilt {
        labels.push(format!("Quilt {x}"));
    } else if let Some(x) = &fabric {
        labels.push(format!("Fabric {x}"));
    }
    if let Some(x) = &neo {
        labels.push(format!("NeoForge {x}"));
    } else if let Some(x) = &forge {
        labels.push(format!("Forge {x}"));
    }
    if let Some(x) = &opti {
        labels.push(format!("OptiFine {x}"));
    }
    if let Some(x) = &lite {
        labels.push(format!("LiteLoader {x}"));
    }
    let args = list(&v["arguments"]["game"])
        .into_iter()
        .flat_map(|item| {
            if item.is_string() {
                vec![item]
            } else if item["value"].is_array() {
                list(&item["value"])
            } else {
                vec![item["value"].clone()]
            }
        })
        .collect::<Vec<_>>();
    let explicit = args
        .iter()
        .position(|x| x.as_str() == Some("--fml.mcVersion"))
        .and_then(|i| args.get(i + 1))
        .and_then(Value::as_str)
        .filter(|s| !s.contains("${"))
        .map(str::to_string);
    let version_coord = forge
        .as_ref()
        .or(neo.as_ref())
        .or(opti.as_ref())
        .and_then(|name| {
            Regex::new(r"^(1\.\d+(?:\.\d+)?)(?:[-_]|$)")
                .unwrap()
                .captures(name)
                .map(|c| c[1].to_string())
        });
    let minecraft = coordinate(v, "net.fabricmc:intermediary:")
        .or(explicit)
        .or(version_coord)
        .or_else(|| {
            [s(v, "minecraftId"), s(v, "jarId"), s(v, "id")]
                .into_iter()
                .find(|s| !s.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_default();
    let release = Regex::new(r"^1\.(\d+)(?:\.(\d+))?$")
        .unwrap()
        .captures(&minecraft)
        .map(|c| {
            (
                c[1].parse::<u32>().unwrap_or(0),
                c.get(2)
                    .and_then(|m| m.as_str().parse::<u32>().ok())
                    .unwrap_or(0),
            )
        });
    let recommended = v["javaVersion"]["majorVersion"]
        .as_u64()
        .unwrap_or_else(|| match release {
            Some((m, p)) if m >= 21 || m == 20 && p >= 5 => 21,
            Some((m, _)) if m >= 18 => 17,
            Some((17, _)) => 16,
            _ => 8,
        });
    let legacy = forge.is_some() && release.is_some_and(|(m, _)| m <= 16) && recommended == 8;
    json!({"minecraft":minecraft,"loader":if labels.is_empty(){"原版".to_string()}else{labels.join(" + ")},"javaRequired":recommended,"javaRecommended":recommended,"javaMaximum":if legacy{json!(8)}else{Value::Null},"modded":!labels.is_empty()})
}
pub fn game_directory(root: &Path, id: &str, isolation: &Value) -> R<PathBuf> {
    valid_id(id)?;
    let dir = inside(root, &format!("versions/{id}"))?;
    if isolation == &json!(true) {
        return Ok(dir);
    }
    if isolation == &json!(false) {
        return Ok(root.into());
    }
    if ["mods", "config", "saves", "resourcepacks"]
        .iter()
        .any(|name| dir.join(name).is_dir())
    {
        Ok(dir)
    } else {
        Ok(root.into())
    }
}
pub fn normalize_game_root(selected: &str) -> R<PathBuf> {
    if !Path::new(selected).is_absolute() {
        return Err("请选择游戏文件夹。".into());
    }
    let root = abs(selected);
    for candidate in [
        root.clone(),
        root.join(".minecraft"),
        root.parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_default(),
    ] {
        if candidate.join("versions").is_dir() {
            return Ok(candidate);
        }
    }
    Err("这里没有 versions 文件夹。请选择已安装的整合包目录或 .minecraft 文件夹。".into())
}
pub fn allowed(rules: &Value, features: &Value) -> bool {
    let rules = list(rules);
    if rules.is_empty() {
        return true;
    }
    let mut allow = false;
    for rule in rules {
        let os = &rule["os"];
        if !s(os, "name").is_empty() && s(os, "name") != "windows" {
            continue;
        }
        if !s(os, "arch").is_empty() && s(os, "arch") != "x86_64" {
            continue;
        }
        if !s(os, "version").is_empty()
            && !Regex::new(s(os, "version"))
                .map(|r| {
                    r.is_match(&format!(
                        "10.0.{}",
                        System::kernel_version().unwrap_or_default()
                    ))
                })
                .unwrap_or(false)
        {
            continue;
        }
        if rule["features"].as_object().is_some_and(|map| {
            map.iter().any(|(key, value)| {
                features[key].as_bool().unwrap_or(false) != value.as_bool().unwrap_or(false)
            })
        }) {
            continue;
        }
        allow = s(&rule, "action") == "allow";
    }
    allow
}
pub fn native_arch(name: &str) -> bool {
    !name.contains(":natives-windows-arm64") && !name.contains(":natives-windows-x86")
}
pub fn offline_uuid(name: &str) -> String {
    let mut hash = Md5::digest(format!("OfflinePlayer:{name}").as_bytes());
    hash[6] = (hash[6] & 15) | 48;
    hash[8] = (hash[8] & 63) | 128;
    hex::encode(hash)
}
pub fn expand_arguments(items: &Value, variables: &Value, features: &Value) -> R<Vec<String>> {
    let mut output = Vec::new();
    let regex = Regex::new(r"\$\{([^}]+)\}").unwrap();
    for item in list(items) {
        if !item.is_string() && !allowed(&item["rules"], features) {
            continue;
        }
        let values = if item.is_string() {
            vec![item]
        } else if item["value"].is_array() {
            list(&item["value"])
        } else {
            vec![item["value"].clone()]
        };
        for value in values {
            let input = value.as_str().ok_or("版本配置包含无效启动参数。")?;
            for capture in regex.captures_iter(input) {
                if variables.get(&capture[1]).is_none() {
                    return Err(format!("该版本需要暂不支持的启动参数：{}", &capture[1]));
                }
            }
            output.push(
                regex
                    .replace_all(input, |c: &regex::Captures| text(&variables[&c[1]]))
                    .into_owned(),
            );
        }
    }
    Ok(output)
}
pub fn tokenize(input: &str) -> Vec<String> {
    Regex::new(r#""(?:[^"\\]|\\.)*"|\S+"#)
        .unwrap()
        .find_iter(input)
        .map(|m| {
            let s = m.as_str();
            if s.starts_with('"') && s.ends_with('"') {
                s[1..s.len() - 1].into()
            } else {
                s.into()
            }
        })
        .collect()
}
pub fn quote_argument(arg: &str) -> String {
    format!(
        "\"{}\"",
        arg.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\r', "\\r")
            .replace('\n', "\\n")
            .replace('\t', "\\t")
    )
}
fn quiet_command(file: &Path) -> Command {
    let mut c = Command::new(file);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    c
}
pub fn probe_java(input: &str) -> R<Value> {
    let mut path = abs(input);
    if path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("javaw.exe"))
    {
        path.set_file_name("java.exe");
    }
    if input.is_empty() || !path.is_file() {
        return Err("找不到 Java 可执行文件，请在设置中重新选择。".into());
    }
    let mut child = quiet_command(&path)
        .args(["-XshowSettings:properties", "-version"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(err)?;
    let out = child.stdout.take().unwrap();
    let error = child.stderr.take().unwrap();
    let t1 = thread::spawn(move || {
        let mut data = Vec::new();
        let _ = out.take(262144).read_to_end(&mut data);
        data
    });
    let t2 = thread::spawn(move || {
        let mut data = Vec::new();
        let _ = error.take(262144).read_to_end(&mut data);
        data
    });
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if child.try_wait().map_err(err)?.is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Java 检测超时。".into());
        }
        thread::sleep(Duration::from_millis(25));
    }
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&t1.join().unwrap_or_default()),
        String::from_utf8_lossy(&t2.join().unwrap_or_default())
    );
    let property = |key: &str| {
        Regex::new(&format!(r"{}\s*=\s*(\S+)", regex::escape(key)))
            .unwrap()
            .captures(&output)
            .map(|c| c[1].to_string())
    };
    let version = property("java.version")
        .or_else(|| {
            Regex::new(r#"version\s+"([^"]+)""#)
                .unwrap()
                .captures(&output)
                .map(|c| c[1].to_string())
        })
        .ok_or("无法识别这个 Java 的版本。")?;
    let major = if version.starts_with("1.") {
        version.split('.').nth(1).unwrap_or("0")
    } else {
        version.split(['.', '-', '+']).next().unwrap_or("0")
    }
    .parse::<u64>()
    .unwrap_or(0);
    let bits = property("sun.arch.data.model")
        .and_then(|p| p.parse::<u64>().ok())
        .unwrap_or(if output.contains("64-Bit") { 64 } else { 32 });
    let encoding = property("native.encoding")
        .or_else(|| property("sun.jnu.encoding"))
        .unwrap_or("UTF-8".into());
    Ok(
        json!({"path":path_text(&path),"version":version,"major":major,"bits":bits,"encoding":encoding,"label":format!("Java {major} · {bits} 位 · {version}")}),
    )
}

pub struct Service {
    pub data_dir: PathBuf,
    pub auth: Arc<crate::auth::AuthManager>,
    pub modrinth: Arc<crate::modrinth::Manager>,
    inner: Mutex<Inner>,
    sink: Mutex<Option<Sink>>,
}
struct Inner {
    settings: Value,
    state: Value,
    logs: VecDeque<Value>,
    busy: bool,
    child: Option<Arc<Mutex<Child>>>,
    log_file: Option<File>,
    java_cache: Option<(Instant, Vec<Value>)>,
}
impl Service {
    pub fn new(data_dir: PathBuf) -> R<Arc<Self>> {
        fs::create_dir_all(&data_dir).map_err(err)?;
        let auth = crate::auth::AuthManager::new(data_dir.clone())?;
        let modrinth = crate::modrinth::Manager::new()?;
        let defaults = default_settings();
        let mut settings = read_json(&data_dir.join("settings.json"))
            .and_then(|mut old| {
                if old.get("javaMode").is_none() {
                    old["javaMode"] = json!(if s(&old, "javaPath").is_empty() {
                        "auto"
                    } else {
                        "manual"
                    });
                }
                validate(&merge(&defaults, &old))
            })
            .unwrap_or(validate(&defaults)?);
        let root = s(&settings, "gameRoot").to_string();
        if !root.is_empty()
            && !list(&settings["directories"])
                .iter()
                .any(|d| s(d, "path").eq_ignore_ascii_case(&root))
        {
            let directory = directory_record(&root, s(&settings, "versionId"), None);
            settings["directories"]
                .as_array_mut()
                .unwrap()
                .push(directory);
        }
        let java = s(&settings, "javaPath").to_string();
        if !java.is_empty()
            && !list(&settings["knownJavaPaths"])
                .iter()
                .any(|v| v.as_str() == Some(&java))
        {
            settings["knownJavaPaths"]
                .as_array_mut()
                .unwrap()
                .push(json!(java));
        }
        Ok(Arc::new(Self {
            data_dir,
            auth,
            modrinth,
            inner: Mutex::new(Inner {
                settings,
                state: json!({"phase":"idle","message":"准备开始你的下一段冒险","pid":null}),
                logs: VecDeque::new(),
                busy: false,
                child: None,
                log_file: None,
                java_cache: None,
            }),
            sink: Mutex::new(None),
        }))
    }
    pub fn set_sink(&self, sink: Sink) {
        *self.sink.lock().unwrap() = Some(sink);
    }
    fn emit(&self, kind: &str, value: Value) {
        if let Some(sink) = self.sink.lock().unwrap().clone() {
            sink(kind, value);
        }
    }
    pub fn settings(&self) -> Value {
        self.inner.lock().unwrap().settings.clone()
    }
    pub fn state(&self) -> Value {
        self.inner.lock().unwrap().state.clone()
    }
    pub fn has_child(&self) -> bool {
        self.inner.lock().unwrap().child.is_some()
    }
    pub fn ensure_account_editable(&self) -> R<()> {
        let inner = self.inner.lock().unwrap();
        if inner.busy || inner.child.is_some() {
            Err("游戏正在运行或准备中，请先结束游戏再更换账户。".into())
        } else {
            Ok(())
        }
    }
    pub(crate) fn state_to(&self, phase: &str, message: &str, pid: Option<u32>) {
        let state = json!({"phase":phase,"message":message,"pid":pid});
        self.inner.lock().unwrap().state = state.clone();
        self.emit("state", state);
    }
    pub(crate) fn content_progress(&self, progress: Value) {
        let total = progress["totalBytes"].as_u64().unwrap_or(1).max(1);
        let bytes = progress["bytes"].as_u64().unwrap_or(0);
        let message = format!("正在下载 {} · {}%", s(&progress,"filename"), bytes.saturating_mul(100)/total);
        let state = json!({"phase":"downloading","message":message,"pid":null,"progress":progress});
        self.inner.lock().unwrap().state=state.clone();
        self.emit("state",state);
    }
    pub fn log(&self, line: &str, level: &str) {
        let sanitized = self.auth.redact(line);
        let scrub = Regex::new(
            r#"(?i)(--accessToken\s+|access_token["'\s:=]+|refresh_token["'\s:=]+)[^\s,"']+"#,
        )
        .unwrap()
        .replace_all(&sanitized, "${1}[已隐藏]")
        .chars()
        .take(30000)
        .collect::<String>();
        let entry = json!({"time":now(),"text":scrub,"level":level});
        {
            let mut inner = self.inner.lock().unwrap();
            inner.logs.push_back(entry.clone());
            while inner.logs.len() > 2500 {
                inner.logs.pop_front();
            }
            if let Some(file) = inner.log_file.as_mut() {
                let _ = writeln!(
                    file,
                    "[{}] [{}] {}",
                    s(&entry, "time"),
                    level,
                    s(&entry, "text")
                );
            }
        }
        self.emit("log", entry);
    }
    pub(crate) fn begin(&self, phase: &str, message: &str) -> R<()> {
        let mut i = self.inner.lock().unwrap();
        if i.busy || i.child.is_some() {
            return Err("游戏已经在运行或准备中。".into());
        }
        i.busy = true;
        if phase=="downloading" {self.modrinth.reset_cancellation();}
        drop(i);
        self.state_to(phase, message, None);
        Ok(())
    }
    pub(crate) fn end(&self) {
        self.inner.lock().unwrap().busy = false;
    }
    pub fn save(&self, patch: Value) -> R<Value> {
        let mut inner = self.inner.lock().unwrap();
        if inner.busy || inner.child.is_some() {
            return Err("游戏正在运行或准备中，请稍后更改配置。".into());
        }
        let previous = inner.settings.clone();
        let mut profiles = previous["profiles"].clone();
        if !s(&previous, "gameRoot").is_empty() && !s(&previous, "versionId").is_empty() {
            profiles[profile_key(s(&previous, "gameRoot"), s(&previous, "versionId"))] =
                profile(&previous);
        }
        let switching = patch
            .get("gameRoot")
            .is_some_and(|v| v != &previous["gameRoot"])
            || patch
                .get("versionId")
                .is_some_and(|v| v != &previous["versionId"]);
        let mut next = merge(&previous, &patch);
        next["profiles"] = profiles.clone();
        if switching {
            let remembered = list(&previous["directories"])
                .into_iter()
                .find(|d| s(d, "path").eq_ignore_ascii_case(s(&next, "gameRoot")))
                .map(|d| s(&d, "lastVersionId").to_string())
                .unwrap_or_default();
            if patch.get("gameRoot").is_some() && patch.get("versionId").is_none() {
                next["versionId"] = json!(remembered.clone());
            }
            let saved = profiles
                .get(profile_key(s(&next, "gameRoot"), s(&next, "versionId")))
                .cloned()
                .unwrap_or(json!({}));
            next = merge(
                &merge(&merge(&next, &profile(&default_settings())), &saved),
                &patch,
            );
            next["profiles"] = profiles.clone();
            if patch.get("gameRoot").is_some() && patch.get("versionId").is_none() {
                next["versionId"] = json!(remembered);
            }
            inner.java_cache = None;
        }
        if patch.get("javaPath").is_some() && patch.get("javaMode").is_none() {
            next["javaMode"] = json!(if s(&patch, "javaPath").is_empty() {
                "auto"
            } else {
                "manual"
            });
        }
        if !s(&patch, "javaPath").is_empty() {
            let mut paths = list(&previous["knownJavaPaths"]);
            if !paths
                .iter()
                .any(|v| v.as_str() == Some(s(&patch, "javaPath")))
            {
                paths.push(patch["javaPath"].clone());
            }
            next["knownJavaPaths"] = json!(paths);
            inner.java_cache = None;
        }
        if !s(&next, "gameRoot").is_empty() {
            let root = s(&next, "gameRoot");
            let old = list(&previous["directories"])
                .into_iter()
                .find(|d| s(d, "path").eq_ignore_ascii_case(root));
            let d = directory_record(
                root,
                s(&next, "versionId"),
                old.as_ref().map(|v| s(v, "name")),
            );
            let mut dirs = list(&previous["directories"])
                .into_iter()
                .filter(|d| !s(d, "path").eq_ignore_ascii_case(root))
                .collect::<Vec<_>>();
            dirs.push(d);
            next["directories"] = json!(dirs);
        }
        let mut checked = validate(&next)?;
        if !s(&checked, "gameRoot").is_empty() && !s(&checked, "versionId").is_empty() {
            let key = profile_key(s(&checked, "gameRoot"), s(&checked, "versionId"));
            let p = profile(&checked);
            checked["profiles"][key] = p;
        }
        let temporary = self.data_dir.join("settings.json.tmp");
        fs::write(
            &temporary,
            serde_json::to_vec_pretty(&checked).map_err(err)?,
        )
        .map_err(err)?;
        fs::rename(&temporary, self.data_dir.join("settings.json")).map_err(err)?;
        inner.settings = checked.clone();
        Ok(checked)
    }
    pub fn add_directory(&self, selected: &str) -> R<Value> {
        let root = normalize_game_root(selected)?;
        self.save(json!({"gameRoot":path_text(&root)}))?;
        let versions = self.versions();
        let config = self.settings();
        if !versions
            .iter()
            .any(|v| s(v, "id") == s(&config, "versionId") && v.get("error").is_none())
        {
            let id = versions
                .iter()
                .find(|v| v.get("error").is_none())
                .map(|v| s(v, "id"))
                .unwrap_or("");
            self.save(json!({"versionId":id}))?;
        }
        self.snapshot()
    }
    pub fn versions(&self) -> Vec<Value> {
        let config = self.settings();
        let root = Path::new(s(&config, "gameRoot"));
        if s(&config, "gameRoot").is_empty() {
            return vec![];
        }
        let mut names = fs::read_dir(root.join("versions"))
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        names.sort();
        names.into_iter().take(150).map(|id|{let result=(||{let version=resolve_version(root,&id)?;let info=describe_version(&version);let key=profile_key(s(&config,"gameRoot"),&id);let isolation=if id==s(&config,"versionId"){config["isolated"].clone()}else{config["profiles"][key]["isolated"].clone()};let game=game_directory(root,&id,&isolation)?;let mods=fs::read_dir(game.join("mods")).ok().into_iter().flatten().filter_map(Result::ok).filter(|e|e.file_name().to_string_lossy().ends_with(".jar")).count();Ok::<Value,String>(merge(&info,&json!({"id":id,"modCount":mods,"gameDir":path_text(&game),"isolated":game!=root,"supported":!s(&version,"mainClass").is_empty(),"mainClass":version["mainClass"]})))})();result.unwrap_or_else(|error|json!({"id":id,"error":error,"supported":false}))}).collect()
    }
    pub fn snapshot(&self) -> R<Value> {
        let config = self.settings();
        let mut suggestions = Vec::new();
        let home = std::env::var("USERPROFILE").unwrap_or_default();
        let mut roots = vec![
            PathBuf::from(&home).join(".minecraft"),
            self.data_dir
                .parent()
                .unwrap_or(Path::new("."))
                .join(".minecraft"),
        ];
        if let Ok(appdata) = std::env::var("APPDATA") {
            roots.insert(0, PathBuf::from(appdata).join(".minecraft"));
        }
        for candidate in roots {
            if let Ok(root) = normalize_game_root(&path_text(&candidate)) {
                let root = path_text(&root);
                if !list(&config["directories"])
                    .iter()
                    .any(|d| s(d, "path").eq_ignore_ascii_case(&root))
                    && !suggestions.contains(&root)
                {
                    suggestions.push(root);
                }
            }
        }
        let versions = self.versions();
        let inner = self.inner.lock().unwrap();
        let logs = inner
            .logs
            .iter()
            .skip(inner.logs.len().saturating_sub(500))
            .cloned()
            .collect::<Vec<_>>();
        Ok(
            json!({"settings":config,"account":self.auth.summary(),"versions":versions,"suggestions":suggestions,"state":inner.state,"logs":logs,"system":system(),"dataDir":path_text(&self.data_dir)}),
        )
    }
    pub fn mods(&self) -> R<Value> {
        let config = self.settings();
        if s(&config, "versionId").is_empty() {
            return Ok(json!([]));
        }
        let game = game_directory(
            Path::new(s(&config, "gameRoot")),
            s(&config, "versionId"),
            &config["isolated"],
        )?;
        let mut files = fs::read_dir(game.join("mods"))
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let metadata = e.metadata().ok()?;
                if !metadata.is_file()
                    || !name.to_lowercase().ends_with(".jar")
                        && !name.to_lowercase().ends_with(".jar.disabled")
                {
                    return None;
                }
                Some(json!({"name":name,"size":metadata.len(),"enabled":name.ends_with(".jar")}))
            })
            .collect::<Vec<_>>();
        files.sort_by(|a, b| s(a, "name").cmp(s(b, "name")));
        Ok(json!(files))
    }
}
fn directory_record(root: &str, id: &str, existing: Option<&str>) -> Value {
    let p = abs(root);
    let dir = if p.file_name().is_some_and(|n| n == ".minecraft") {
        p.parent().unwrap_or(&p)
    } else {
        &p
    };
    json!({"path":path_text(&p),"name":existing.filter(|s|!s.is_empty()).map(str::to_string).unwrap_or_else(||dir.file_name().unwrap_or_default().to_string_lossy().into_owned()),"lastVersionId":id})
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    #[serde(skip)]
    pub entry_index: usize,
    #[serde(skip)]
    pub sensitive_token: Option<String>,
    pub config: Value,
    pub version: Value,
    pub game_dir: PathBuf,
    pub native_dir: PathBuf,
    pub native_jars: Vec<PathBuf>,
    pub classpath: Vec<PathBuf>,
    pub files: Vec<FileItem>,
    pub args: Vec<String>,
}
#[derive(Clone, serde::Serialize)]
pub struct FileItem {
    pub file: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
fn optional(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}
fn item(file: PathBuf, meta: &Value, label: String, fallback: Option<String>) -> FileItem {
    FileItem {
        file,
        sha1: optional(meta, "sha1"),
        url: optional(meta, "url").or(fallback),
        label,
        reason: None,
    }
}
fn sha1_file(path: &Path) -> R<String> {
    let mut file = File::open(path).map_err(err)?;
    let mut hash = Sha1::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(err)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}

impl Service {
    pub fn javas(&self, force: bool) -> R<Vec<Value>> {
        let config = self.settings();
        if !force {
            if let Some((time, list)) = &self.inner.lock().unwrap().java_cache {
                if time.elapsed() < Duration::from_secs(30) {
                    return Ok(list.clone());
                }
            }
        }
        let mut candidates = Vec::new();
        let mut seen = HashSet::new();
        let mut add = |p: PathBuf| {
            if p.is_file() && seen.insert(path_text(&p).to_lowercase()) {
                candidates.push(p);
            }
        };
        if !s(&config, "javaPath").is_empty() {
            add(abs(s(&config, "javaPath")));
        }
        for known in list(&config["knownJavaPaths"]) {
            if let Some(p) = known.as_str() {
                add(abs(p));
            }
        }
        if let Ok(home) = std::env::var("JAVA_HOME") {
            add(PathBuf::from(home).join("bin/java.exe"));
        }
        for path in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
            add(path.join("java.exe"));
        }
        for base in [
            std::env::var_os("ProgramFiles"),
            std::env::var_os("ProgramFiles(x86)"),
            std::env::var_os("USERPROFILE")
                .map(|p| PathBuf::from(p).join(".jdks").into_os_string()),
        ]
        .into_iter()
        .flatten()
        {
            let base = PathBuf::from(base);
            for entry in fs::read_dir(&base)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
            {
                if !entry.path().is_dir()
                    || !Regex::new("(?i)java|jdk|jre|adoptium|zulu|bellsoft|microsoft|amazon")
                        .unwrap()
                        .is_match(&entry.file_name().to_string_lossy())
                {
                    continue;
                }
                add(entry.path().join("bin/java.exe"));
                for child in fs::read_dir(entry.path())
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
                    .take(40)
                {
                    if child.path().is_dir() {
                        add(child.path().join("bin/java.exe"));
                    }
                }
            }
        }
        if !s(&config, "gameRoot").is_empty() {
            let root = abs(s(&config, "gameRoot"));
            for base in [root.clone(), root.parent().unwrap_or(&root).into()] {
                for folder in ["runtime", "java", "jre", "jdk"] {
                    let mut queue = VecDeque::from([(base.join(folder), 0usize)]);
                    let mut visited = 0;
                    while let Some((dir, depth)) = queue.pop_front() {
                        visited += 1;
                        if visited > 80 {
                            break;
                        }
                        add(dir.join("bin/java.exe"));
                        add(dir.join("java.exe"));
                        if depth >= 4 {
                            continue;
                        }
                        for child in fs::read_dir(&dir)
                            .ok()
                            .into_iter()
                            .flatten()
                            .filter_map(Result::ok)
                        {
                            if child.path().is_dir()
                                && !Regex::new("(?i)legal|lib|include|jmods|man|conf")
                                    .unwrap()
                                    .is_match(&child.file_name().to_string_lossy())
                            {
                                queue.push_back((child.path(), depth + 1));
                            }
                        }
                    }
                }
            }
        }
        let mut found = Vec::new();
        for group in candidates
            .into_iter()
            .take(30)
            .collect::<Vec<_>>()
            .chunks(4)
        {
            let handles = group
                .iter()
                .cloned()
                .map(|p| thread::spawn(move || probe_java(&path_text(&p))))
                .collect::<Vec<_>>();
            for handle in handles {
                if let Ok(Ok(java)) = handle.join() {
                    if !found
                        .iter()
                        .any(|v| s(v, "path").eq_ignore_ascii_case(s(&java, "path")))
                    {
                        found.push(java);
                    }
                }
            }
        }
        found.sort_by_key(|java| std::cmp::Reverse(java["major"].as_u64().unwrap_or(0)));
        self.inner.lock().unwrap().java_cache = Some((Instant::now(), found.clone()));
        Ok(found)
    }
    pub fn select_java(&self, version: &Value) -> R<Value> {
        let config = self.settings();
        if s(&config, "javaMode") == "manual" {
            return probe_java(s(&config, "javaPath"));
        }
        let info = describe_version(version);
        let required = info["javaRequired"].as_u64().unwrap_or(8);
        let max = info["javaMaximum"].as_u64();
        let mut suitable = self
            .javas(false)?
            .into_iter()
            .filter(|v| {
                v["bits"] == 64
                    && v["major"]
                        .as_u64()
                        .is_some_and(|n| n >= required && max.is_none_or(|m| n <= m))
            })
            .collect::<Vec<_>>();
        suitable.sort_by_key(|v| v["major"].as_u64().unwrap_or(0));
        suitable
            .iter()
            .find(|v| v["major"] == info["javaRecommended"])
            .cloned()
            .or_else(|| suitable.first().cloned())
            .ok_or(format!(
                "没有找到适用的 64 位 Java {required}，请在启动设置中手动选择或安装。"
            ))
    }
    pub fn build_plan(&self) -> R<Plan> {
        let identity = self.auth.preview()?;
        self.build_plan_with_identity(identity.as_ref())
    }
    pub(crate) fn build_plan_with_identity(
        &self,
        identity: Option<&crate::auth::Identity>,
    ) -> R<Plan> {
        let config = validate(&self.settings())?;
        let root = Path::new(s(&config, "gameRoot"));
        let id = s(&config, "versionId");
        if s(&config, "gameRoot").is_empty() || id.is_empty() {
            return Err("请先添加游戏目录并选择一个已安装的版本。".into());
        }
        let version = resolve_version(root, id)?;
        if !Regex::new(r"^[\w.$]+$")
            .unwrap()
            .is_match(s(&version, "mainClass"))
        {
            return Err("游戏入口无效。".into());
        }
        let game = game_directory(root, id, &config["isolated"])?;
        let hash = hex::encode(Sha256::digest(
            format!("{}\0{}", s(&config, "gameRoot"), id).as_bytes(),
        ));
        let native_dir = inside(&self.data_dir, &format!("natives/{}", &hash[..16]))?;
        let mut classpath = Vec::new();
        let mut natives = Vec::new();
        let mut files = Vec::new();
        for lib in list(&version["libraries"]) {
            let name = s(&lib, "name");
            if lib["clientreq"] == false
                || !allowed(&lib["rules"], &json!({}))
                || !native_arch(name)
            {
                continue;
            }
            let artifact = &lib["downloads"]["artifact"];
            let relative = optional(artifact, "path").unwrap_or(maven_path(name)?);
            let file = inside(root, &format!("libraries/{relative}"))?;
            classpath.push(file.clone());
            files.push(item(
                file.clone(),
                artifact,
                name.into(),
                Some(format!(
                    "{}{}",
                    optional(&lib, "url").unwrap_or("https://libraries.minecraft.net/".into()),
                    relative
                )),
            ));
            if name.contains(":natives-") {
                natives.push(file);
            }
            if let Some(classifier) = optional(&lib["natives"], "windows") {
                let classifier = classifier.replace("${arch}", "64");
                let native = &lib["downloads"]["classifiers"][classifier];
                let rel = optional(native, "path").ok_or(format!("找不到本机依赖配置：{name}"))?;
                let file = inside(root, &format!("libraries/{rel}"))?;
                natives.push(file.clone());
                files.push(item(file, native, format!("{name} 本机依赖"), None));
            }
        }
        let jar = s(&version, "jarId");
        valid_id(jar)?;
        let client = inside(root, &format!("versions/{jar}/{jar}.jar"))?;
        classpath.push(client.clone());
        files.push(item(
            client.clone(),
            &version["downloads"]["client"],
            "Minecraft 游戏文件".into(),
            None,
        ));
        let assets = optional(&version["assetIndex"], "id")
            .or_else(|| optional(&version, "assets"))
            .unwrap_or("legacy".into());
        let index = inside(root, &format!("assets/indexes/{assets}.json"))?;
        let asset_data = read_json(&index).unwrap_or(Value::Null);
        files.push(item(index, &version["assetIndex"], "资源索引".into(), None));
        let cp = classpath
            .iter()
            .map(|p| path_text(p))
            .collect::<Vec<_>>()
            .join(";");
        let mut variables = json!({"auth_player_name":s(&config,"username"),"auth_uuid":offline_uuid(s(&config,"username")),"auth_access_token":"0","user_type":"legacy","user_properties":"{}","auth_session":"0","version_name":id,"game_directory":path_text(&game),"assets_root":path_text(&inside(root,"assets")?),"assets_index_name":assets,"version_type":optional(&version,"type").unwrap_or("release".into()),"launcher_name":"XUMOB Minecraft Launcher","launcher_version":"0.5.0","natives_directory":path_text(&native_dir),"library_directory":path_text(&inside(root,"libraries")?),"classpath_separator":";","classpath":cp,"clientid":"","auth_xuid":"","resolution_width":config["width"],"resolution_height":config["height"],"game_assets":path_text(&if asset_data["map_to_resources"]==true{inside(&game,"resources")?}else{inside(root,&format!("assets/virtual/{assets}"))?}),"primary_jar":path_text(&client)});
        if let Some(identity) = identity {
            variables = merge(&variables, &identity.variables());
        }
        let features = json!({"has_custom_resolution":true});
        let jvm = if list(&version["arguments"]["jvm"]).is_empty() {
            vec![
                format!("-Djava.library.path={}", path_text(&native_dir)),
                "-cp".into(),
                cp,
            ]
        } else {
            expand_arguments(&version["arguments"]["jvm"], &variables, &features)?
        };
        let fixed = jvm
            .iter()
            .map(|arg| {
                Regex::new(r"^(-D[^=\s]+=)\s+")
                    .unwrap()
                    .replace(arg, "$1")
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>();
        let game_args = if list(&version["arguments"]["game"]).is_empty() {
            expand_arguments(
                &json!(tokenize(s(&version, "minecraftArguments"))),
                &variables,
                &features,
            )?
        } else {
            expand_arguments(&version["arguments"]["game"], &variables, &features)?
        };
        let extra = tokenize(s(&config, "extraJvmArgs"));
        if extra.iter().any(|arg| {
            Regex::new(r"(?i)^-Xm[sx]").unwrap().is_match(arg)
                || ["-jar", "-cp", "-classpath", "--class-path"].contains(&arg.as_str())
        }) {
            return Err("附加 JVM 参数请不要重写内存、classpath 或游戏入口。".into());
        }
        let memory = config["memoryMB"].as_u64().unwrap();
        let mut args = vec![
            format!("-Xms{}M", memory.min(1024)),
            format!("-Xmx{memory}M"),
            "-XX:+UseG1GC".into(),
            "-Dfile.encoding=UTF-8".into(),
            "-Dstdout.encoding=UTF-8".into(),
            "-Dstderr.encoding=UTF-8".into(),
            "-Dsun.stdout.encoding=UTF-8".into(),
            "-Dsun.stderr.encoding=UTF-8".into(),
        ];
        args.extend(fixed);
        args.extend(extra);
        let entry_index = args.len();
        args.push(s(&version, "mainClass").into());
        args.extend(game_args);
        args.extend(tokenize(s(&config, "extraGameArgs")));
        let mut seen = HashSet::new();
        natives.retain(|p| seen.insert(p.clone()));
        Ok(Plan {
            entry_index,
            sensitive_token: identity
                .filter(|i| i.access_token != "0")
                .map(|i| i.access_token.clone()),
            config,
            version,
            game_dir: game,
            native_dir,
            native_jars: natives,
            classpath,
            files,
            args,
        })
    }
    pub fn inspect_files(&self, plan: &Plan, verify: bool) -> R<Value> {
        let mut missing = Vec::new();
        for file in &plan.files {
            let reason = if !file.file.is_file() {
                Some("缺失")
            } else if verify
                && file.sha1.as_ref().is_some_and(|sha| {
                    sha1_file(&file.file)
                        .map(|h| h != sha.to_lowercase())
                        .unwrap_or(true)
                })
            {
                Some("校验不一致")
            } else {
                None
            };
            if let Some(reason) = reason {
                let mut item = file.clone();
                item.reason = Some(reason.into());
                missing.push(item);
            }
        }
        let index = plan.files.iter().find(|f| f.label == "资源索引").unwrap();
        let mut count = 0;
        if index.file.is_file() && !missing.iter().any(|item| item.file == index.file) {
            let data = read_json(&index.file)?;
            if let Some(objects) = data["objects"].as_object() {
                for object in objects.values() {
                    let hash = s(object, "hash");
                    if !Regex::new("^[a-f0-9]{40}$").unwrap().is_match(hash) {
                        return Err("资源索引中有无效的文件哈希。".into());
                    }
                    count += 1;
                    let file = inside(
                        Path::new(s(&plan.config, "gameRoot")),
                        &format!("assets/objects/{}/{hash}", &hash[..2]),
                    )?;
                    let size = fs::metadata(&file).ok().map(|m| m.len());
                    if size != object["size"].as_u64() {
                        missing.push(FileItem {
                            file,
                            sha1: Some(hash.into()),
                            url: Some(format!(
                                "https://resources.download.minecraft.net/{}/{hash}",
                                &hash[..2]
                            )),
                            label: format!("资源 {}", &hash[..10]),
                            reason: Some(
                                if size.is_none() {
                                    "缺失"
                                } else {
                                    "文件大小不一致"
                                }
                                .into(),
                            ),
                        });
                    }
                }
            }
        }
        Ok(
            json!({"missing":missing,"assetCount":count,"libraryCount":plan.classpath.len().saturating_sub(1)}),
        )
    }
    pub fn check(&self) -> R<Value> {
        self.begin("checking", "正在检查 Java、游戏文件和资源…")?;
        let result: R<Value> = (|| {
            let plan = self.build_plan()?;
            let java = self
                .select_java(&plan.version)
                .unwrap_or_else(|error| json!({"error":error}));
            let files = self.inspect_files(&plan, true)?;
            let info = describe_version(&plan.version);
            let required = info["javaRequired"].as_u64().unwrap_or(8);
            let maximum = info["javaMaximum"].as_u64();
            let java_ok = java.get("error").is_none()
                && java["bits"] == 64
                && java["major"]
                    .as_u64()
                    .is_some_and(|n| n >= required && maximum.is_none_or(|m| n <= m));
            let n = list(&files["missing"]).len();
            self.state_to(
                "idle",
                if n > 0 || !java_ok {
                    "检查完成，有项目需要处理"
                } else {
                    "检查通过，可以启动游戏"
                },
                None,
            );
            self.log(
                &format!(
                    "环境检查：{} 个依赖、{} 个资源，{n} 个文件需要补全。",
                    files["libraryCount"], files["assetCount"]
                ),
                "info",
            );
            Ok(merge(
                &files,
                &json!({"java":java,"required":required,"maximum":maximum,"javaOk":java_ok,"ok":java_ok&&n==0}),
            ))
        })();
        self.end();
        if let Err(error) = &result {
            self.state_to("error", error, None);
        }
        result
    }
    pub fn prepare_assets(&self, plan: &Plan) -> R<()> {
        let index = read_json(
            &plan
                .files
                .iter()
                .find(|f| f.label == "资源索引")
                .unwrap()
                .file,
        )?;
        if index["virtual"] != true && index["map_to_resources"] != true {
            return Ok(());
        }
        let root = Path::new(s(&plan.config, "gameRoot"));
        let target = if index["map_to_resources"] == true {
            inside(&plan.game_dir, "resources")?
        } else {
            let id = optional(&plan.version["assetIndex"], "id")
                .or_else(|| optional(&plan.version, "assets"))
                .unwrap_or("legacy".into());
            inside(root, &format!("assets/virtual/{id}"))?
        };
        if let Some(objects) = index["objects"].as_object() {
            for (name, object) in objects {
                let file = inside(&target, name)?;
                if file.is_file() {
                    continue;
                }
                let hash = s(object, "hash");
                if hash.len() != 40 {
                    return Err("无效资源哈希。".into());
                }
                let source = inside(root, &format!("assets/objects/{}/{hash}", &hash[..2]))?;
                fs::create_dir_all(file.parent().unwrap()).map_err(err)?;
                fs::copy(source, file).map_err(err)?;
            }
        }
        Ok(())
    }
    pub fn invocation(&self, plan: &Plan, java: &Value) -> R<(Vec<String>, Option<String>)> {
        if plan.args.join(" ").encode_utf16().count() <= 22000 {
            return Ok((plan.args.clone(), None));
        }
        if java["major"].as_u64().unwrap_or(0) >= 9 {
            if let Some(secret) = plan.sensitive_token.as_deref() {
                let mut command_args = Vec::new();
                let mut chunk = Vec::new();
                let mut part = 0;
                let encoding = encoding_rs::Encoding::for_label(s(java, "encoding").as_bytes())
                    .ok_or("不支持当前 Java 参数文件编码。")?;
                let mut flush = |chunk: &mut Vec<String>, output: &mut Vec<String>| -> R<()> {
                    if chunk.is_empty() {
                        return Ok(());
                    }
                    let text = chunk
                        .iter()
                        .map(|a| quote_argument(a))
                        .collect::<Vec<_>>()
                        .join("\n");
                    let (bytes, _, errors) = encoding.encode(&text);
                    if errors {
                        return Err(
                            "当前 Java 编码无法表示游戏路径，请调整路径或使用 UTF-8 环境。".into(),
                        );
                    }
                    let file = self.data_dir.join(format!("launch-arguments-{part}.txt"));
                    part += 1;
                    fs::write(&file, &bytes).map_err(err)?;
                    output.push(format!("@{}", path_text(&file)));
                    chunk.clear();
                    Ok(())
                };
                let mut contains_entry = false;
                let mut files_finished = false;
                for (index, arg) in plan.args.iter().enumerate() {
                    if files_finished {
                        command_args.push(arg.clone());
                        continue;
                    }
                    if arg.contains(secret) {
                        flush(&mut chunk, &mut command_args)?;
                        // Java stops expanding subsequent @files after the main class.
                        // Preserve the remaining game arguments directly in their order.
                        files_finished = contains_entry;
                        command_args.push(arg.clone());
                    } else {
                        chunk.push(arg.clone());
                        if index == plan.entry_index {
                            contains_entry = true;
                        }
                    }
                }
                flush(&mut chunk, &mut command_args)?;
                if command_args.join(" ").encode_utf16().count() > 28000 {
                    return Err("正版启动参数过长，请缩短游戏目录或附加参数。".into());
                }
                return Ok((command_args, None));
            }
            let text = plan
                .args
                .iter()
                .map(|a| quote_argument(a))
                .collect::<Vec<_>>()
                .join("\n");
            let encoding = encoding_rs::Encoding::for_label(s(java, "encoding").as_bytes()).ok_or(
                format!("暂不支持 Java 参数文件编码：{}", s(java, "encoding")),
            )?;
            let (encoded, _, errors) = encoding.encode(&text);
            if errors {
                return Err(
                    "当前 Java 的系统编码无法表示游戏路径，请使用 UTF-8 环境或调整路径。".into(),
                );
            }
            let file = self.data_dir.join("launch-arguments.txt");
            fs::write(&file, &encoded).map_err(err)?;
            return Ok((vec![format!("@{}", path_text(&file))], None));
        }
        let mut args = plan.args.clone();
        let i = args
            .iter()
            .position(|a| ["-cp", "-classpath", "--class-path"].contains(&a.as_str()))
            .ok_or("旧 Java 的启动参数过长，请缩短游戏目录路径。")?;
        let cp = args.get(i + 1).ok_or("缺少 classpath。")?.clone();
        args.drain(i..i + 2);
        if args.join(" ").encode_utf16().count() > 28000 {
            return Err("旧 Java 的启动参数过长，请缩短游戏目录路径。".into());
        }
        Ok((args, Some(cp)))
    }
    pub fn launch(self: &Arc<Self>) -> R<Value> {
        self.begin("preparing", "正在准备游戏环境…")?;
        let result = (|| {
            let mut plan = self.build_plan()?;
            let java = self.select_java(&plan.version)?;
            let info = describe_version(&plan.version);
            let required = info["javaRequired"].as_u64().unwrap_or(8);
            let major = java["major"].as_u64().unwrap_or(0);
            if major < required {
                return Err(format!(
                    "这个版本需要 Java {required} 或更高版本，当前是 Java {major}。"
                ));
            }
            if info["javaMaximum"].as_u64().is_some_and(|m| major > m) {
                return Err(format!(
                    "这个旧版 Forge 配置需要 Java {}，当前是 Java {major}。",
                    info["javaMaximum"]
                ));
            }
            if java["bits"] != 64 {
                return Err("请为整合包选择 64 位 Java。".into());
            }
            let check = self.inspect_files(&plan, false)?;
            let missing = list(&check["missing"]).len();
            if missing > 0 {
                return Err(format!(
                    "有 {missing} 个游戏依赖或资源缺失，请先点击“检查环境”并补全。"
                ));
            }
            let identity = self.auth.identity_for_launch()?;
            if identity.is_some() {
                plan = self.build_plan_with_identity(identity.as_ref())?;
            }
            fs::create_dir_all(&plan.native_dir).map_err(err)?;
            for jar in &plan.native_jars {
                extract_natives(jar, &plan.native_dir)?;
            }
            self.prepare_assets(&plan)?;
            let logs = self.data_dir.join("logs");
            fs::create_dir_all(&logs).map_err(err)?;
            let log_file = logs.join(format!("game-{}.log", now().replace([':', '.'], "-")));
            self.inner.lock().unwrap().log_file = Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_file)
                    .map_err(err)?,
            );
            self.log(
                &format!(
                    "启动 {} · Java {major} · {} MB · {}用户 {}",
                    s(&plan.config, "versionId"),
                    plan.config["memoryMB"],
                    if identity.is_some() {
                        "正版"
                    } else {
                        "离线"
                    },
                    identity
                        .as_ref()
                        .map(|i| i.name.as_str())
                        .unwrap_or(s(&plan.config, "username"))
                ),
                "info",
            );
            let (args, classpath) = self.invocation(&plan, &java)?;
            let mut command = quiet_command(Path::new(s(&java, "path")));
            command
                .args(args)
                .current_dir(&plan.game_dir)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            if let Some(cp) = classpath {
                command.env("CLASSPATH", cp);
            }
            let mut child = command.spawn().map_err(err)?;
            let pid = child.id();
            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();
            let child = Arc::new(Mutex::new(child));
            self.inner.lock().unwrap().child = Some(child.clone());
            self.state_to(
                "running",
                "游戏进程运行中，首次加载可能需要几分钟",
                Some(pid),
            );
            let secret = identity.as_ref().map(|i| i.access_token.clone());
            let stderr_secret = secret.clone();
            let service = self.clone();
            thread::spawn(move || capture(stdout, service, "info", secret));
            let service = self.clone();
            thread::spawn(move || capture(stderr, service, "warn", stderr_secret));
            let service = self.clone();
            thread::spawn(move || loop {
                let result = child.lock().unwrap().try_wait();
                match result {
                    Ok(Some(status)) => {
                        let stopped = s(&service.state(), "phase") == "stopping";
                        let code = status.code().unwrap_or(-1);
                        service.log(
                            &if stopped {
                                "游戏进程已停止。".to_string()
                            } else {
                                format!("游戏进程结束，退出码：{code}")
                            },
                            if code == 0 || stopped {
                                "info"
                            } else {
                                "error"
                            },
                        );
                        let state = json!({"phase":if code==0||stopped{"idle"}else{"error"},"message":if stopped{"游戏已停止".to_string()}else if code==0{"游戏已关闭，欢迎回来".into()}else{format!("游戏异常退出（{code}），请查看运行日志")},"pid":null});
                        {
                            let mut inner = service.inner.lock().unwrap();
                            inner.child = None;
                            inner.log_file.take();
                            inner.state = state.clone();
                        }
                        service.emit("state", state);
                        break;
                    }
                    Err(error) => {
                        service.state_to("error", &error.to_string(), None);
                        service.inner.lock().unwrap().child = None;
                        break;
                    }
                    Ok(None) => thread::sleep(Duration::from_millis(75)),
                }
            });
            Ok(json!({"pid":pid,"logFile":path_text(&log_file)}))
        })();
        self.end();
        if let Err(error) = &result {
            self.inner.lock().unwrap().log_file.take();
            self.state_to("error", error, None);
            self.log(error, "error");
        }
        result
    }
    pub fn stop(&self) -> R<()> {
        let child = self.inner.lock().unwrap().child.clone();
        if let Some(child) = child {
            let pid = child.lock().unwrap().id();
            self.state_to("stopping", "正在停止游戏进程…", Some(pid));
            child.lock().unwrap().kill().map_err(err)?;
        }
        Ok(())
    }
    pub fn folder(&self, kind: &str) -> R<PathBuf> {
        let c = self.settings();
        match kind {
            "game" => {
                if s(&c, "gameRoot").is_empty() {
                    Err("请先添加游戏目录。".into())
                } else {
                    Ok(abs(s(&c, "gameRoot")))
                }
            }
            "mods" => Ok(game_directory(
                Path::new(s(&c, "gameRoot")),
                s(&c, "versionId"),
                &c["isolated"],
            )?
            .join("mods")),
            "logs" => {
                let root = self.data_dir.join("logs");
                fs::create_dir_all(&root).map_err(err)?;
                Ok(root)
            }
            "data" => Ok(self.data_dir.clone()),
            _ => Err("目录类型无效。".into()),
        }
    }
    fn download(&self, item: &FileItem) -> R<()> {
        let url = item
            .url
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or(format!(
                "{} 没有可信下载信息，请使用原启动器补全。",
                item.label
            ))?;
        let sha = item
            .sha1
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or(format!(
                "{} 没有可信下载信息，请使用原启动器补全。",
                item.label
            ))?;
        let parsed = reqwest::Url::parse(url).map_err(err)?;
        let hosts = [
            "libraries.minecraft.net",
            "resources.download.minecraft.net",
            "piston-data.mojang.com",
            "piston-meta.mojang.com",
            "launchermeta.mojang.com",
            "launcher.mojang.com",
            "meta.fabricmc.net",
            "maven.fabricmc.net",
            "maven.minecraftforge.net",
            "maven.neoforged.net",
            "maven.quiltmc.org",
        ];
        if parsed.scheme() != "https"
            || !hosts.contains(&parsed.host_str().unwrap_or(""))
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(format!(
                "{} 的下载地址暂不支持，请使用原启动器补全。",
                item.label
            ));
        }
        let temporary = item.file.with_file_name(format!(
            "{}.blockfield-{}-{}.tmp",
            item.file.file_name().unwrap_or_default().to_string_lossy(),
            Utc::now().timestamp_nanos_opt().unwrap_or(0),
            std::process::id()
        ));
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(err)?;
        let mut failure = String::new();
        for _ in 0..2 {
            let result = (|| {
                let mut response = client.get(url).send().map_err(err)?;
                if !response.status().is_success() {
                    return Err(format!("下载失败（{}）", response.status()));
                }
                fs::create_dir_all(item.file.parent().unwrap()).map_err(err)?;
                let mut file = File::create(&temporary).map_err(err)?;
                let mut hash = Sha1::new();
                let mut size = 0u64;
                let mut buffer = [0u8; 65536];
                loop {
                    let n = response.read(&mut buffer).map_err(err)?;
                    if n == 0 {
                        break;
                    }
                    size += n as u64;
                    if size > 150 * 1024 * 1024 {
                        return Err("下载文件超过大小限制。".into());
                    }
                    hash.update(&buffer[..n]);
                    file.write_all(&buffer[..n]).map_err(err)?;
                }
                drop(file);
                if hex::encode(hash.finalize()) != sha.to_lowercase() {
                    return Err("下载文件校验失败。".into());
                }
                fs::rename(&temporary, &item.file).map_err(err)?;
                Ok(())
            })();
            match result {
                Ok(()) => return Ok(()),
                Err(e) => {
                    failure = e;
                    let _ = fs::remove_file(&temporary);
                }
            }
        }
        Err(format!("{}：{failure}", item.label))
    }
    pub fn repair(self: &Arc<Self>) -> R<Value> {
        self.begin("repairing", "正在补全基础依赖…")?;
        let result = (|| {
            let plan = self.build_plan()?;
            let mut checked = self.inspect_files(&plan, true)?;
            let mut added_index = 0;
            let missing = list(&checked["missing"]);
            if let Some(index) = missing.iter().find(|item| s(item, "label") == "资源索引") {
                self.download(&json_file_item(index))?;
                added_index = 1;
                checked = self.inspect_files(&plan, true)?;
            }
            let items = list(&checked["missing"]);
            let total = items.len();
            let queue = Arc::new(Mutex::new(VecDeque::from(items)));
            let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let mut workers = Vec::new();
            for _ in 0..4.min(total) {
                let service = self.clone();
                let queue = queue.clone();
                let completed = completed.clone();
                workers.push(thread::spawn(move || loop {
                    let next = queue.lock().unwrap().pop_front();
                    if let Some(next) = next {
                        service.download(&json_file_item(&next))?;
                        let count =
                            completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                        service.state_to(
                            "repairing",
                            &format!("正在补全基础依赖 {count} / {total}"),
                            None,
                        );
                    } else {
                        return Ok::<(), String>(());
                    }
                }));
            }
            let mut error = None;
            for worker in workers {
                if let Err(e) = worker.join().unwrap_or(Err("下载线程异常结束。".into())) {
                    error = Some(e);
                }
            }
            if let Some(error) = error {
                return Err(error);
            }
            let repaired = completed.load(std::sync::atomic::Ordering::Relaxed) + added_index;
            self.log(&format!("基础依赖补全完成：{repaired} 个文件。"), "info");
            self.state_to("idle", "基础依赖已补全，可以启动游戏", None);
            Ok(json!({"repaired":repaired}))
        })();
        self.end();
        if let Err(error) = &result {
            self.state_to("error", error, None);
        }
        result
    }
}
fn json_file_item(value: &Value) -> FileItem {
    FileItem {
        file: PathBuf::from(s(value, "file")),
        sha1: optional(value, "sha1"),
        url: optional(value, "url"),
        label: s(value, "label").into(),
        reason: optional(value, "reason"),
    }
}
pub fn extract_natives(jar: &Path, destination: &Path) -> R<usize> {
    let file = File::open(jar).map_err(err)?;
    let mut archive = zip::ZipArchive::new(file).map_err(err)?;
    let mut count = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(err)?;
        let name = entry.name().to_string();
        if name.starts_with("META-INF/")
            || ![".dll", ".so", ".dylib", ".jnilib"]
                .iter()
                .any(|extension| name.to_lowercase().ends_with(extension))
        {
            continue;
        }
        if entry.size() > 64 * 1024 * 1024 {
            return Err("本机依赖压缩包格式不受支持。".into());
        }
        let basename = name.rsplit(['/', '\\']).next().unwrap();
        let target = inside(destination, basename)?;
        let temporary = target.with_extension("blockfield-tmp");
        let mut out = File::create(&temporary).map_err(err)?;
        std::io::copy(&mut entry, &mut out).map_err(err)?;
        drop(out);
        fs::rename(temporary, target).map_err(err)?;
        count += 1;
    }
    Ok(count)
}
pub(crate) fn capture<T: Read + Send + 'static>(
    stream: T,
    service: Arc<Service>,
    level: &str,
    secret: Option<String>,
) {
    let reader = BufReader::new(stream);
    for line in reader.split(b'\n') {
        match line {
            Ok(line) => {
                let text = String::from_utf8_lossy(&line);
                let text = text.trim_end_matches('\r');
                if !text.trim().is_empty() {
                    let clean = if let Some(secret) = secret.as_deref().filter(|s| !s.is_empty()) {
                        text.replace(secret, "[已隐藏]")
                    } else {
                        text.to_string()
                    };
                    service.log(&clean, level);
                }
            }
            Err(_) => break,
        }
    }
}
