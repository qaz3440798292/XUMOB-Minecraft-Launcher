use base64::Engine as _;
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
fn authlib_injector(data_dir: &Path) -> R<PathBuf> {
    let metadata = crate::net::json("https://authlib-injector.yushi.moe/artifact/latest.json")?;
    let url = s(&metadata, "download_url");
    let expected = s(&metadata["checksums"], "sha256").to_ascii_lowercase();
    if url.is_empty() || expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("无法读取 authlib-injector 的官方版本信息。".into());
    }
    let directory = data_dir.join("libraries").join("authlib-injector");
    fs::create_dir_all(&directory).map_err(err)?;
    let jar = directory.join("authlib-injector.jar");
    if jar.is_file() && crate::net::hash(&jar, true).is_ok_and(|hash| hash == expected) {
        return Ok(jar);
    }
    let bytes = crate::net::bytes(url, 32 * 1024 * 1024)?;
    if hex::encode(Sha256::digest(&bytes)) != expected {
        return Err("authlib-injector 下载校验失败，请重试。".into());
    }
    let temporary = jar.with_extension("jar.new");
    fs::write(&temporary, bytes).map_err(err)?;
    fs::rename(&temporary, &jar).map_err(err)?;
    Ok(jar)
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
pub fn system_memory() -> Value {
    let mut sys = System::new();
    sys.refresh_memory();
    let total = sys.total_memory() / 1024 / 1024;
    memory_capacity(total, sys.available_memory() / 1024 / 1024)
}
fn system() -> Value {
    system_memory()
}
// The slider's hard limit is stable. Free memory changes independently and is
// shown as a recommendation, rather than silently changing a saved allocation.
pub fn memory_capacity(total: u64, available: u64) -> Value {
    let reserve = 1024u64.max(total / 10);
    let maximum = (total.saturating_sub(reserve) / 256 * 256).max(1024);
    json!({"totalMB":total,"freeMB":available.min(total),"reserveMB":reserve,"maxMB":maximum,"recommendedMB":(available.saturating_sub(512)/256*256).min(maximum),"platform":"win32","arch":"x64"})
}

/// Store preferences outside versioned portable folders, copying only known
/// preference/credential files. Existing destination files are never replaced.
pub fn persistent_data_directory(executable: &Path, local_app_data: &Path) -> R<PathBuf> {
    let destination = local_app_data.join("XUMOB Minecraft Launcher").join("data");
    fs::create_dir_all(&destination).map_err(err)?;
    if destination.join("settings.json").exists() {
        return Ok(destination);
    }
    let executable_dir = executable.parent().ok_or("无法确定启动器所在目录。")?;
    let adjacent = executable_dir.join("data");
    let valid = |dir: &Path| read_json(&dir.join("settings.json")).is_ok_and(|v| v.is_object());
    let source = if adjacent != destination && valid(&adjacent) {
        Some(adjacent)
    } else {
        let mut candidates = fs::read_dir(executable_dir.parent().unwrap_or(executable_dir))
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_type().is_ok_and(|t| t.is_dir())
                    && e.file_name()
                        .to_string_lossy()
                        .starts_with("XUMOB Minecraft Launcher-Windows-v")
            })
            .map(|e| e.path().join("data"))
            .filter(|p| p != &destination && valid(p))
            .collect::<Vec<_>>();
        candidates.sort_by_key(|p| {
            std::cmp::Reverse(
                fs::metadata(p.join("settings.json"))
                    .and_then(|m| m.modified())
                    .ok(),
            )
        });
        candidates.into_iter().next()
    };
    if let Some(source) = source {
        // Copy settings last so an interrupted migration can resume next time.
        for filename in ["account-session.bin", "account.json", "settings.json"] {
            let from = source.join(filename);
            let to = destination.join(filename);
            if !from.is_file() || to.exists() {
                continue;
            }
            let temporary =
                destination.join(format!(".{filename}.migration-{}", std::process::id()));
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(err)?;
            let result = File::open(from)
                .and_then(|mut input| std::io::copy(&mut input, &mut output))
                .and_then(|_| output.sync_all());
            drop(output);
            if let Err(error) = result {
                let _ = fs::remove_file(temporary);
                return Err(err(error));
            }
            // Linking commits a complete file and fails if another process has
            // already created the destination; it cannot overwrite credentials.
            let result = fs::hard_link(&temporary, &to);
            let _ = fs::remove_file(temporary);
            if let Err(error) = result {
                if error.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(err(error));
                }
            }
        }
    }
    Ok(destination)
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
    let argument = |flag: &str| {
        args.iter()
            .position(|v| v.as_str() == Some(flag))
            .and_then(|i| args.get(i + 1))
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty() && !v.contains("${"))
            .map(str::to_owned)
    };
    let neo_group = argument("--fml.forgeGroup").as_deref() == Some("net.neoforged");
    let quilt = coordinate(v, "org.quiltmc:quilt-loader:");
    let fabric = coordinate(v, "net.fabricmc:fabric-loader:");
    let neo = coordinate(v, "net.neoforged:neoforge:")
        .or_else(|| coordinate(v, "net.neoforged:forge:"))
        .or_else(|| argument("--fml.neoForgeVersion"))
        .or_else(|| {
            if neo_group {
                argument("--fml.forgeVersion")
            } else {
                None
            }
        });
    let forge = coordinate(v, "net.minecraftforge:forge:")
        .or_else(|| coordinate(v, "net.minecraftforge:minecraftforge:"))
        .or_else(|| {
            if !neo_group {
                argument("--fml.forgeVersion")
            } else {
                None
            }
        });
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
    // Loader JSONs sometimes copy a launcher's own runtime requirement into
    // `javaVersion`. For established Minecraft releases, the Minecraft
    // version is the reliable compatibility baseline (e.g. 1.19.x is Java 17).
    // Keep the manifest field for newer releases whose requirement may have
    // changed beyond the known release mapping.
    let manifest_java = v["javaVersion"]["majorVersion"].as_u64();
    let release_java = release.and_then(|(m, p)| match m {
        21 => Some(21),
        20 if p >= 5 => Some(21),
        20 => Some(17),
        18..=19 => Some(17),
        17 => Some(16),
        0..=16 => Some(8),
        _ => manifest_java.or(Some(21)),
    });
    let calendar_java = Regex::new(r"^(\d{2})\.(\d+)")
        .unwrap()
        .captures(&minecraft)
        .and_then(|captures| captures[1].parse::<u32>().ok())
        .filter(|year| *year >= 26)
        .map(|_| 25);
    let required = match (release_java, calendar_java, manifest_java) {
        (Some(known), _, _) => known,
        (None, Some(known), Some(manifest)) => known.max(manifest),
        (None, Some(known), None) => known,
        (None, None, Some(manifest)) => manifest,
        (None, None, None) => 8,
    };
    let legacy = forge.is_some() && release.is_some_and(|(m, _)| m <= 16) && required == 8;
    let maximum = if legacy { Some(8) } else { None };
    // Recommend the minimum known-good runtime for the game's actual version.
    // In particular, 1.19 should recommend Java 17, not the newest installed JDK.
    let recommended = required;
    json!({"minecraft":minecraft,"loader":if labels.is_empty(){"原版".to_string()}else{labels.join(" + ")},"javaRequired":required,"javaRecommended":recommended,"javaMaximum":maximum,"modded":!labels.is_empty()})
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

