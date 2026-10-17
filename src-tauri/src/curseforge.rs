//! CurseForge's documented API, normalized for the shared content resolver.
use crate::{
    core::R,
    modrinth::{ContentFile, Manager, Transport},
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc, Mutex},
};
pub fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}
pub fn a(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
pub struct Vault {
    key: Mutex<String>,
}
impl Vault {
    fn new() -> Arc<Self> {
        let key = std::env::var("XUMOB_CURSEFORGE_API_KEY")
            .ok()
            .or_else(|| option_env!("XUMOB_BUILD_CURSEFORGE_API_KEY").map(str::to_owned))
            .unwrap_or_default();
        Arc::new(Self {
            key: Mutex::new(key),
        })
    }
    pub(crate) fn get(&self) -> R<String> {
        let key = self.key.lock().unwrap().clone();
        if key.is_empty() {
            Err("请先配置 CurseForge API Key。".into())
        } else {
            Ok(key)
        }
    }
    pub fn status(&self) -> Value {
        json!({"configured":!self.key.lock().unwrap().is_empty()})
    }
    pub fn configure(&self, key: &str) -> R<Value> {
        let key = key.trim();
        if key.len() < 20 || key.len() > 512 || key.bytes().any(|b| b.is_ascii_control()) {
            return Err("CurseForge API Key 格式无效。".into());
        }
        *self.key.lock().unwrap() = key.to_owned();
        Ok(self.status())
    }
}
pub struct CurseForge {
    pub content: Arc<Manager>,
    pub vault: Arc<Vault>,
    network: Arc<Network>,
}
impl CurseForge {
    pub(crate) fn identify_metadata(&self, files: &HashMap<String, PathBuf>) -> R<Value> {
        let identified = self.network.identify(files)?;
        let map = identified
            .as_object()
            .ok_or("CurseForge 文件识别响应无效。")?;
        let mut ids = map
            .values()
            .filter_map(|v| v["project_id"].as_str())
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        let mut projects = HashMap::new();
        for chunk in ids.chunks(50) {
            let ids = chunk
                .iter()
                .filter_map(|id| id.parse::<u64>().ok())
                .collect::<Vec<_>>();
            let response = self
                .network
                .call("/mods", &[], Some(json!({"modIds": ids})))?;
            for project in a(&response["data"]) {
                if let Some(id) = project["id"].as_u64() {
                    projects.insert(id.to_string(), project.clone());
                }
            }
        }
        let mut result = serde_json::Map::new();
        for (hash, file) in map {
            let id = s(file, "project_id");
            if let Some(project) = projects.get(id) {
                result.insert(
                    hash.clone(),
                    json!({
                        "id": id,
                        "title": project["name"],
                        "iconUrl": safe_icon_url(s(&project["logo"], "url")),
                    }),
                );
            }
        }
        Ok(Value::Object(result))
    }
    pub(crate) fn pack_entries(&self, index: &Value, optional: bool) -> R<Vec<Value>> {
        let manifest = a(&index["files"]);
        if manifest.len() > 10000 {
            return Err("整合包文件数量超过限制。".into());
        }
        let requested = manifest
            .iter()
            .filter(|f| optional || f["required"] != false)
            .collect::<Vec<_>>();
        let mut files = HashMap::new();
        let mut mods = HashMap::new();
        for chunk in requested.chunks(50) {
            let ids = chunk
                .iter()
                .map(|f| {
                    f["fileID"]
                        .as_u64()
                        .ok_or("整合包文件标识无效。".to_string())
                })
                .collect::<R<Vec<_>>>()?;
            let result = self
                .network
                .call("/mods/files", &[], Some(json!({"fileIds":ids})))?;
            for f in a(&result["data"]) {
                if let Some(id) = f["id"].as_u64() {
                    files.insert(id, f.clone());
                }
            }
            let ids = chunk
                .iter()
                .map(|f| {
                    f["projectID"]
                        .as_u64()
                        .ok_or("整合包项目标识无效。".to_string())
                })
                .collect::<R<Vec<_>>>()?;
            let result = self
                .network
                .call("/mods", &[], Some(json!({"modIds":ids})))?;
            for m in a(&result["data"]) {
                if let Some(id) = m["id"].as_u64() {
                    mods.insert(id, m.clone());
                }
            }
        }
        requested
            .into_iter()
            .map(|entry| {
                let file_id = entry["fileID"].as_u64().ok_or("整合包文件标识无效。")?;
                let project_id = entry["projectID"].as_u64().ok_or("整合包项目标识无效。")?;
                let file = files.get(&file_id).ok_or("整合包中的文件已不可用。")?;
                if file["modId"].as_u64() != Some(project_id) {
                    return Err("整合包项目与文件标识不一致。".into());
                }
                let project = mods
                    .get(&project_id)
                    .ok_or("整合包中的项目已不可用。")?;
                let urls = file_download_urls(file);
                if urls.is_empty() {
                    return Err(format!("CurseForge 文件信息不完整：{}（项目 {project_id}，文件 {file_id}）。请重新检查安装配置。", s(project, "name")));
                }
                let class_id = project["classId"].as_u64();
                let folder = match class_id {
                    Some(6) => "mods",
                    Some(12) => "resourcepacks",
                    Some(6552) => "shaderpacks",
                    // CurseForge's Data Packs class. Keep these in the instance-level
                    // datapacks directory so compatible global datapack loaders can
                    // discover them, matching CurseForge's own profile layout.
                    Some(6945) => "datapacks",
                    _ => infer_pack_folder(file),
                };
                let normalized = normalize_file(file);
                let normalized_file = &normalized["files"][0];
                Ok(json!({
                    "path": format!("{folder}/{}", s(normalized_file, "filename")),
                    "downloads": urls,
                    "fileSize": normalized_file["size"],
                    "hashes": normalized_file["hashes"],
                }))
            })
            .collect()
    }
    pub fn new(_data: &Path) -> R<Arc<Self>> {
        let vault = Vault::new();
        let network = Arc::new(Network {
            vault: vault.clone(),
            client: crate::net::client()?,
        });
        Ok(Arc::new(Self {
            content: Manager::with_transport(network.clone()),
            vault,
            network,
        }))
    }
    pub fn file(&self, project: &str, id: &str) -> R<Value> {
        digits(project)?;
        digits(id)?;
        self.network
            .call(&format!("/mods/{project}/files/{id}"), &[], None)
            .map(|v| normalize_file(&v["data"]))
    }
}
pub(crate) fn safe_icon_url(value: &str) -> String {
    let Ok(url) = crate::net::allowed(value) else {
        return String::new();
    };
    if url
        .host_str()
        .is_some_and(|host| host == "forgecdn.net" || host.ends_with(".forgecdn.net"))
    {
        url.to_string()
    } else {
        String::new()
    }
}
fn digits(v: &str) -> R<()> {
    if v.is_empty() || v.len() > 20 || !v.bytes().all(|b| b.is_ascii_digit()) {
        Err("CurseForge 项目标识无效。".into())
    } else {
        Ok(())
    }
}
struct Network {
    vault: Arc<Vault>,
    client: reqwest::blocking::Client,
}
impl Network {
    fn call(&self, path: &str, q: &[(String, String)], body: Option<Value>) -> R<Value> {
        let key = self.vault.get()?;
        let url = format!("https://api.curseforge.com/v1{path}");
        let req = if let Some(body) = body {
            self.client.post(url).json(&body)
        } else {
            self.client.get(url).query(q)
        };
        let r = req
            .header("x-api-key", key)
            .header("Accept", "application/json")
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .map_err(|_| "无法连接 CurseForge，请检查网络后重试。")?;
        if !r.status().is_success() {
            return Err(match r.status().as_u16() {
                401 | 403 => "CurseForge 拒绝了请求，请检查 API Key 或文件下载授权。",
                429 => "CurseForge 请求过于频繁，请稍后重试。",
                404 => "CurseForge 项目或文件已不可用。",
                _ => "CurseForge 服务暂时不可用。",
            }
            .into());
        }
        let mut data = Vec::new();
        r.take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut data)
            .map_err(|_| "无法读取 CurseForge 响应。")?;
        if data.len() > 16 * 1024 * 1024 {
            return Err("CurseForge 响应过大，请缩小筛选。".into());
        }
        serde_json::from_slice(&data).map_err(|_| "CurseForge 返回的数据无效。".into())
    }
    fn files(&self, id: &str, q: &[(String, String)]) -> R<Value> {
        digits(id)?;
        let mut out = Vec::new();
        for index in (0..10000).step_by(50) {
            let mut q = q.to_vec();
            q.extend([
                ("index".into(), index.to_string()),
                ("pageSize".into(), "50".into()),
            ]);
            let v = self.call(&format!("/mods/{id}/files"), &q, None)?;
            let files = a(&v["data"]);
            out.extend(
                files
                    .iter()
                    .filter(|f| f["isAvailable"] != false && f["isServerPack"] != true)
                    .map(normalize_file),
            );
            if files.len() < 50
                || index + 50 >= v["pagination"]["totalCount"].as_u64().unwrap_or(0) as usize
            {
                break;
            }
        }
        Ok(json!(out))
    }
}
fn class(kind: &str) -> u32 {
    match kind {
        "modpack" => 4471,
        "resourcepack" => 12,
        "shader" => 6552,
        _ => 6,
    }
}
fn kind(id: u64) -> &'static str {
    match id {
        4471 => "modpack",
        12 => "resourcepack",
        6552 => "shader",
        _ => "mod",
    }
}
fn loader(v: &str) -> Option<u32> {
    match v {
        "forge" => Some(1),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" => Some(6),
        _ => None,
    }
}
fn loaders(f: &Value) -> Vec<String> {
    let mut values = a(&f["gameVersions"])
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_ascii_lowercase)
        .filter(|s| ["forge", "fabric", "neoforge", "quilt"].contains(&s.as_str()))
        .collect::<Vec<_>>();
    if values.is_empty() {
        values.extend(["minecraft", "iris", "optifine"].map(str::to_owned))
    }
    values
}
// Reference: PCL ResourceVersion.FromPlatformJson (Meloong-Git/PCL), by 龙腾猫跃.
// An empty API downloadUrl does not mean the public CDN file is unavailable.
fn file_download_urls(file: &Value) -> Vec<String> {
    let mut urls = Vec::new();
    let supplied = s(file, "downloadUrl");
    if crate::net::allowed(supplied).is_ok() {
        urls.extend(crate::net::download_sources(supplied));
    }
    let name = s(file, "fileName");
    if let Some(id) = file["id"].as_u64().filter(|id| *id > 0) {
        if !name.is_empty()
            && name != "."
            && name != ".."
            && !name
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\'))
        {
            let mut url = reqwest::Url::parse("https://mediafilez.forgecdn.net/").unwrap();
            url.path_segments_mut().unwrap().extend([
                "files",
                &(id / 1000).to_string(),
                &(id % 1000).to_string(),
                name,
            ]);
            for candidate in crate::net::download_sources(url.as_str()) {
                if !urls.contains(&candidate) {
                    urls.push(candidate);
                }
            }
        }
    }
    urls
}