fn has_argument(items: &Value, expected: &str) -> bool {
    match items {
        Value::String(value) => value == expected,
        Value::Array(values) => values.iter().any(|value| has_argument(value, expected)),
        Value::Object(values) => values.values().any(|value| has_argument(value, expected)),
        _ => false,
    }
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
    c.env_remove("XUMOB_CURSEFORGE_API_KEY")
        .env_remove("XUMOB_BUILD_CURSEFORGE_API_KEY");
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
    #[cfg(windows)]
    if !path.with_file_name("javaw.exe").is_file() {
        return Err("这个 Java 缺少 javaw.exe，无法作为游戏运行环境。请安装完整的 64 位 JDK。".into());
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
    let is_jdk = path
        .parent()
        .is_some_and(|bin| bin.join("javac.exe").is_file());
    if major >= 16 && !is_jdk {
        return Err(format!(
            "检测到 Java {major} 运行时缺少 javac.exe。为减少 Forge 安装和模组兼容问题，请使用完整 JDK。"
        ));
    }
    let is_alias = path_text(&path).to_ascii_lowercase().contains("\\javapath\\");
    Ok(json!({"path":path_text(&path),"version":version,"major":major,"bits":bits,"isJdk":is_jdk,"isAlias":is_alias,"encoding":encoding,"label":format!("Java {major} · {} · {bits} 位 · {version}", if is_jdk {"JDK"} else {"JRE"})}))
}

fn find_runtime_java(root: &Path, depth: usize) -> Option<PathBuf> {
    let java = root.join("bin").join("java.exe");
    if java.is_file() {
        return Some(java);
    }
    if depth == 0 {
        return None;
    }
    fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .find_map(|entry| find_runtime_java(&entry.path(), depth - 1))
}

pub struct Service {
    pub data_dir: PathBuf,
    pub auth: Arc<crate::auth::AuthManager>,
    pub modrinth: Arc<crate::modrinth::Manager>,
    pub curseforge: Arc<crate::curseforge::CurseForge>,
    pub installer: Arc<crate::install::Manager>,
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
        let curseforge = crate::curseforge::CurseForge::new(&data_dir)?;
        let installer = crate::install::Manager::new();
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
                // A PC with less RAM must not reset the remembered game path.
                let max = system()["maxMB"].as_u64().unwrap_or(8192);
                old["memoryMB"] = json!(old["memoryMB"].as_u64().unwrap_or(4096).clamp(1024, max));
                for (key, minimum, maximum) in [("width", 640, 7680), ("height", 360, 4320)] {
                    if old.get(key).is_some()
                        && !old[key]
                            .as_u64()
                            .is_some_and(|n| (minimum..=maximum).contains(&n))
                    {
                        old[key] = defaults[key].clone();
                    }
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
            curseforge,
            installer,
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
        let global = self.global_settings();
        let version_id = s(&global, "versionId").to_owned();
        if version_id.is_empty() || s(&global, "gameRoot").is_empty() {
            return global;
        }
        let key = profile_key(s(&global, "gameRoot"), &version_id);
        if global["profiles"][&key]["custom"].as_bool() == Some(true) {
            let mut overrides = global["profiles"][&key].clone();
            if let Some(values) = overrides.as_object_mut() {
                values.remove("custom");
            }
            merge(&global, &overrides)
        } else {
            global
        }
    }
    pub fn global_settings(&self) -> Value {
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
        let message = format!(
            "正在下载 {} · {}%",
            s(&progress, "filename"),
            bytes.saturating_mul(100) / total
        );
        self.progress_to("downloading", &message, progress);
    }
    pub(crate) fn progress_to(&self, phase: &str, message: &str, progress: Value) {
        let state = json!({"phase":phase,"message":message,"pid":null,"progress":progress});
        self.inner.lock().unwrap().state = state.clone();
        self.emit("state", state);
    }
    pub fn log(&self, line: &str, level: &str) {
        let sanitized = self.auth.redact(line);
        let scrub = Regex::new(
            r#"(?i)(--(?:accessToken|clientId)\s+|(?:access_token|refresh_token|client_id|clientId)["'\s:=]+)[^\s,"']+"#,
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
        if phase == "downloading" {
            self.modrinth.reset_cancellation();
        }
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
        let switching = patch
            .get("gameRoot")
            .is_some_and(|v| v != &previous["gameRoot"])
            || patch
                .get("versionId")
                .is_some_and(|v| v != &previous["versionId"]);
        let mut next = merge(&previous, &patch);
        if switching {
            let remembered = list(&previous["directories"])
                .into_iter()
                .find(|d| s(d, "path").eq_ignore_ascii_case(s(&next, "gameRoot")))
                .map(|d| s(&d, "lastVersionId").to_string())
                .unwrap_or_default();
            if patch.get("gameRoot").is_some() && patch.get("versionId").is_none() {
                next["versionId"] = json!(remembered.clone());
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
        let checked = validate(&next)?;
        let temporary = self.data_dir.join("settings.json.tmp");
        fs::write(
            &temporary,
            serde_json::to_vec_pretty(&checked).map_err(err)?,
        )
        .map_err(err)?;
        fs::rename(&temporary, self.data_dir.join("settings.json")).map_err(err)?;
        inner.settings = checked;
        drop(inner);
        Ok(self.settings())
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
    pub fn version_settings(&self, id: &str) -> R<Value> {
        valid_id(id)?;
        let global = self.global_settings();
        let root = Path::new(s(&global, "gameRoot"));
        if root.as_os_str().is_empty() {
            return Err("请先选择游戏目录。".into());
        }
        let version_dir = inside(root, &format!("versions/{id}"))?;
        let metadata =
            fs::symlink_metadata(&version_dir).map_err(|_| format!("找不到客户端“{id}”。"))?;
        if !metadata.file_type().is_dir() {
            return Err(format!("客户端“{id}”不是普通文件夹。"));
        }
        resolve_version(root, id)?;
        let key = profile_key(s(&global, "gameRoot"), id);
        let stored = global["profiles"][&key].clone();
        let customized = stored["custom"].as_bool() == Some(true);
        let mut overrides = if customized { stored } else { json!({}) };
        if let Some(values) = overrides.as_object_mut() {
            values.remove("custom");
        }
        let mut effective = merge(&global, &overrides);
        effective["versionId"] = json!(id);
        Ok(
            json!({"id":id,"customized":customized,"overrides":overrides,"effective":effective,"globalSettings":global}),
        )
    }
    fn servers_file(&self, id: &str) -> R<(PathBuf, bool)> {
        let settings = self.version_settings(id)?;
        let global = self.global_settings();
        let root = Path::new(s(&global, "gameRoot"));
        let version = resolve_version(root, id)?;
        let minecraft = s(&describe_version(&version), "minecraft").to_owned();
        let major = minecraft
            .split('.')
            .next()
            .and_then(|part| part.parse::<u32>().ok());
        // Minecraft's calendar-version releases use uncompressed NBT for servers.dat;
        // legacy releases continue to use gzip-compressed NBT.
        let compressed = major.is_none_or(|major| major < 26);
        Ok((
            game_directory(root, id, &settings["effective"]["isolated"])?.join("servers.dat"),
            compressed,
        ))
    }
    fn ensure_servers_editable(&self) -> R<()> {
        let inner = self.inner.lock().unwrap();
        if inner.busy || inner.child.is_some() {
            return Err("游戏运行或准备期间不能修改服务器列表。".into());
        }
        Ok(())
    }
    pub fn servers(&self, id: &str) -> R<Value> {
        crate::servers::list(&self.servers_file(id)?.0)
    }
    pub fn add_server(&self, id: &str, name: &str, address: &str) -> R<Value> {
        self.ensure_servers_editable()?;
        let (path, compressed) = self.servers_file(id)?;
        crate::servers::add(&path, name, address, compressed)
    }
    pub fn remove_server(&self, id: &str, address: &str) -> R<Value> {
        self.ensure_servers_editable()?;
        let (path, compressed) = self.servers_file(id)?;
        crate::servers::remove(&path, address, compressed)
    }
    pub fn move_server(&self, id: &str, address: &str, offset: i32) -> R<Value> {
        self.ensure_servers_editable()?;
        let (path, compressed) = self.servers_file(id)?;
        crate::servers::move_server(&path, address, offset, compressed)
    }
    pub fn ping_servers(&self, id: &str) -> R<Value> {
        let (path, compressed) = self.servers_file(id)?;
        crate::servers::ping_all(&path, compressed)
    }
    pub fn save_version_settings(&self, id: &str, custom: bool, patch: Value) -> R<Value> {
        valid_id(id)?;
        let mut inner = self.inner.lock().unwrap();
        if inner.busy || inner.child.is_some() {
            return Err("游戏正在运行或准备中，请稍后更改配置。".into());
        }
        let previous = inner.settings.clone();
        let root = Path::new(s(&previous, "gameRoot"));
        if root.as_os_str().is_empty() {
            return Err("请先选择游戏目录。".into());
        }
        let version_dir = inside(root, &format!("versions/{id}"))?;
        let metadata =
            fs::symlink_metadata(&version_dir).map_err(|_| format!("找不到客户端“{id}”。"))?;
        if !metadata.file_type().is_dir() {
            return Err(format!("客户端“{id}”不是普通文件夹。"));
        }
        resolve_version(root, id)?;

        let key = profile_key(s(&previous, "gameRoot"), id);
        let mut next = previous.clone();
        if custom {
            let old_overrides = if previous["profiles"][&key]["custom"].as_bool() == Some(true) {
                previous["profiles"][&key].clone()
            } else {
                json!({})
            };
            let mut effective = merge(&merge(&previous, &old_overrides), &patch);
            effective["versionId"] = json!(id);
            let mut overrides = profile(&validate(&effective)?);
            overrides["custom"] = json!(true);
            if s(&overrides, "javaMode") == "manual" && !s(&overrides, "javaPath").is_empty() {
                let mut paths = list(&next["knownJavaPaths"]);
                if !paths.iter().any(|value| {
                    value
                        .as_str()
                        .is_some_and(|path| path.eq_ignore_ascii_case(s(&overrides, "javaPath")))
                }) {
                    paths.push(json!(s(&overrides, "javaPath")));
                }
                next["knownJavaPaths"] = json!(paths);
            }
            next["profiles"][&key] = overrides;
        } else if let Some(profiles) = next["profiles"].as_object_mut() {
            profiles.remove(&key);
        }
        let checked = validate(&next)?;
        let temporary = self.data_dir.join("settings.json.tmp");
        fs::write(
            &temporary,
            serde_json::to_vec_pretty(&checked).map_err(err)?,
        )
        .map_err(err)?;
        fs::rename(&temporary, self.data_dir.join("settings.json")).map_err(err)?;
        inner.settings = checked;
        drop(inner);
        self.version_settings(id)
    }
    pub fn versions(&self) -> Vec<Value> {
        let config = self.global_settings();
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
        names
            .into_iter()
            .take(150)
            .map(|id| {
                let result = (|| {
                    let version = resolve_version(root, &id)?;
                    let info = describe_version(&version);
                    let key = profile_key(s(&config, "gameRoot"), &id);
                    let has_custom_settings =
                        config["profiles"][&key]["custom"].as_bool() == Some(true);
                    let isolation = if has_custom_settings {
                        config["profiles"][key]
                            .get("isolated")
                            .cloned()
                            .unwrap_or_else(|| config["isolated"].clone())
                    } else {
                        config["isolated"].clone()
                    };
                    let game = game_directory(root, &id, &isolation)?;
                    let mods = fs::read_dir(game.join("mods"))
                        .ok()
                        .into_iter()
                        .flatten()
                        .filter_map(Result::ok)
                        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".jar"))
                        .count();
                    let mut card = json!({
                        "id": id,
                        "modCount": mods,
                        "gameDir": path_text(&game),
                        "isolated": game != root,
                        "supported": !s(&version, "mainClass").is_empty(),
                        "hasCustomSettings": has_custom_settings,
                        "mainClass": version["mainClass"]
                    });
                    let sidecar = root.join("versions").join(&id).join(".xumob-pack.json");
                    if let Ok(bytes) = fs::read(sidecar) {
                        if let Ok(pack) = serde_json::from_slice::<Value>(&bytes) {
                            let icon = if s(&pack, "provider") == "curseforge" {
                                crate::curseforge::safe_icon_url(s(&pack, "iconData"))
                            } else {
                                crate::modrinth::icon_url(s(&pack, "iconData"))
                            };
                            if !icon.is_empty() {
                                card["iconData"] = json!(icon);
                            }
                        }
                    }
                    if s(&card, "iconData").is_empty() {
                        if let Some(icon) = local_version_icon(root, &id) {
                            card["iconData"] = json!(icon);
                        }
                    }
                    Ok::<Value, String>(merge(&info, &card))
                })();
                result.unwrap_or_else(|error| json!({"id":id,"error":error,"supported":false}))
            })
            .collect()
    }
    pub fn delete_versions(&self, ids: &[String]) -> R<Value> {
        if ids.is_empty() || ids.len() > 150 {
            return Err("请选择要删除的客户端。".into());
        }
        {
            let inner = self.inner.lock().unwrap();
            if inner.busy || inner.child.is_some() {
                return Err("游戏正在运行或准备中，暂时不能删除客户端。".into());
            }
        }
        let config = self.settings();
        let root = Path::new(s(&config, "gameRoot"));
        if root.as_os_str().is_empty() {
            return Err("请先选择游戏目录。".into());
        }
        let versions_dir = root.join("versions");
        let versions_meta = fs::symlink_metadata(&versions_dir).map_err(err)?;
        if !versions_meta.file_type().is_dir() {
            return Err("游戏版本目录无效。".into());
        }
        let mut selected = HashSet::new();
        for id in ids {
            valid_id(id)?;
            if !selected.insert(id.as_str()) {
                continue;
            }
            let target = inside(&versions_dir, id)?;
            let metadata =
                fs::symlink_metadata(&target).map_err(|_| format!("找不到客户端“{id}”。"))?;
            if !metadata.file_type().is_dir() {
                return Err(format!("客户端“{id}”不是普通文件夹，已取消删除。"));
            }
        }
        let mut remaining = fs::read_dir(&versions_dir)
            .map_err(err)?
            .filter_map(Result::ok)
            .filter(|entry| {
                let id = entry.file_name().to_string_lossy().into_owned();
                !selected.contains(id.as_str()) && entry.file_type().is_ok_and(|kind| kind.is_dir())
            })
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        remaining.sort();
        let next_version = remaining
            .into_iter()
            .find(|id| resolve_version(root, id).is_ok())
            .unwrap_or_default();
        for id in &selected {
            fs::remove_dir_all(inside(&versions_dir, id)?).map_err(err)?;
        }
        if selected.contains(s(&config, "versionId")) {
            self.save(json!({"versionId":next_version}))?;
        }
        let mut inner = self.inner.lock().unwrap();
        let mut checked = inner.settings.clone();
        let root = s(&checked, "gameRoot").to_owned();
        for id in &selected {
            checked["profiles"]
                .as_object_mut()
                .ok_or("实例配置无效。")?
                .remove(&profile_key(&root, id));
        }
        checked = validate(&checked)?;
        let temporary = self.data_dir.join("settings.json.tmp");
        fs::write(
            &temporary,
            serde_json::to_vec_pretty(&checked).map_err(err)?,
        )
        .map_err(err)?;
        fs::rename(&temporary, self.data_dir.join("settings.json")).map_err(err)?;
        inner.settings = checked;
        drop(inner);
        self.snapshot()
    }
    pub fn snapshot(&self) -> R<Value> {
        let config = self.settings();
        let global_config = self.global_settings();
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
            json!({"settings":config,"globalSettings":global_config,"account":self.auth.summary(),"versions":versions,"suggestions":suggestions,"state":inner.state,"logs":logs,"system":system(),"dataDir":path_text(&self.data_dir)}),
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
    pub fn mod_action(&self, names: &[String], action: &str) -> R<Value> {
        if names.is_empty() || names.len() > 500 || !matches!(action, "delete" | "toggle") {
            return Err("模组操作无效。".into());
        }
        {
            let inner = self.inner.lock().unwrap();
            if inner.busy || inner.child.is_some() {
                return Err("游戏正在运行或准备中，暂时不能管理模组。".into());
            }
        }
        let config = self.settings();
        if s(&config, "versionId").is_empty() {
            return Err("请先选择一个游戏客户端。".into());
        }
        let game = game_directory(
            Path::new(s(&config, "gameRoot")),
            s(&config, "versionId"),
            &config["isolated"],
        )?;
        let mods_dir = game.join("mods");
        let mut unique = HashSet::new();
        let mut files = Vec::new();
        for name in names {
            if name.is_empty()
                || name.contains(['/', '\\', ':', '\0'])
                || !name.to_ascii_lowercase().ends_with(".jar")
                    && !name.to_ascii_lowercase().ends_with(".jar.disabled")
                || !unique.insert(name.as_str())
            {
                return Err("模组文件名无效。".into());
            }
            let path = inside(&mods_dir, name)?;
            let metadata =
                fs::symlink_metadata(&path).map_err(|_| format!("找不到模组文件“{name}”。"))?;
            if !metadata.file_type().is_file() {
                return Err(format!("模组“{name}”不是普通文件，已取消操作。"));
            }
            files.push((name.clone(), path));
        }
        if action == "toggle" && files.len() != 1 {
            return Err("一次只能启用或停用一个模组。".into());
        }
        for (name, path) in files {
            if action == "delete" {
                fs::remove_file(path).map_err(err)?;
            } else if name.to_ascii_lowercase().ends_with(".jar.disabled") {
                let enabled_name = name[..name.len() - ".disabled".len()].to_string();
                let enabled_path = inside(&mods_dir, &enabled_name)?;
                if enabled_path.exists() {
                    return Err("启用失败：同名模组文件已经存在。".into());
                }
                fs::rename(path, enabled_path).map_err(err)?;
            } else {
                let disabled_path = inside(&mods_dir, &format!("{name}.disabled"))?;
                if disabled_path.exists() {
                    return Err("停用失败：同名停用文件已经存在。".into());
                }
                fs::rename(path, disabled_path).map_err(err)?;
            }
        }
        self.mods()
    }
}
fn local_version_icon(root: &Path, id: &str) -> Option<String> {
    let directory = root.join("versions").join(id);
    for (filename, mime, signature) in [
        ("icon.png", "image/png", &[0x89, b'P', b'N', b'G'][..]),
        ("PCL/Logo.png", "image/png", &[0x89, b'P', b'N', b'G'][..]),
        ("icon.jpg", "image/jpeg", &[0xff, 0xd8, 0xff][..]),
        ("icon.jpeg", "image/jpeg", &[0xff, 0xd8, 0xff][..]),
        ("icon.webp", "image/webp", &b"RIFF"[..]),
    ] {
        let path = directory.join(filename);
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.file_type().is_file()
            || metadata.len() == 0
            || metadata.len() > 2 * 1024 * 1024
        {
            continue;
        }
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        if !bytes.starts_with(signature)
            || (mime == "image/webp" && !bytes.get(8..12).is_some_and(|v| v == b"WEBP"))
        {
            continue;
        }
        return Some(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ));
    }
    None
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
        // PATH may expose Oracle's `javapath` shim and the actual JDK directory
        // as separate executables. Keep one entry per runtime version and
        // prefer a real installation path over a shim.
        found.sort_by_key(|java| {
            (
                java["isAlias"].as_bool().unwrap_or(false),
                !java["isJdk"].as_bool().unwrap_or(false),
                std::cmp::Reverse(java["major"].as_u64().unwrap_or(0)),
                s(java, "path").to_ascii_lowercase(),
            )
        });
        let mut seen_versions = HashSet::new();
        found.retain(|java| {
            let key = (
                java["major"].as_u64().unwrap_or(0),
                s(java, "version").to_ascii_lowercase(),
            );
            seen_versions.insert(key)
        });
        self.inner.lock().unwrap().java_cache = Some((Instant::now(), found.clone()));
        Ok(found)
    }
    pub fn download_corretto(&self, major: u64, release: Option<&str>) -> R<String> {
        let legacy_java8 = release == Some("8.312.07.1");
        if release.is_some() && !legacy_java8 {
            return Err("不支持请求的 Amazon Corretto 历史版本。".into());
        }
        if (legacy_java8 && major != 8)
            || (!legacy_java8 && !matches!(major, 8 | 11 | 17 | 21 | 25 | 26))
        {
            return Err(format!(
                "Amazon Corretto 暂不提供 Java {major} 的 Windows x64 安装包。"
            ));
        }
        let runtime_label = if legacy_java8 {
            "8.312.07.1".to_string()
        } else {
            major.to_string()
        };
        self.begin(
            "downloading_java",
            &format!("正在准备 Amazon Corretto Java {runtime_label}…"),
        )?;
        let result = (|| {
            let runtime_root = self.data_dir.join("runtimes");
            fs::create_dir_all(&runtime_root).map_err(err)?;
            let runtime = runtime_root.join(format!("amazon-corretto-{runtime_label}"));
            if runtime.is_dir() {
                if let Some(java) = find_runtime_java(&runtime, 4) {
                    if probe_java(&path_text(&java)).ok().is_some_and(|details| {
                        details["major"].as_u64() == Some(major)
                            && (!legacy_java8
                                || details["version"]
                                    .as_str()
                                    .is_some_and(|v| v.contains("_312")))
                    }) {
                        return Ok(path_text(&java));
                    }
                }
            }

            let archive_name = if legacy_java8 {
                "amazon-corretto-8.312.07.1-windows-x64-jdk.zip".to_string()
            } else {
                format!("amazon-corretto-{major}-x64-windows-jdk.zip")
            };
            let archive_path = runtime_root.join(format!("{archive_name}.part"));
            let stage = runtime_root.join(format!("amazon-corretto-{runtime_label}.staging"));
            let _ = fs::remove_file(&archive_path);
            let _ = fs::remove_dir_all(&stage);
            let result = (|| {
                let client = reqwest::blocking::Client::builder()
                    .user_agent("XUMOB-Minecraft-Launcher/0.7.0")
                    .connect_timeout(Duration::from_secs(15))
                    .timeout(Duration::from_secs(900))
                    .build()
                    .map_err(|_| "无法创建 Amazon Corretto 下载连接。".to_string())?;
                let (base, expected, use_md5) = if legacy_java8 {
                    (
                        format!(
                            "https://corretto.aws/downloads/resources/8.312.07.1/{archive_name}"
                        ),
                        "44e5efdb55886fef4e7f552a3879618b".to_string(),
                        true,
                    )
                } else {
                    let checksum_url =
                        format!("https://corretto.aws/downloads/latest_sha256/{archive_name}");
                    let checksum_response = client.get(checksum_url).send().map_err(|_| {
                        "无法获取 Amazon Corretto 校验信息，请检查网络连接。".to_string()
                    })?;
                    if !checksum_response.status().is_success() {
                        return Err("无法获取 Amazon Corretto 校验信息。".into());
                    }
                    let checksum_text = checksum_response.text().map_err(err)?;
                    let expected = checksum_text
                        .split_whitespace()
                        .find(|part| {
                            part.len() == 64 && part.bytes().all(|b| b.is_ascii_hexdigit())
                        })
                        .ok_or("Amazon Corretto 返回的校验信息无效。")?
                        .to_ascii_lowercase();
                    (
                        format!("https://corretto.aws/downloads/latest/{archive_name}"),
                        expected,
                        false,
                    )
                };

                self.state_to(
                    "downloading_java",
                    &format!("正在从 Amazon Corretto 下载 Java {major}…"),
                    None,
                );
                let mut response = client
                    .get(base)
                    .send()
                    .map_err(|_| "Amazon Corretto 下载失败，请检查网络连接后重试。".to_string())?;
                if !response.status().is_success() {
                    return Err(format!(
                        "Amazon Corretto 下载失败（HTTP {}）。",
                        response.status()
                    ));
                }
                let content_length = response.content_length();
                if content_length.is_some_and(|length| length > 1024 * 1024 * 1024) {
                    return Err("Java 安装包大小异常，已取消下载。".into());
                }
                let mut archive = File::create(&archive_path).map_err(err)?;
                let mut digest = Sha256::new();
                let mut legacy_digest = Md5::new();
                let mut buffer = [0u8; 128 * 1024];
                let mut downloaded = 0u64;
                let mut last_report = Instant::now();
                self.progress_to(
                    "downloading_java",
                    &format!("正在下载 Amazon Corretto Java {major}…"),
                    json!({"filename":archive_name,"bytes":0,"totalBytes":content_length.unwrap_or(0),"fileIndex":1,"fileCount":1,"unit":"bytes"}),
                );
                loop {
                    let count = response
                        .read(&mut buffer)
                        .map_err(|_| "Amazon Corretto 下载中断，请检查网络后重试。".to_string())?;
                    if count == 0 {
                        break;
                    }
                    downloaded = downloaded.saturating_add(count as u64);
                    if downloaded > 1024 * 1024 * 1024 {
                        return Err("Java 安装包超过允许大小，已取消下载。".into());
                    }
                    archive.write_all(&buffer[..count]).map_err(err)?;
                    digest.update(&buffer[..count]);
                    legacy_digest.update(&buffer[..count]);
                    if last_report.elapsed() >= Duration::from_millis(800) {
                        let message = content_length
                            .filter(|total| *total > 0)
                            .map(|total| {
                                format!(
                                    "正在下载 Amazon Corretto Java {major}… {}%",
                                    downloaded.saturating_mul(100) / total
                                )
                            })
                            .unwrap_or_else(|| format!("正在下载 Amazon Corretto Java {major}…"));
                        self.progress_to(
                            "downloading_java",
                            &message,
                            json!({"filename":archive_name,"bytes":downloaded,"totalBytes":content_length.unwrap_or(0),"fileIndex":1,"fileCount":1,"unit":"bytes"}),
                        );
                        last_report = Instant::now();
                    }
                }
                archive.sync_all().map_err(err)?;
                let actual = if use_md5 {
                    hex::encode(legacy_digest.finalize())
                } else {
                    hex::encode(digest.finalize())
                };
                if actual != expected {
                    return Err("Java 安装包校验失败，已删除不完整文件。".into());
                }
                self.state_to(
                    "downloading_java",
                    &format!("正在校验并安装 Amazon Corretto Java {major}…"),
                    None,
                );
                fs::create_dir_all(&stage).map_err(err)?;
                let mut zip = zip::ZipArchive::new(File::open(&archive_path).map_err(err)?)
                    .map_err(|_| "Amazon Corretto 安装包不是有效 ZIP 文件。".to_string())?;
                if zip.len() > 50_000 {
                    return Err("Java 安装包内容异常，已取消安装。".into());
                }
                let mut extracted = 0u64;
                for index in 0..zip.len() {
                    let mut entry = zip.by_index(index).map_err(err)?;
                    let Some(relative) = entry.enclosed_name() else {
                        return Err("Java 安装包包含无效路径，已取消安装。".into());
                    };
                    if entry
                        .unix_mode()
                        .is_some_and(|mode| mode & 0o170000 == 0o120000)
                    {
                        continue;
                    }
                    let destination = inside(&stage, &path_text(&relative))?;
                    if entry.is_dir() {
                        fs::create_dir_all(destination).map_err(err)?;
                        continue;
                    }
                    extracted = extracted.saturating_add(entry.size());
                    if extracted > 2 * 1024 * 1024 * 1024 {
                        return Err("Java 安装包解压后体积异常，已取消安装。".into());
                    }
                    if let Some(parent) = destination.parent() {
                        fs::create_dir_all(parent).map_err(err)?;
                    }
                    let mut output = File::create(destination).map_err(err)?;
                    std::io::copy(&mut entry, &mut output).map_err(err)?;
                }
                let staged_java =
                    find_runtime_java(&stage, 4).ok_or("Java 安装包中没有找到可用的运行环境。")?;
                let details = probe_java(&path_text(&staged_java))?;
                if details["major"].as_u64() != Some(major)
                    || details["bits"].as_u64() != Some(64)
                    || (legacy_java8
                        && !details["version"]
                            .as_str()
                            .is_some_and(|v| v.contains("_312")))
                {
                    return Err("下载的 Java 版本与所需版本不匹配。".into());
                }
                if runtime.exists() {
                    fs::remove_dir_all(&runtime).map_err(err)?;
                }
                fs::rename(&stage, &runtime).map_err(err)?;
                let java =
                    find_runtime_java(&runtime, 4).ok_or("Java 安装完成后未找到运行文件。")?;
                Ok(path_text(&java))
            })();
            let _ = fs::remove_file(&archive_path);
            if result.is_err() {
                let _ = fs::remove_dir_all(&stage);
            }
            result
        })();
        self.end();
        match &result {
            Ok(_) => self.state_to(
                "idle",
                &format!("Amazon Corretto Java {runtime_label} 已安装"),
                None,
            ),
            Err(error) => self.state_to("idle", error, None),
        }
        result
    }
    pub fn select_java(&self, version: &Value) -> R<Value> {
        let config = self.settings();
        if s(&config, "javaMode") == "manual" {
            return probe_java(s(&config, "javaPath"));
        }
        let info = describe_version(version);
        let required = info["javaRequired"].as_u64().unwrap_or(8);
        let recommended = info["javaRecommended"].as_u64().unwrap_or(required);
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
        suitable.sort_by_key(|v| {
            let major = v["major"].as_u64().unwrap_or(0);
            let jdk_rank = if v["isJdk"] == true { 0 } else { 1 };
            let recommendation_rank = if major == recommended { 0 } else { 1 };
            let major_rank = match major {
                21 => 0,
                17 => 1,
                8 => 2,
                25 => 3,
                _ => 4 + major as usize,
            };
            (recommendation_rank, jdk_rank, major_rank)
        });
        suitable
            .iter()
            .find(|v| v["major"] == info["javaRecommended"])
            .cloned()
            .or_else(|| suitable.first().cloned())
            .ok_or(format!(
                "没有找到适用的 64 位 JDK（最低 Java {required}，建议 Java {recommended}），请在版本设置中选择或安装。"
            ))
    }
    pub fn build_plan(&self) -> R<Plan> {
        let identity = self.auth.preview()?;
        self.build_plan_with_launch_options(identity.as_ref(), None)
    }
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn build_plan_with_identity(
        &self,
        identity: Option<&crate::auth::Identity>,
    ) -> R<Plan> {
        self.build_plan_with_launch_options(identity, None)
    }
    fn build_plan_with_launch_options(
        &self,
        identity: Option<&crate::auth::Identity>,
        server_address: Option<&str>,
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
        let mut variables = json!({"auth_player_name":s(&config,"username"),"auth_uuid":offline_uuid(s(&config,"username")),"auth_access_token":"0","user_type":"legacy","user_properties":"{}","auth_session":"0","version_name":id,"game_directory":path_text(&game),"assets_root":path_text(&inside(root,"assets")?),"assets_index_name":assets,"version_type":optional(&version,"type").unwrap_or("release".into()),"launcher_name":"XUMOB Minecraft Launcher","launcher_version":"0.7.0","natives_directory":path_text(&native_dir),"library_directory":path_text(&inside(root,"libraries")?),"classpath_separator":";","classpath":cp,"clientid":"","auth_xuid":"","resolution_width":config["width"],"resolution_height":config["height"],"game_assets":path_text(&if asset_data["map_to_resources"]==true{inside(&game,"resources")?}else{inside(root,&format!("assets/virtual/{assets}"))?}),"primary_jar":path_text(&client)});
        if let Some(identity) = identity {
            variables = merge(&variables, &identity.variables());
        }
        let quick_play_identifier = if let Some(address) = server_address {
            let (host, port) = crate::servers::parse_address(address)?;
            let host = if host.contains(':') {
                format!("[{host}]")
            } else {
                host
            };
            Some(format!("{host}:{port}"))
        } else {
            None
        };
        if let Some(identifier) = &quick_play_identifier {
            variables["quickPlayMultiplayer"] = json!(identifier);
            variables["quickPlayPath"] = json!(path_text(&inside(&game, "quickPlay/log.json")?));
        }
        let features = json!({
            "has_custom_resolution":true,
            "has_quick_plays_support":quick_play_identifier.is_some(),
            "is_quick_play_multiplayer":quick_play_identifier.is_some()
        });
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
        if let Some(identity) = identity.filter(|identity| identity.api_root.is_some()) {
            let jar = authlib_injector(&self.data_dir)?;
            let api_root = identity.api_root.as_deref().unwrap_or_default();
            args.push(format!("-javaagent:{}={api_root}", path_text(&jar)));
            if let Some(metadata) = &identity.prefetched_metadata {
                args.push(format!("-Dauthlibinjector.yggdrasil.prefetched={metadata}"));
            }
        }
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
        self.launch_to_server(None)
    }
    pub fn launch_to_server(self: &Arc<Self>, server_address: Option<String>) -> R<Value> {
        self.begin("preparing", "正在准备游戏环境…")?;
        let result = (|| {
            let mut plan = self.build_plan()?;
            let java = self.select_java(&plan.version)?;
            let info = describe_version(&plan.version);
            let minecraft_major = s(&info, "minecraft")
                .split('.')
                .next()
                .and_then(|part| part.parse::<u32>().ok());
            crate::servers::normalize_format(
                &plan.game_dir.join("servers.dat"),
                minecraft_major.is_none_or(|major| major < 26),
            )?;
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
            let supports_quick_play =
                has_argument(&plan.version["arguments"]["game"], "--quickPlayMultiplayer");
            if identity.is_some() || (server_address.is_some() && supports_quick_play) {
                plan = self.build_plan_with_launch_options(
                    identity.as_ref(),
                    if supports_quick_play {
                        server_address.as_deref()
                    } else {
                        None
                    },
                )?;
            }
            if let Some(address) = server_address.as_deref().filter(|_| !supports_quick_play) {
                let (host, port) = crate::servers::parse_address(address)?;
                plan.args
                    .extend(["--server".into(), host, "--port".into(), port.to_string()]);
            }
            let join_mode = if let Some(address) = server_address.as_deref() {
                let (host, port) = crate::servers::parse_address(address)?;
                let host = if host.contains(':') {
                    format!("[{host}]")
                } else {
                    host
                };
                let endpoint = format!("{host}:{port}");
                if supports_quick_play {
                    if !plan
                        .args
                        .windows(2)
                        .any(|args| args[0] == "--quickPlayMultiplayer" && args[1] == endpoint)
                    {
                        return Err(format!(
                            "无法连接服务器：{} 的版本配置没有生成 Quick Play 直连参数。",
                            s(&plan.config, "versionId")
                        ));
                    }
                    format!("Minecraft Quick Play · {endpoint}")
                } else {
                    format!("传统服务器参数 · {endpoint}")
                }
            } else {
                "普通启动".into()
            };
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
                    "启动 {} · {} · Java {major} · {} MB · {}用户 {}",
                    s(&plan.config, "versionId"),
                    join_mode,
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
            let mut process = child.lock().unwrap();
            if process.try_wait().map_err(err)?.is_some() {
                return Ok(());
            }
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                let tree_kill = Command::new("taskkill.exe")
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .creation_flags(0x08000000)
                    .status();
                if !matches!(tree_kill, Ok(status) if status.success()) {
                    process.kill().map_err(err)?;
                }
            }
            #[cfg(not(windows))]
            process.kill().map_err(err)?;
            process.wait().map_err(err)?;
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
    fn download(&self, item: &FileItem, progress: &mut dyn FnMut(u64, u64)) -> R<()> {
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
            "{}.xumob-{}-{}.tmp",
            item.file.file_name().unwrap_or_default().to_string_lossy(),
            Utc::now().timestamp_nanos_opt().unwrap_or(0),
            std::process::id()
        ));
        let client = crate::net::download_client()?;
        let mut failure = String::new();
        for _ in 0..2 {
            let result = (|| {
                let mut response = client.get(url).send().map_err(err)?;
                if !response.status().is_success() {
                    return Err(format!("下载失败（{}）", response.status()));
                }
                let total_size = response.content_length().unwrap_or(0);
                fs::create_dir_all(item.file.parent().unwrap()).map_err(err)?;
                let mut file = File::create(&temporary).map_err(err)?;
                let mut hash = Sha1::new();
                let mut size = 0u64;
                let mut last_report = Instant::now() - Duration::from_secs(1);
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
                    if last_report.elapsed() >= Duration::from_millis(120) {
                        progress(size, total_size);
                        last_report = Instant::now();
                    }
                }
                progress(size, total_size);
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
                let item = json_file_item(index);
                let label = item.label.clone();
                self.download(&item, &mut |bytes, total_bytes| {
                    self.progress_to(
                        "repairing",
                        &format!("正在下载 {label}"),
                        json!({"filename":label,"bytes":bytes,"totalBytes":total_bytes,"fileIndex":1,"fileCount":1,"unit":"bytes"}),
                    );
                })?;
                added_index = 1;
                checked = self.inspect_files(&plan, true)?;
            }
            let items = list(&checked["missing"]);
            let total = items.len();
            let queue = Arc::new(Mutex::new(VecDeque::from(items)));
            let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let downloaded_bytes = Arc::new(std::sync::atomic::AtomicU64::new(0));
            let mut workers = Vec::new();
            // Keep a bounded pool: enough parallel requests to saturate typical
            // broadband links without exhausting Windows sockets on asset batches.
            for _ in 0..8.min(total) {
                let service = self.clone();
                let queue = queue.clone();
                let completed = completed.clone();
                let downloaded_bytes = downloaded_bytes.clone();
                workers.push(thread::spawn(move || loop {
                    let next = queue.lock().unwrap().pop_front();
                    if let Some(next) = next {
                        let item = json_file_item(&next);
                        let label = item.label.clone();
                        let mut file_bytes = 0u64;
                        service.download(&item, &mut |bytes, _| {
                            let delta = bytes.saturating_sub(file_bytes);
                            file_bytes = bytes;
                            let transferred = downloaded_bytes
                                .fetch_add(delta, std::sync::atomic::Ordering::Relaxed)
                                + delta;
                            let count = completed.load(std::sync::atomic::Ordering::Relaxed);
                            service.progress_to(
                                "repairing",
                                &format!("正在补全基础依赖 {count} / {total}"),
                                json!({"filename":label,"bytes":count,"totalBytes":total,"fileIndex":count,"fileCount":total,"downloadedBytes":transferred,"unit":"files"}),
                            );
                        })?;
                        let count =
                            completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                        service.progress_to(
                            "repairing",
                            &format!("正在补全基础依赖 {count} / {total}"),
                            json!({"filename":label,"bytes":count,"totalBytes":total,"fileIndex":count,"fileCount":total,"downloadedBytes":downloaded_bytes.load(std::sync::atomic::Ordering::Relaxed),"unit":"files"}),
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
        let temporary = target.with_extension("xumob-tmp");
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