fn infer_pack_folder(file: &Value) -> &'static str {
    let has_module = |name: &str| a(&file["modules"]).iter().any(|m| s(m, "name") == name);
    if s(file, "fileName").to_ascii_lowercase().ends_with(".jar")
        || has_module("META-INF")
        || has_module("mcmod.info")
        || has_module("fabric.mod.json")
    {
        "mods"
    } else if has_module("shaders") {
        "shaderpacks"
    } else if has_module("pack.mcmeta") {
        "resourcepacks"
    } else {
        "mods"
    }
}

fn normalize_file(f: &Value) -> Value {
    let urls = file_download_urls(f);
    let url = urls.first().map(String::as_str).unwrap_or("");
    let sha1 = a(&f["hashes"])
        .iter()
        .find(|h| h["algo"] == 1)
        .map(|h| s(h, "value"))
        .unwrap_or("");
    let games = a(&f["gameVersions"])
        .iter()
        .filter_map(Value::as_str)
        .filter(|s| s.bytes().next().is_some_and(|b| b.is_ascii_digit()) && !s.starts_with("Java"))
        .collect::<Vec<_>>();
    let dependencies = a(&f["dependencies"])
        .iter()
        .filter_map(|d| {
            let t = match d["relationType"].as_u64() {
                Some(3) => "required",
                Some(5) => "incompatible",
                _ => return None,
            };
            Some(json!({"project_id":d["modId"].to_string(),"version_id":null,"dependency_type":t}))
        })
        .collect::<Vec<_>>();
    json!({"id":f["id"].to_string(),"project_id":f["modId"].to_string(),"name":f["displayName"],"version_number":f["displayName"],"version_type":match f["releaseType"].as_u64(){Some(2)=>"beta",Some(3)=>"alpha",_=>"release"},"date_published":f["fileDate"],"downloads":f["downloadCount"],"loaders":loaders(f),"game_versions":games,"dependencies":dependencies,"files":[{"filename":f["fileName"],"url":url,"size":f["fileLength"],"primary":true,"hashes":{"sha1":sha1}}]})
}
impl Transport for Network {
    fn allow_sha1(&self) -> bool {
        true
    }
    fn api(&self, route: &str, query: &[(String, String)], _body: Option<Value>) -> R<Value> {
        let q: HashMap<_, _> = query.iter().cloned().collect();
        if route == "/search" {
            let facets: Value =
                serde_json::from_str(q.get("facets").map(String::as_str).unwrap_or("[]"))
                    .map_err(|_| "筛选条件无效。")?;
            let mut values = HashMap::new();
            let mut categories = Vec::new();
            for v in a(&facets).iter().flat_map(a).filter_map(Value::as_str) {
                if let Some((k, v)) = v.split_once(':') {
                    if k == "categories" {
                        categories.push(v.to_owned());
                    } else {
                        values.insert(k.to_owned(), v.to_owned());
                    }
                }
            }
            let k = values
                .get("project_type")
                .map(String::as_str)
                .unwrap_or("mod");
            let mut params = vec![
                ("gameId".into(), "432".into()),
                ("classId".into(), class(k).to_string()),
                (
                    "searchFilter".into(),
                    q.get("query").cloned().unwrap_or_default(),
                ),
                ("index".into(), q.get("offset").cloned().unwrap_or_default()),
                ("pageSize".into(), "20".into()),
                ("sortOrder".into(), "desc".into()),
                (
                    "sortField".into(),
                    match q.get("index").map(String::as_str) {
                        Some("downloads") => "6",
                        Some("updated") => "3",
                        Some("newest") => "11",
                        _ => "1",
                    }
                    .into(),
                ),
            ];
            if let Some(v) = values.get("versions") {
                params.push(("gameVersion".into(), v.clone()))
            }
            for v in &categories {
                if let Some(l) = loader(v) {
                    params.push(("modLoaderType".into(), l.to_string()))
                } else if v.bytes().all(|b| b.is_ascii_digit()) {
                    params.push(("categoryId".into(), v.clone()))
                }
            }
            let v = self.call("/mods/search", &params, None)?;
            return Ok(
                json!({"total_hits":v["pagination"]["totalCount"],"hits":a(&v["data"]).iter().map(|m|json!({"project_id":m["id"].to_string(),"title":m["name"],"description":m["summary"],"author":a(&m["authors"]).iter().map(|a|s(a,"name")).collect::<Vec<_>>().join(", "),"downloads":m["downloadCount"],"categories":a(&m["categories"]).iter().map(|v|s(v,"name")).collect::<Vec<_>>(),"icon_url":m["logo"]["url"],"slug":m["slug"]})).collect::<Vec<_>>()}),
            );
        }
        if route == "/tag/category" {
            let v = self.call("/categories", &[("gameId".into(), "432".into())], None)?;
            let categories = a(&v["data"]);
            return Ok(json!(categories.iter().filter(|c|c["isClass"]!=true).filter_map(|c|category_type(c,categories).map(|kind|json!({"name":c["id"].to_string(),"project_type":kind,"header":c["name"]}))).collect::<Vec<_>>()));
        }
        if route == "/tag/loader" {
            return Ok(json!([]));
        }
        if route == "/tag/game_version" {
            let v = crate::net::json(
                "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
            )?;
            return Ok(json!(a(&v["versions"]).iter().map(|v|json!({"version":v["id"],"version_type":v["type"],"date":v["releaseTime"],"major":false})).collect::<Vec<_>>()));
        }
        let parts = route.trim_matches('/').split('/').collect::<Vec<_>>();
        if parts.len() >= 2 && parts[0] == "project" {
            let id = parts[1];
            digits(id)?;
            if parts.len() == 3 && parts[2] == "version" {
                let mut params = Vec::new();
                if let Some(g) = q.get("game_versions") {
                    let v: Value = serde_json::from_str(g).map_err(|_| "筛选无效。")?;
                    if let Some(g) = v[0].as_str() {
                        params.push(("gameVersion".into(), g.into()))
                    }
                }
                if let Some(l) = q.get("loaders") {
                    let v: Value = serde_json::from_str(l).map_err(|_| "筛选无效。")?;
                    if let Some(l) = loader(v[0].as_str().unwrap_or("")) {
                        params.push(("modLoaderType".into(), l.to_string()))
                    }
                }
                return self.files(id, &params);
            }
            let m = self.call(&format!("/mods/{id}"), &[], None)?;
            let m = &m["data"];
            let body = self.call(&format!("/mods/{id}/description"), &[], None)?;
            return Ok(
                json!({"id":id,"title":m["name"],"description":m["summary"],"body":body["data"],"project_type":kind(m["classId"].as_u64().unwrap_or(6)),"slug":m["slug"],"icon_url":m["logo"]["url"],"downloads":m["downloadCount"],"client_side":"optional","source_url":m["links"]["sourceUrl"],"issues_url":m["links"]["issuesUrl"],"wiki_url":m["links"]["wikiUrl"]}),
            );
        }
        if parts.len() == 2 && parts[0] == "version" {
            digits(parts[1])?;
            let id = parts[1].parse::<u64>().map_err(|_| "文件标识无效。")?;
            let v = self.call("/mods/files", &[], Some(json!({"fileIds":[id]})))?;
            return a(&v["data"])
                .first()
                .map(normalize_file)
                .ok_or("该文件已不可用。".into());
        }
        Err("不支持的 CurseForge 请求。".into())
    }
    fn download(
        &self,
        file: &ContentFile,
        out: &mut File,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64),
    ) -> R<()> {
        if file.url.is_empty() {
            return Err(
                "作者未允许通过第三方启动器下载此文件，请前往 CurseForge 项目页手动下载。".into(),
            );
        }
        let key = self.vault.get()?;
        let mut last_error = "所有官方文件节点均暂时不可用，请稍后重试。".to_string();
        for url in crate::net::download_sources(&file.url) {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("下载已取消。".into());
            }
            out.set_len(0).map_err(|_| "无法清理未完成的下载文件。")?;
            out.seek(SeekFrom::Start(0))
                .map_err(|_| "无法写入下载文件。")?;
            match crate::net::stream(&url, out, file.size, cancel, Some(&key), progress) {
                Ok(()) => return Ok(()),
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }
    fn identify(&self, files: &HashMap<String, PathBuf>) -> R<Value> {
        let mut fingerprints = HashMap::new();
        for (hash, path) in files {
            fingerprints.insert(fingerprint(path)?, hash.clone());
        }
        let mut result = serde_json::Map::new();
        for chunk in fingerprints.keys().copied().collect::<Vec<_>>().chunks(100) {
            let v = self.call("/fingerprints", &[], Some(json!({"fingerprints":chunk})))?;
            for m in a(&v["data"]["exactMatches"]) {
                if let Some(hash) = m["file"]["fileFingerprint"]
                    .as_u64()
                    .and_then(|n| fingerprints.get(&(n as u32)))
                {
                    let version = normalize_file(&m["file"]);
                    if s(&version["files"][0]["hashes"], "sha1").eq_ignore_ascii_case(hash) {
                        result.insert(hash.clone(), version);
                    }
                }
            }
        }
        Ok(Value::Object(result))
    }
}
fn category_type(category: &Value, categories: &[Value]) -> Option<&'static str> {
    let mut current = category;
    for _ in 0..16 {
        for id in [current["classId"].as_u64(), current["id"].as_u64()]
            .into_iter()
            .flatten()
        {
            match id {
                6 => return Some("mod"),
                12 => return Some("resourcepack"),
                6552 => return Some("shader"),
                4471 => return Some("modpack"),
                _ => {}
            }
        }
        let parent = current["parentCategoryId"].as_u64()?;
        if parent == 0 {
            return None;
        }
        current = categories
            .iter()
            .find(|c| c["id"].as_u64() == Some(parent))?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_api_url_gets_exact_official_cdn_file() {
        let file =
            json!({"id": 8619298u64, "fileName": "spell_enchants_1.20.1.zip", "downloadUrl": null});
        let urls = file_download_urls(&file);
        assert_eq!(
            urls[0],
            "https://mediafilez.forgecdn.net/files/8619/298/spell_enchants_1.20.1.zip"
        );
        assert_eq!(urls.len(), 3);
        assert_eq!(normalize_file(&file)["files"][0]["url"], urls[0]);
    }
    #[test]
    fn cdn_file_ids_and_names_are_encoded_without_path_injection() {
        let file = json!({"id": 10000001u64, "fileName": "Pack™ 1.0+#.zip", "downloadUrl": ""});
        let urls = file_download_urls(&file);
        let url = reqwest::Url::parse(&urls[0]).unwrap();
        assert!(url.path().starts_with("/files/10000/1/"));
        assert!(url.path().contains("%E2%84%A2%20"));
        assert!(url.path().contains("%23"));
        assert_eq!(url.fragment(), None);
        assert!(file_download_urls(&json!({"id": 1, "fileName": "../bad.jar"})).is_empty());
        assert!(file_download_urls(&json!({"id": 1, "fileName": "..\\bad.jar"})).is_empty());
        assert!(file_download_urls(&json!({"id": 1, "fileName": ".."})).is_empty());
        assert!(file_download_urls(&json!({"fileName": "mod.jar"})).is_empty());
    }
    #[test]
    fn missing_project_class_uses_file_modules() {
        assert_eq!(infer_pack_folder(&json!({"fileName":"a.jar"})), "mods");
        assert_eq!(
            infer_pack_folder(&json!({"modules":[{"name":"pack.mcmeta"}]})),
            "resourcepacks"
        );
        assert_eq!(
            infer_pack_folder(&json!({"modules":[{"name":"shaders"}]})),
            "shaderpacks"
        );
    }
    #[test]
    fn required_file_dependencies_are_preserved_for_recursive_resolution() {
        let file = json!({
            "id": 1001,
            "modId": 2001,
            "fileName": "root.jar",
            "fileLength": 1234,
            "downloadUrl": "https://mediafilez.forgecdn.net/files/1/1/root.jar",
            "dependencies": [
                {"modId": 2002, "relationType": 3},
                {"modId": 2003, "relationType": 2},
                {"modId": 2004, "relationType": 5}
            ]
        });
        let normalized = normalize_file(&file);
        assert_eq!(a(&normalized["dependencies"]).len(), 2);
        assert_eq!(normalized["dependencies"][0]["project_id"], "2002");
        assert_eq!(normalized["dependencies"][0]["dependency_type"], "required");
        assert_eq!(
            normalized["dependencies"][1]["dependency_type"],
            "incompatible"
        );
    }
    #[test]
    fn categories_follow_their_class_instead_of_defaulting_to_mods() {
        let categories = json!([{"id":6,"isClass":true},{"id":12,"isClass":true},{"id":5,"isClass":true},{"id":100,"parentCategoryId":6},{"id":101,"parentCategoryId":100},{"id":200,"classId":12},{"id":300,"parentCategoryId":5}]);
        let list = a(&categories);
        assert_eq!(category_type(&list[4], list), Some("mod"));
        assert_eq!(category_type(&list[5], list), Some("resourcepack"));
        assert_eq!(category_type(&list[6], list), None);
    }
}
// CurseForge fingerprint: MurmurHash2(seed=1), ignoring ASCII whitespace.
fn fingerprint(path: &Path) -> R<u32> {
    let mut f = File::open(path).map_err(|_| "无法识别已安装文件。")?;
    let mut buf = [0; 65536];
    let mut len = 0u32;
    loop {
        let n = f.read(&mut buf).map_err(|_| "文件识别失败。")?;
        if n == 0 {
            break;
        }
        len += buf[..n]
            .iter()
            .filter(|b| !matches!(**b, 9 | 10 | 13 | 32))
            .count() as u32
    }
    f.seek(SeekFrom::Start(0)).map_err(|_| "文件识别失败。")?;
    let m = 0x5bd1e995u32;
    let mut h = 1 ^ len;
    let mut tail = Vec::with_capacity(4);
    loop {
        let n = f.read(&mut buf).map_err(|_| "文件识别失败。")?;
        if n == 0 {
            break;
        }
        for b in &buf[..n] {
            if matches!(*b, 9 | 10 | 13 | 32) {
                continue;
            }
            tail.push(*b);
            if tail.len() == 4 {
                let mut k = u32::from_le_bytes(tail[..].try_into().unwrap());
                k = k.wrapping_mul(m);
                k ^= k >> 24;
                k = k.wrapping_mul(m);
                h = h.wrapping_mul(m) ^ k;
                tail.clear();
            }
        }
    }
    for (i, b) in tail.iter().enumerate() {
        h ^= (*b as u32) << (i * 8)
    }
    if !tail.is_empty() {
        h = h.wrapping_mul(m)
    }
    h ^= h >> 13;
    h = h.wrapping_mul(m);
    h ^= h >> 15;
    Ok(h)
}
