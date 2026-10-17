//! Microsoft device authorization and Xbox/Minecraft sessions. Secrets stay native.
use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

type R<T> = Result<T, String>;
const SCOPE: &str = "XboxLive.SignIn XboxLive.offline_access";
fn field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
fn token(v: &Value, key: &str) -> R<String> {
    let value = field(v, key);
    if value.is_empty() || value.len() > 16384 || value.contains(['\r', '\n', '\0']) {
        return Err("认证服务返回了无效的凭据，请重新登录。".into());
    }
    Ok(value.into())
}
fn epoch() -> i64 {
    Utc::now().timestamp()
}
fn valid_client(value: &str) -> bool {
    Regex::new(r"(?i)^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$")
        .unwrap()
        .is_match(value)
        && value != "00000000-0000-0000-0000-000000000000"
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub xuid: String,
    #[serde(rename = "skinUrl", default)]
    pub skin_url: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Config {
    mode: String,
    // Read legacy account.json once; never serialize the app identifier there.
    #[serde(rename = "clientId", default, skip_serializing)]
    client_id: String,
    profile: Option<Profile>,
    #[serde(default)]
    accounts: Vec<AccountRecord>,
    #[serde(default)]
    active_account_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Session {
    client_id: String,
    refresh_token: String,
    access_token: String,
    expires_at: i64,
    profile: Profile,
    #[serde(default, alias = "wam_account_id")]
    broker_account_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct YggdrasilSession {
    api_root: String,
    username: String,
    access_token: String,
    client_token: String,
    profile: Profile,
    metadata: Value,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "credential")]
enum Credential {
    Microsoft(Session),
    Yggdrasil(YggdrasilSession),
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountRecord {
    id: String,
    mode: String,
    name: String,
    username: String,
    uuid: String,
    #[serde(default)]
    api_root: String,
    #[serde(default)]
    skin_url: String,
}
// Intentionally not serializable or Debug: access tokens must never enter snapshots.
pub struct Identity {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    pub client_id: String,
    pub xuid: String,
    pub user_type: String,
    pub api_root: Option<String>,
    pub prefetched_metadata: Option<String>,
}
impl Identity {
    pub fn variables(&self) -> Value {
        let session = if self.user_type == "mojang" {
            self.access_token.clone()
        } else {
            format!("token:{}:{}", self.access_token, self.uuid)
        };
        json!({"auth_player_name":self.name,"auth_uuid":self.uuid,"auth_access_token":self.access_token,"auth_session":session,"user_type":self.user_type,"user_properties":"{}","clientid":self.client_id,"auth_xuid":self.xuid})
    }
}
fn ygg_identity(session: &YggdrasilSession) -> R<Identity> {
    use base64::Engine as _;
    let metadata = serde_json::to_vec(&session.metadata).map_err(|_| "无法读取 Yggdrasil 服务信息。")?;
    Ok(Identity {
        name: session.profile.name.clone(),
        uuid: session.profile.id.clone(),
        access_token: session.access_token.clone(),
        client_id: String::new(),
        xuid: String::new(),
        user_type: "mojang".into(),
        api_root: Some(session.api_root.clone()),
        prefetched_metadata: Some(base64::engine::general_purpose::STANDARD.encode(metadata)),
    })
}
#[derive(Clone)]
struct Pending {
    id: u64,
    device_code: String,
    uri: String,
    expires: Instant,
    interval: u64,
    next: Instant,
    client_id: String,
}
struct State {
    config: Config,
    session: Option<Session>,
    pending: Option<Pending>,
    generation: u64,
    needs_login: bool,
    credentials: HashMap<String, Credential>,
}
pub struct AuthManager {
    directory: PathBuf,
    state: Mutex<State>,
    operation: Mutex<()>,
    http: Arc<dyn Transport>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Endpoint {
    Device,
    Microsoft,
    Xbox,
    Xsts,
    Minecraft,
    License,
    Profile,
}
impl Endpoint {
    fn url(self) -> &'static str {
        match self {
            Self::Device => "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode",
            Self::Microsoft => "https://login.microsoftonline.com/consumers/oauth2/v2.0/token",
            Self::Xbox => "https://user.auth.xboxlive.com/user/authenticate",
            Self::Xsts => "https://xsts.auth.xboxlive.com/xsts/authorize",
            Self::Minecraft => "https://api.minecraftservices.com/authentication/login_with_xbox",
            Self::License => "https://api.minecraftservices.com/entitlements/license",
            Self::Profile => "https://api.minecraftservices.com/minecraft/profile",
        }
    }
}
enum Body {
    Form(Vec<(&'static str, String)>),
    Json(Value),
    Empty,
}
struct Reply {
    status: u16,
    data: Value,
}
trait Transport: Send + Sync {
    fn send(&self, endpoint: Endpoint, body: Body, bearer: Option<&str>) -> R<Reply>;
}
struct Network;
impl Transport for Network {
    fn send(&self, endpoint: Endpoint, body: Body, bearer: Option<&str>) -> R<Reply> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(25))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("XUMOB-Minecraft-Launcher/0.7.0")
            .build()
            .map_err(|_| "无法初始化登录网络连接。")?;
        let request = match body {
            Body::Form(values) => client.post(endpoint.url()).form(&values),
            Body::Json(value) => client.post(endpoint.url()).json(&value),
            Body::Empty => client.get(endpoint.url()),
        };
        let mut request = request.header("Accept", "application/json");
        if endpoint == Endpoint::License {
            use sha2::{Digest, Sha256};
            let hash = hex::encode(Sha256::digest(format!(
                "{}:{}",
                std::process::id(),
                Utc::now().timestamp_nanos_opt().unwrap_or_default()
            )));
            let request_id = format!(
                "{}-{}-4{}-a{}-{}",
                &hash[..8],
                &hash[8..12],
                &hash[13..16],
                &hash[17..20],
                &hash[20..32]
            );
            request = request.query(&[("requestId", request_id)]);
        }
        if matches!(endpoint, Endpoint::Xbox | Endpoint::Xsts) {
            request = request.header("x-xbl-contract-version", "1");
        }
        if let Some(value) = bearer {
            request = request.bearer_auth(value);
        }
        let response = request.send().map_err(|e| {
            if e.is_timeout() {
                "登录连接超时，请检查网络后重试。"
            } else {
                "无法连接登录服务，请检查网络后重试。"
            }
        })?;
        let status = response.status().as_u16();
        let mut bytes = Vec::new();
        response
            .take(131073)
            .read_to_end(&mut bytes)
            .map_err(|_| "无法读取登录服务响应。")?;
        if bytes.len() > 131072 {
            return Err("登录服务响应过大。".into());
        }
        let data = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Ok(Reply { status, data })
    }
}
fn response(reply: Reply, stage: Endpoint) -> R<Value> {
    if (200..300).contains(&reply.status) && reply.data.is_object() {
        return Ok(reply.data);
    }
    if stage == Endpoint::Microsoft || stage == Endpoint::Device {
        return Err(oauth_error(field(&reply.data, "error")));
    }
    if matches!(stage, Endpoint::Xbox | Endpoint::Xsts) {
        return Err(match reply.data["XErr"].as_u64().unwrap_or(0) {
            2148916233 => "该微软账户尚未建立 Xbox 档案，请先在 Xbox 官网完成账户设置。",
            2148916234 => "请先在 Xbox 官网接受服务条款，再重新登录。",
            2148916235 => "该账户所在地区暂不支持 Xbox 登录服务。",
            2148916236 => "该账户需要在微软官网完成年龄验证。",
            2148916237 => "该账户已达到家庭设置中的可用游戏时间。",
            2148916238 | 2148916229 => "该账户需要完成微软家庭设置并获得监护人允许。",
            _ => "Xbox 认证失败，请检查账户设置或稍后重试。",
        }
        .into());
    }
    if stage == Endpoint::Minecraft && reply.status == 403 {
        // A 403 can also be an edge/network refusal. Only diagnose registration
        // when the service explicitly says so; never expose arbitrary response text.
        let invalid_app = ["error", "errorMessage", "message"].iter().any(|key| {
            let value = field(&reply.data, key);
            value.len() <= 2048
                && value
                    .to_ascii_lowercase()
                    .contains("invalid app registration")
        });
        let message = if invalid_app {
            "Minecraft 明确拒绝了此应用注册。请由应用所有者核对 Client ID，并申请或确认 Minecraft Java 服务访问许可。"
        } else {
            "Minecraft 认证请求被拒绝，目前无法确定是否与应用许可有关。请重试；若仍失败，请提供下方诊断信息。"
        };
        let kind = if invalid_app {
            "Invalid app registration"
        } else if reply.data.is_object() {
            "未明确说明"
        } else {
            "非 JSON 响应"
        };
        let mut detail = format!("{message}\n诊断：Minecraft 登录；HTTP 403；类型：{kind}");
        // UUID-shaped request references only. Tokens, bodies and server messages
        // are deliberately excluded, including from malformed error responses.
        for key in ["requestId", "correlationId", "traceId", "trace_id"] {
            let value = field(&reply.data, key);
            if valid_client(value) {
                detail.push_str(&format!("；请求编号：{value}"));
                break;
            }
        }
        return Err(detail);
    }
    if stage == Endpoint::Profile && reply.status == 404 {
        return Err("未找到 Java 版角色，请先在 Minecraft 官网设置角色名称。".into());
    }
    if reply.status == 401 {
        return Err("正版登录已失效，请重新进行微软登录。".into());
    }
    if reply.status == 429 {
        return Err("登录请求过于频繁，请稍后重试。".into());
    }
    Err("Minecraft 认证服务暂时不可用，请稍后重试。".into())
}
fn oauth_error(code: &str) -> String {
    match code {
        "invalid_client" | "unauthorized_client" => {
            "微软应用配置不可用，请核对 Client ID 并启用公共客户端流程。"
        }
        "invalid_scope" => "微软应用未允许 Xbox 登录权限，请检查应用配置。",
        "invalid_grant" | "interaction_required" => "微软登录已失效，请重新授权。",
        "authorization_declined" | "access_denied" => "你已拒绝此次登录授权。",
        "expired_token" | "bad_verification_code" => "登录代码已过期，请重新获取。",
        _ => "微软认证失败，请稍后重试。",
    }
    .into()
}
fn safe_uri(value: &str) -> R<String> {
    let url = reqwest::Url::parse(value).map_err(|_| "登录网址无效。")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !matches!(
            url.host_str(),
            Some(
                "microsoft.com"
                    | "www.microsoft.com"
                    | "login.microsoftonline.com"
                    | "login.live.com"
            )
        )
    {
        return Err("登录服务返回了非微软授权网址。".into());
    }
    Ok(url.to_string())
}
fn profile(data: &Value, xuid: &str) -> R<Profile> {
    let id = field(data, "id").replace('-', "").to_lowercase();
    let name = field(data, "name");
    if !Regex::new(r"^[a-f0-9]{32}$").unwrap().is_match(&id)
        || !Regex::new(r"^[A-Za-z0-9_]{1,16}$").unwrap().is_match(name)
    {
        return Err("Minecraft 服务返回的角色资料无效。".into());
    }
    Ok(Profile {
        id,
        name: name.into(),
        xuid: if xuid.len() <= 32 && xuid.bytes().all(|b| b.is_ascii_digit()) {
            xuid.into()
        } else {
            String::new()
        },
        skin_url: String::new(),
    })
}
fn write_atomic(file: &Path, bytes: &[u8]) -> R<()> {
    let temporary = file.with_extension("new");
    fs::write(&temporary, bytes).map_err(|_| "无法保存账户配置，请检查数据目录权限。")?;
    fs::rename(&temporary, file).map_err(|_| "无法保存账户配置，请检查数据目录权限。".to_string())
}
fn account_id(mode: &str, identity: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = hex::encode(Sha256::digest(identity.as_bytes()));
    format!("{mode}-{}", &hash[..24])
}
fn valid_username(name: &str) -> bool {
    (3..=16).contains(&name.len())
        && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}
fn yggdrasil_profile(value: &Value) -> R<Profile> {
    let id = field(value, "id").replace('-', "").to_lowercase();
    let name = field(value, "name");
    if id.len() != 32
        || !id.bytes().all(|c| c.is_ascii_hexdigit())
        || !valid_username(name)
    {
        return Err("认证服务器没有返回有效的 Minecraft 角色。".into());
    }
    Ok(Profile { id, name: name.into(), xuid: String::new(), skin_url: String::new() })
}
fn ygg_skin_url(profile: &Value, metadata: &Value) -> Option<String> {
    use base64::Engine as _;
    let property = profile.get("properties")?.as_array()?.iter().find(|property| {
        field(property, "name") == "textures"
    })?;
    let encoded = field(property, "value");
    if encoded.is_empty() || encoded.len() > 128 * 1024 { return None; }
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(encoded))
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(encoded))
        .ok()?;
    if decoded.len() > 96 * 1024 { return None; }
    let textures: Value = serde_json::from_slice(&decoded).ok()?;
    let url = reqwest::Url::parse(field(&textures["textures"]["SKIN"], "url")).ok()?;
    if url.scheme() != "https" || url.host_str().is_none() { return None; }
    let host = url.host_str()?.to_ascii_lowercase();
    let allowed = metadata.get("skinDomains").and_then(Value::as_array).into_iter().flatten()
        .filter_map(Value::as_str)
        .chain([".minecraft.net", ".mojang.com"])
        .any(|domain| {
            let domain = domain.trim_start_matches('.').to_ascii_lowercase();
            host == domain || host.ends_with(&format!(".{domain}"))
        });
    allowed.then(|| url.to_string())
}
fn ygg_fetch_skin(root: &reqwest::Url, profile: &Value, metadata: &Value) -> String {
    if let Some(url) = ygg_skin_url(profile, metadata) { return url; }
    let id = field(profile, "id").replace('-', "");
    if id.len() != 32 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) { return String::new(); }
    let Some(endpoint) = root.join(&format!("sessionserver/session/minecraft/profile/{id}?unsigned=false")).ok() else {
        return String::new();
    };
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.url().scheme() == "https" && attempt.previous().len() < 3 { attempt.follow() } else { attempt.stop() }
        }))
        .user_agent("XUMOB-Minecraft-Launcher/0.7.0")
        .build() else { return String::new(); };
    let Ok(response) = client.get(endpoint).send() else { return String::new(); };
    if !response.status().is_success() || response.url().scheme() != "https" { return String::new(); }
    let Ok(bytes) = response.bytes() else { return String::new(); };
    if bytes.len() > 256 * 1024 { return String::new(); }
    let Ok(profile) = serde_json::from_slice::<Value>(&bytes) else { return String::new(); };
    ygg_skin_url(&profile, metadata).unwrap_or_default()
}
fn ygg_url(value: &str) -> R<reqwest::Url> {
    let value = value.trim();
    let value = if value.contains("://") { value.to_string() } else { format!("https://{value}") };
    let url = reqwest::Url::parse(&value).map_err(|_| "Yggdrasil API 地址无效。")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Yggdrasil API 必须使用安全的 HTTPS 地址。".into());
    }
    Ok(url)
}
fn ygg_metadata(root: &reqwest::Url) -> R<(reqwest::Url, Value)> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.url().scheme() == "https" && attempt.previous().len() < 5 {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .user_agent("XUMOB-Minecraft-Launcher/0.7.0")
        .build()
        .map_err(|_| "无法连接 Yggdrasil 认证服务器。")?;
    let response = client.get(root.clone()).send().map_err(|_| "无法连接 Yggdrasil 认证服务器，请检查 API 地址和网络。")?;
    if response.url().scheme() != "https" {
        return Err("Yggdrasil 认证服务器跳转到了不安全的地址。".into());
    }
    let final_url = response.url().clone();
    let location = response.headers().get("x-authlib-injector-api-location").and_then(|v| v.to_str().ok()).map(str::to_string);
    let resolved_root = if let Some(location) = location {
        let resolved = final_url.join(&location).map_err(|_| "认证服务器返回了无效的 API 地址。")?;
        if resolved.scheme() != "https" || resolved.host_str().is_none() {
            return Err("认证服务器返回了不安全的 API 地址。".into());
        }
        resolved
    } else {
        final_url.clone()
    };
    let (root, body) = if resolved_root != final_url {
        let response = client.get(resolved_root.clone()).send().map_err(|_| "无法读取 Yggdrasil API 信息。")?;
        if response.url().scheme() != "https" {
            return Err("Yggdrasil 认证服务器跳转到了不安全的地址。".into());
        }
        let api_url = response.url().clone();
        let bytes = response.bytes().map_err(|_| "无法读取认证服务器信息。")?;
        if bytes.len() > 1024 * 1024 { return Err("认证服务器信息过大。".into()); }
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (api_url, body)
    } else {
        let bytes = response.bytes().map_err(|_| "无法读取认证服务器信息。")?;
        if bytes.len() > 1024 * 1024 { return Err("认证服务器信息过大。".into()); }
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (final_url, body)
    };
    if !body.is_object() {
        return Err("该网站没有提供有效的 Yggdrasil API 信息。".into());
    }
    let mut root = root;
    if !root.path().ends_with('/') {
        root.set_path(&format!("{}/", root.path().trim_end_matches('/')));
    }
    Ok((root, body))
}
fn ygg_authenticate(api_root: &str, username: &str, password: &str) -> R<YggdrasilSession> {
    if username.trim().is_empty() || username.len() > 256 || password.is_empty() || password.len() > 1024 {
        return Err("请输入有效的账户名和密码。".into());
    }
    let root = ygg_url(api_root)?;
    let (root, metadata) = ygg_metadata(&root)?;
    let mut base = root.clone();
    if !base.path().ends_with('/') {
        base.set_path(&format!("{}/", base.path()));
    }
    let endpoint = base.join("authserver/authenticate").map_err(|_| "Yggdrasil 登录地址无效。")?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(25))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("XUMOB-Minecraft-Launcher/0.7.0")
        .build()
        .map_err(|_| "无法连接 Yggdrasil 认证服务器。")?;
    let requested_client_token = format!("xumob-{}-{}", std::process::id(), Utc::now().timestamp_nanos_opt().unwrap_or_default());
    let response = client.post(endpoint)
        .header("Accept", "application/json")
        .json(&json!({"username":username,"password":password,"clientToken":requested_client_token,"requestUser":true,"agent":{"name":"Minecraft","version":1}}))
        .send()
        .map_err(|_| "无法连接 Yggdrasil 认证服务器，请检查网络后重试。")?;
    let status = response.status();
    let bytes = response.bytes().map_err(|_| "无法读取 Yggdrasil 登录结果。")?;
    if bytes.len() > 256 * 1024 {
        return Err("认证服务器返回的信息过大。".into());
    }
    if !status.is_success() {
        return Err(if status.as_u16() == 401 || status.as_u16() == 403 {
            "账户名或密码错误，请检查后重试。".into()
        } else {
            format!("Yggdrasil 认证服务器拒绝了登录（HTTP {}）。", status.as_u16())
        });
    }
    let result: Value = serde_json::from_slice(&bytes).map_err(|_| "认证服务器返回了无效的登录结果。")?;
    let profile_value = result.get("selectedProfile").filter(|v| v.is_object()).cloned()
        .or_else(|| result["availableProfiles"].as_array().and_then(|v| v.first()).cloned())
        .ok_or("此账户没有可用于 Minecraft 的角色。")?;
    let mut profile = yggdrasil_profile(&profile_value)?;
    profile.skin_url = ygg_fetch_skin(&root, &profile_value, &metadata);
    let access_token = token(&result, "accessToken")?;
    let client_token = if field(&result, "clientToken").is_empty() {
        requested_client_token
    } else {
        token(&result, "clientToken")?
    };
    let api_root = root.to_string();
    Ok(YggdrasilSession { api_root, username: username.to_string(), access_token, client_token, profile, metadata })
}
fn ygg_validate(session: &YggdrasilSession) -> R<YggdrasilSession> {
    let root = ygg_url(&session.api_root)?;
    let validate = root.join("authserver/validate").map_err(|_| "Yggdrasil API 地址无效。")?;
    let refresh = root.join("authserver/refresh").map_err(|_| "Yggdrasil API 地址无效。")?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build().map_err(|_| "无法连接 Yggdrasil 认证服务器。")?;
    let body = json!({"accessToken":session.access_token,"clientToken":session.client_token});
    if client.post(validate).json(&body).send().is_ok_and(|response| response.status().as_u16() == 204) {
        return Ok(session.clone());
    }
    let response = client.post(refresh).json(&json!({"accessToken":session.access_token,"clientToken":session.client_token,"requestUser":true,"selectedProfile":{"id":session.profile.id,"name":session.profile.name}}))
        .send().map_err(|_| "外置账户凭据已失效，请重新登录。")?;
    if !response.status().is_success() {
        return Err("外置账户凭据已失效，请重新验证账户。".into());
    }
    let bytes = response.bytes().map_err(|_| "无法读取外置账户刷新结果。")?;
    if bytes.len() > 128 * 1024 { return Err("认证服务器返回的信息过大。".into()); }
    let result: Value = serde_json::from_slice(&bytes).map_err(|_| "认证服务器返回了无效的刷新结果。")?;
    let profile_value = result.get("selectedProfile").filter(|value| value.is_object()).cloned();
    let mut profile = profile_value.as_ref().map(yggdrasil_profile).transpose()?.unwrap_or_else(|| session.profile.clone());
    if profile.skin_url.is_empty() {
        profile.skin_url = profile_value.as_ref()
            .map(|value| ygg_fetch_skin(&root, value, &session.metadata))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| session.profile.skin_url.clone());
    }
    Ok(YggdrasilSession {
        api_root: session.api_root.clone(), username: session.username.clone(),
        access_token: token(&result, "accessToken")?,
        client_token: if field(&result, "clientToken").is_empty() { session.client_token.clone() } else { token(&result, "clientToken")? },
        profile, metadata: session.metadata.clone(),
    })
}
impl AuthManager {
    pub fn new(directory: PathBuf) -> R<Arc<Self>> {
        Self::with_transport(directory, Arc::new(Network))
    }
    fn with_transport(directory: PathBuf, http: Arc<dyn Transport>) -> R<Arc<Self>> {
        fs::create_dir_all(&directory).map_err(|_| "无法创建账户数据目录。")?;
        let runtime_configured = std::env::var("XUMOB_MICROSOFT_CLIENT_ID").unwrap_or_default();
        let build_configured = option_env!("XUMOB_BUILD_MICROSOFT_CLIENT_ID").unwrap_or_default();
        let mut config = fs::read(directory.join("account.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Config>(&b).ok())
            .unwrap_or(Config {
                mode: "offline".into(),
                client_id: String::new(),
                profile: None,
                accounts: Vec::new(),
                active_account_id: String::new(),
            });
        // Prefer the build-time value so an older local account.json cannot
        // override the identifier selected for this build.
        if valid_client(build_configured.trim()) {
            config.client_id = build_configured.trim().to_lowercase();
        } else if valid_client(runtime_configured.trim()) {
            config.client_id = runtime_configured.trim().to_lowercase();
        }
        if !valid_client(&config.client_id) {
            config.client_id = String::new();
        }
        if !matches!(config.mode.as_str(), "offline" | "microsoft" | "littleskin" | "yggdrasil") {
            config.mode = "offline".into();
        }
        let vault = directory.join("account-session.bin");
        let legacy_session = fs::read(&vault)
            .ok()
            .filter(|b| b.len() <= 131072)
            .and_then(|bytes| unprotect(&bytes).ok())
            .and_then(|bytes| serde_json::from_slice::<Session>(&bytes).ok())
            .filter(|session| {
                session.client_id == config.client_id
                    && (!session.refresh_token.is_empty() || !session.broker_account_id.is_empty())
            });
        let mut credentials = fs::read(directory.join("account-accounts.bin"))
            .ok()
            .and_then(|bytes| unprotect(&bytes).ok())
            .and_then(|bytes| serde_json::from_slice::<HashMap<String, Credential>>(&bytes).ok())
            .unwrap_or_default();
        let legacy_id = config.profile.as_ref().map(|profile| account_id("microsoft", &profile.id));
        if let (Some(id), Some(session)) = (legacy_id.as_ref(), legacy_session.as_ref()) {
            credentials.insert(id.clone(), Credential::Microsoft(session.clone()));
            if !config.accounts.iter().any(|account| &account.id == id) {
                config.accounts.push(AccountRecord {
                    id: id.clone(),
                    mode: "microsoft".into(),
                    name: session.profile.name.clone(),
                    username: session.profile.name.clone(),
                    uuid: session.profile.id.clone(),
                    api_root: String::new(),
                    skin_url: String::new(),
                });
            }
            if config.active_account_id.is_empty() {
                config.active_account_id = id.clone();
            }
        }
        let active_microsoft = config
            .accounts
            .iter()
            .find(|account| account.id == config.active_account_id && account.mode == "microsoft")
            .and_then(|account| match credentials.get(&account.id) {
                Some(Credential::Microsoft(session)) => Some(session.clone()),
                _ => None,
            });
        let session = if config.mode == "microsoft" {
            active_microsoft.or(legacy_session)
        } else {
            None
        };
        let needs_login = config.mode == "microsoft" && config.profile.is_some() && session.is_none();
        write_atomic(
            &directory.join("account.json"),
            &serde_json::to_vec(&config).map_err(|_| "无法保存账户配置。")?,
        )?;
        Ok(Arc::new(Self {
            directory,
            state: Mutex::new(State {
                config,
                session,
                pending: None,
                generation: 0,
                needs_login,
                credentials,
            }),
            operation: Mutex::new(()),
            http,
        }))
    }
    fn operation(&self) -> R<MutexGuard<'_, ()>> {
        self.operation
            .try_lock()
            .map_err(|_| "账户正在认证，请稍后重试。".into())
    }
    fn save_config(&self, state: &State) -> R<()> {
        write_atomic(
            &self.directory.join("account.json"),
            &serde_json::to_vec(&state.config).map_err(|_| "无法保存账户配置。")?,
        )
    }
    fn save_session(&self, session: &Session) -> R<()> {
        let bytes = serde_json::to_vec(session).map_err(|_| "无法保存登录凭据。")?;
        write_atomic(
            &self.directory.join("account-session.bin"),
            &protect(&bytes)?,
        )
    }
    fn save_credentials(&self, state: &State) -> R<()> {
        let bytes = serde_json::to_vec(&state.credentials).map_err(|_| "无法保存登录凭据。")?;
        write_atomic(
            &self.directory.join("account-accounts.bin"),
            &protect(&bytes)?,
        )
    }
    pub fn summary(&self) -> Value {
        let state = self.state.lock().unwrap();
        let accounts = state.config.accounts.iter().map(|account| {
            let signed_in = account.mode == "offline" || state.credentials.contains_key(&account.id);
            json!({"id":account.id,"mode":account.mode,"name":account.name,"username":account.username,"uuid":account.uuid,"apiRoot":account.api_root,"skinUrl":account.skin_url,"signedIn":signed_in,"active":account.id == state.config.active_account_id})
        }).collect::<Vec<_>>();
        let signed_in = if state.config.mode == "microsoft" {
            state.session.is_some() && !state.needs_login
        } else if state.config.mode == "littleskin" || state.config.mode == "yggdrasil" {
            state.credentials.contains_key(&state.config.active_account_id)
        } else {
            false
        };
        json!({"mode":state.config.mode,"configured":valid_client(&state.config.client_id),"profile":state.config.profile,"signedIn":signed_in,"needsLogin":state.needs_login,"pending":state.pending.as_ref().is_some_and(|p|p.expires>Instant::now()),"accounts":accounts,"activeAccountId":state.config.active_account_id})
    }
    pub fn refresh_account_skins(&self) -> R<Value> {
        let _op = self.operation()?;
        let mut state = self.state.lock().unwrap();
        let ids = state.config.accounts.iter()
            .filter(|account| matches!(account.mode.as_str(), "littleskin" | "yggdrasil") && account.skin_url.is_empty())
            .map(|account| account.id.clone()).collect::<Vec<_>>();
        let mut changed = false;
        for id in ids {
            let Some(Credential::Yggdrasil(mut session)) = state.credentials.get(&id).cloned() else { continue; };
            let Ok(root) = ygg_url(&session.api_root) else { continue; };
            let profile = json!({"id":session.profile.id,"name":session.profile.name});
            let skin_url = ygg_fetch_skin(&root, &profile, &session.metadata);
            if skin_url.is_empty() { continue; }
            session.profile.skin_url = skin_url.clone();
            state.credentials.insert(id.clone(), Credential::Yggdrasil(session.clone()));
            if let Some(account) = state.config.accounts.iter_mut().find(|account| account.id == id) {
                account.skin_url = skin_url;
            }
            if state.config.active_account_id == id { state.config.profile = Some(session.profile.clone()); }
            changed = true;
        }
        if changed {
            self.save_credentials(&state)?;
            self.save_config(&state)?;
        }
        drop(state);
        Ok(self.summary())
    }
    pub fn add_offline_account(&self, name: &str) -> R<Value> {
        let _op = self.operation()?;
        let name = name.trim();
        if !valid_username(name) {
            return Err("离线用户名需要为 3–16 位字母、数字或下划线。".into());
        }
        let id = account_id("offline", &name.to_ascii_lowercase());
        let mut state = self.state.lock().unwrap();
        if !state.config.accounts.iter().any(|account| account.id == id) {
            state.config.accounts.push(AccountRecord {
                id: id.clone(), mode: "offline".into(), name: name.into(),
                username: name.into(), uuid: crate::core::offline_uuid(name), api_root: String::new(),
                skin_url: String::new(),
            });
        }
        state.config.active_account_id = id;
        state.config.mode = "offline".into();
        state.config.profile = None;
        state.session = None;
        state.needs_login = false;
        self.save_config(&state)?;
        drop(state);
        Ok(self.summary())
    }
    pub fn add_yggdrasil_account(
        &self,
        mode: &str,
        api_root: &str,
        username: &str,
        password: &str,
    ) -> R<Value> {
        let _op = self.operation()?;
        if !matches!(mode, "littleskin" | "yggdrasil") {
            return Err("外置账户类型无效。".into());
        }
        let endpoint = if mode == "littleskin" {
            "https://littleskin.cn/api/yggdrasil/"
        } else {
            api_root
        };
        let session = ygg_authenticate(endpoint, username.trim(), password)?;
        let id = account_id(mode, &format!("{}:{}", session.api_root, session.profile.id));
        let mut state = self.state.lock().unwrap();
        let record = AccountRecord {
            id: id.clone(), mode: mode.into(), name: session.profile.name.clone(),
            username: username.trim().into(), uuid: session.profile.id.clone(),
            api_root: session.api_root.clone(),
            skin_url: session.profile.skin_url.clone(),
        };
        if let Some(existing) = state.config.accounts.iter_mut().find(|account| account.id == id) {
            *existing = record;
        } else {
            state.config.accounts.push(record);
        }
        state.credentials.insert(id.clone(), Credential::Yggdrasil(session.clone()));
        state.config.active_account_id = id;
        state.config.mode = mode.into();
        state.config.profile = Some(session.profile);
        state.session = None;
        state.needs_login = false;
        self.save_credentials(&state)?;
        self.save_config(&state)?;
        drop(state);
        Ok(self.summary())
    }
    pub fn select_account(&self, id: &str) -> R<Value> {
        let _op = self.operation()?;
        let mut state = self.state.lock().unwrap();
        let record = state.config.accounts.iter().find(|account| account.id == id)
            .cloned().ok_or("找不到这个账户。")?;
        match record.mode.as_str() {
            "offline" => {
                state.session = None;
                state.config.profile = None;
                state.needs_login = false;
            }
            "microsoft" => {
                let session = match state.credentials.get(id) {
                    Some(Credential::Microsoft(session)) => session.clone(),
                    _ => return Err("此微软账户需要重新登录。".into()),
                };
                state.config.profile = Some(session.profile.clone());
                state.session = Some(session);
                state.needs_login = false;
            }
            "littleskin" | "yggdrasil" => {
                match state.credentials.get(id) {
                    Some(Credential::Yggdrasil(_)) => {}
                    _ => return Err("此外置账户需要重新验证。".into()),
                }
                state.config.profile = Some(Profile { id: record.uuid, name: record.name, xuid: String::new(), skin_url: record.skin_url });
                state.session = None;
                state.needs_login = false;
            }
            _ => return Err("账户类型无效。".into()),
        }
        state.config.mode = record.mode;
        state.config.active_account_id = id.into();
        self.save_config(&state)?;
        if let Some(session) = state.session.clone() {
            self.save_session(&session)?;
        }
        drop(state);
        Ok(self.summary())
    }
    pub fn remove_account(&self, id: &str) -> R<Value> {
        let _op = self.operation()?;
        let mut state = self.state.lock().unwrap();
        let was_active = state.config.active_account_id == id;
        state.config.accounts.retain(|account| account.id != id);
        state.credentials.remove(id);
        if was_active {
            state.config.active_account_id.clear();
            state.config.profile = None;
            state.session = None;
            state.needs_login = false;
            state.config.mode = "offline".into();
            if let Some(fallback) = state.config.accounts.first().cloned() {
                state.config.mode = fallback.mode.clone();
                state.config.active_account_id = fallback.id.clone();
                let fallback_credential = state.credentials.get(&fallback.id).cloned();
                match fallback_credential {
                    Some(Credential::Microsoft(session)) if fallback.mode == "microsoft" => {
                        state.config.profile = Some(session.profile.clone());
                        state.session = Some(session);
                    }
                    Some(Credential::Yggdrasil(session)) if fallback.mode != "offline" => {
                        state.config.profile = Some(session.profile.clone());
                    }
                    _ => {}
                }
            }
        }
        self.save_credentials(&state)?;
        self.save_config(&state)?;
        if was_active && state.session.is_none() {
            let legacy = self.directory.join("account-session.bin");
            if legacy.exists() {
                fs::remove_file(legacy).map_err(|_| "无法清除旧的登录凭据。")?;
            }
        }
        drop(state);
        Ok(self.summary())
    }
    pub fn configure(&self, value: &str) -> R<Value> {
        let _op = self.operation()?;
        let value = value.trim().to_lowercase();
        if !valid_client(&value) {
            return Err("Client ID 应为微软应用的 GUID，不需要 Client Secret。".into());
        }
        let mut state = self.state.lock().unwrap();
        if state.pending.is_some() {
            return Err("请先取消当前登录，再更改应用配置。".into());
        }
        if state.config.client_id != value {
            state.config.client_id = value;
            state.config.profile = None;
            state.config.mode = "offline".into();
            state.session = None;
            state.needs_login = false;
            let old_ids = state.config.accounts.iter().filter(|account| account.mode == "microsoft").map(|account| account.id.clone()).collect::<Vec<_>>();
            state.config.accounts.retain(|account| account.mode != "microsoft");
            for id in old_ids { state.credentials.remove(&id); }
            state.config.active_account_id.clear();
            self.save_credentials(&state)?;
            let file = self.directory.join("account-session.bin");
            if file.exists() {
                fs::remove_file(file).map_err(|_| "无法移除旧登录凭据。")?;
            }
            self.save_config(&state)?;
        }
        drop(state);
        Ok(self.summary())
    }
    pub fn microsoft_client_id(&self) -> R<String> {
        let state = self.state.lock().unwrap();
        if !valid_client(&state.config.client_id) {
            return Err("请先配置微软应用 Client ID。".into());
        }
        Ok(state.config.client_id.clone())
    }
    pub fn select(&self, mode: &str) -> R<Value> {
        let _op = self.operation()?;
        let mut state = self.state.lock().unwrap();
        if state.pending.is_some() {
            return Err("请先完成或取消微软登录。".into());
        }
        match mode {
            "offline" => {}
            "microsoft" if state.session.is_some() && !state.needs_login => {}
            "microsoft" => return Err("请先完成微软正版登录。".into()),
            _ => return Err("账户类型无效。".into()),
        }
        state.config.mode = mode.into();
        if mode == "microsoft" {
            if let Some(session) = state.session.clone() {
                let id = account_id("microsoft", &session.profile.id);
                state.config.active_account_id = id.clone();
                state.config.profile = Some(session.profile.clone());
                state.credentials.insert(id.clone(), Credential::Microsoft(session.clone()));
                if !state.config.accounts.iter().any(|account| account.id == id) {
                    state.config.accounts.push(AccountRecord { id, mode: "microsoft".into(), name: session.profile.name.clone(), username: session.profile.name.clone(), uuid: session.profile.id.clone(), api_root: String::new(), skin_url: String::new() });
                }
                self.save_credentials(&state)?;
            }
        } else {
            state.config.profile = None;
            state.config.active_account_id = state.config.accounts.iter().find(|account| account.mode == "offline").map(|account| account.id.clone()).unwrap_or_default();
        }
        self.save_config(&state)?;
        drop(state);
        Ok(self.summary())
    }
    pub fn logout(&self) -> R<Value> {
        self.cancel();
        let _op = self.operation()?;
        let mut state = self.state.lock().unwrap();
        if let Some(session) = state.session.as_ref() {
            let id = account_id("microsoft", &session.profile.id);
            state.config.accounts.retain(|account| account.id != id);
            state.credentials.remove(&id);
        }
        let vault = self.directory.join("account-session.bin");
        if vault.exists() { fs::remove_file(vault).map_err(|_| "无法清除登录凭据，请检查数据目录权限。")?; }
        state.config.profile = None;
        state.config.mode = "offline".into();
        state.session = None;
        state.needs_login = false;
        state.config.active_account_id.clear();
        self.save_credentials(&state)?;
        self.save_config(&state)?;
        drop(state);
        Ok(self.summary())
    }
    pub fn cancel(&self) {
        let mut state = self.state.lock().unwrap();
        state.generation = state.generation.wrapping_add(1);
        state.pending = None;
    }
    fn current(&self, id: u64) -> R<()> {
        let state = self.state.lock().unwrap();
        if state.generation != id
            || state
                .pending
                .as_ref()
                .is_none_or(|p| p.id != id || p.expires <= Instant::now())
        {
            Err("登录已取消或代码已过期，请重新登录。".into())
        } else {
            Ok(())
        }
    }
    pub fn start(&self) -> R<Value> {
        let _op = self.operation()?;
        let (id, client_id) = {
            let mut state = self.state.lock().unwrap();
            if !valid_client(&state.config.client_id) {
                return Err("请先配置微软应用 Client ID。".into());
            }
            state.generation = state.generation.wrapping_add(1);
            state.pending = None;
            (state.generation, state.config.client_id.clone())
        };
        let data = response(
            self.http.send(
                Endpoint::Device,
                Body::Form(vec![
                    ("client_id", client_id.clone()),
                    ("scope", SCOPE.into()),
                    ("mkt", "zh-CN".into()),
                ]),
                None,
            )?,
            Endpoint::Device,
        )?;
        let device_code = token(&data, "device_code")?;
        let user_code = token(&data, "user_code")?;
        if user_code.len() > 32 {
            return Err("登录代码格式无效。".into());
        }
        let uri = safe_uri(field(&data, "verification_uri"))?;
        let expires = data["expires_in"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= 1800)
            .ok_or("登录服务未提供有效期限。")?;
        let interval = data["interval"].as_u64().unwrap_or(5).clamp(5, 60);
        let expires_at = epoch() + expires as i64;
        let mut state = self.state.lock().unwrap();
        if state.generation != id {
            return Err("登录已取消。".into());
        }
        state.pending = Some(Pending {
            id,
            device_code,
            uri: uri.clone(),
            expires: Instant::now() + Duration::from_secs(expires),
            interval,
            next: Instant::now() + Duration::from_secs(interval),
            client_id,
        });
        Ok(
            json!({"sessionId":id.to_string(),"userCode":user_code,"verificationUri":uri,"expiresAt":expires_at,"interval":interval}),
        )
    }
    pub fn browser_uri(&self, session_id: &str) -> R<String> {
        let id = session_id.parse::<u64>().map_err(|_| "登录会话无效。")?;
        let state = self.state.lock().unwrap();
        let pending = state
            .pending
            .as_ref()
            .filter(|p| p.id == id && state.generation == id && p.expires > Instant::now())
            .ok_or("登录已取消或代码已过期。")?;
        Ok(pending.uri.clone())
    }
    pub fn poll(&self, session_id: &str) -> R<Value> {
        let _op = self.operation()?;
        let id = session_id.parse::<u64>().map_err(|_| "登录会话无效。")?;
        self.current(id)?;
        let pending = {
            let mut state = self.state.lock().unwrap();
            let p = state
                .pending
                .as_mut()
                .filter(|p| p.id == id && p.expires > Instant::now())
                .ok_or("登录已取消或代码已过期。")?;
            if Instant::now() < p.next {
                return Ok(
                    json!({"status":"pending","retryAfter":p.next.saturating_duration_since(Instant::now()).as_secs().max(1)}),
                );
            }
            p.next = Instant::now() + Duration::from_secs(p.interval);
            p.clone()
        };
        let reply = self.http.send(
            Endpoint::Microsoft,
            Body::Form(vec![
                (
                    "grant_type",
                    "urn:ietf:params:oauth:grant-type:device_code".into(),
                ),
                ("client_id", pending.client_id.clone()),
                ("device_code", pending.device_code.clone()),
            ]),
            None,
        )?;
        self.current(id)?;
        let code = field(&reply.data, "error");
        if code == "authorization_pending" || code == "slow_down" {
            let mut state = self.state.lock().unwrap();
            if let Some(p) = state.pending.as_mut() {
                if code == "slow_down" {
                    p.interval = (p.interval + 5).min(60);
                }
                p.next = Instant::now() + Duration::from_secs(p.interval);
                return Ok(json!({"status":"pending","retryAfter":p.interval}));
            }
        }
        let data = match response(reply, Endpoint::Microsoft) {
            Ok(data) => data,
            Err(error) => {
                self.cancel();
                return Err(error);
            }
        };
        let authentication = (|| {
            let msa = token(&data, "access_token")?;
            let refresh = token(&data, "refresh_token")?;
            self.exchange(&pending.client_id, msa, refresh, Some(id))
        })();
        let session = match authentication {
            Ok(session) => session,
            Err(error) => {
                self.cancel();
                return Err(error);
            }
        };
        self.current(id)?;
        {
            let mut state = self.state.lock().unwrap();
            if state.generation != id || state.pending.is_none() {
                return Err("登录已取消。".into());
            }
            self.save_session(&session)?;
            let id = account_id("microsoft", &session.profile.id);
            state.credentials.insert(id.clone(), Credential::Microsoft(session.clone()));
            if let Some(account) = state.config.accounts.iter_mut().find(|account| account.id == id) {
                account.name = session.profile.name.clone();
                account.username = session.profile.name.clone();
                account.uuid = session.profile.id.clone();
            } else {
                state.config.accounts.push(AccountRecord { id: id.clone(), mode: "microsoft".into(), name: session.profile.name.clone(), username: session.profile.name.clone(), uuid: session.profile.id.clone(), api_root: String::new(), skin_url: String::new() });
            }
            state.config.active_account_id = id;
            state.config.profile = Some(session.profile.clone());
            state.config.mode = "microsoft".into();
            state.session = Some(session);
            state.needs_login = false;
            state.pending = None;
            self.save_credentials(&state)?;
            self.save_config(&state)?;
        }
        Ok(json!({"status":"complete","account":self.summary()}))
    }
    /// Completes an interactive Windows Web Account Manager login. WAM owns
    /// the Microsoft refresh-token cache; we persist only its account ID and
    /// the encrypted Minecraft session, then ask WAM for a fresh MSA token at
    /// game launch.
    pub fn complete_broker_login(&self, msa: String, broker_account_id: String) -> R<Value> {
        let _op = self.operation()?;
        if broker_account_id.is_empty() || broker_account_id.len() > 512 {
            return Err("Windows 未返回有效的 Microsoft 账户信息。".into());
        }
        let client_id = {
            let state = self.state.lock().unwrap();
            if !valid_client(&state.config.client_id) {
                return Err("请先配置微软应用 Client ID。".into());
            }
            state.config.client_id.clone()
        };
        let mut session = self.exchange(&client_id, msa, String::new(), None)?;
        session.broker_account_id = broker_account_id;
        self.store_microsoft_session(session)?;
        Ok(self.summary())
    }
    pub fn broker_refresh_context(&self) -> Option<(String, String)> {
        let state = self.state.lock().ok()?;
        if state.config.mode != "microsoft" || state.needs_login {
            return None;
        }
        let session = state.session.as_ref()?;
        if session.broker_account_id.is_empty() {
            return None;
        }
        Some((session.client_id.clone(), session.broker_account_id.clone()))
    }
    pub fn refresh_broker_session(&self, msa: String) -> R<()> {
        let _op = self.operation()?;
        let cached = {
            let state = self.state.lock().unwrap();
            if state.config.mode != "microsoft" || state.needs_login {
                return Err("请先完成微软正版登录。".into());
            }
            state.session.clone().filter(|s| !s.broker_account_id.is_empty())
                .ok_or("当前账户不是 Windows 系统账户登录。")?
        };
        let mut session = self.exchange(&cached.client_id, msa, String::new(), None)?;
        if session.profile.id != cached.profile.id {
            return Err("Windows 返回的 Microsoft 账户与当前选择不一致，请重新登录。".into());
        }
        session.broker_account_id = cached.broker_account_id;
        self.store_microsoft_session(session)
    }
    fn store_microsoft_session(&self, session: Session) -> R<()> {
        let mut state = self.state.lock().unwrap();
        self.save_session(&session)?;
        let id = account_id("microsoft", &session.profile.id);
        state.credentials.insert(id.clone(), Credential::Microsoft(session.clone()));
        if let Some(account) = state.config.accounts.iter_mut().find(|account| account.id == id) {
            account.name = session.profile.name.clone();
            account.username = session.profile.name.clone();
            account.uuid = session.profile.id.clone();
        } else {
            state.config.accounts.push(AccountRecord { id: id.clone(), mode: "microsoft".into(), name: session.profile.name.clone(), username: session.profile.name.clone(), uuid: session.profile.id.clone(), api_root: String::new(), skin_url: String::new() });
        }
        state.config.active_account_id = id;
        state.config.profile = Some(session.profile.clone());
        state.config.mode = "microsoft".into();
        state.session = Some(session);
        state.needs_login = false;
        state.pending = None;
        self.save_credentials(&state)?;
        self.save_config(&state)
    }
    fn request(
        &self,
        endpoint: Endpoint,
        body: Body,
        bearer: Option<&str>,
        pending: Option<u64>,
    ) -> R<Value> {
        if let Some(id) = pending {
            self.current(id)?;
        }
        let data = response(self.http.send(endpoint, body, bearer)?, endpoint)?;
        if let Some(id) = pending {
            self.current(id)?;
        }
        Ok(data)
    }
    fn exchange(
        &self,
        client_id: &str,
        msa: String,
        refresh: String,
        pending: Option<u64>,
    ) -> R<Session> {
        let xbox=self.request(Endpoint::Xbox,Body::Json(json!({"Properties":{"AuthMethod":"RPS","SiteName":"user.auth.xboxlive.com","RpsTicket":format!("d={msa}")},"RelyingParty":"http://auth.xboxlive.com","TokenType":"JWT"})),None,pending)?;
        let user_token = token(&xbox, "Token")?;
        let user_hash = token(&xbox["DisplayClaims"]["xui"][0], "uhs")?;
        let xsts=self.request(Endpoint::Xsts,Body::Json(json!({"Properties":{"SandboxId":"RETAIL","UserTokens":[user_token]},"RelyingParty":"rp://api.minecraftservices.com/","TokenType":"JWT"})),None,pending)?;
        if field(&xsts["DisplayClaims"]["xui"][0], "uhs") != user_hash {
            return Err("Xbox 认证身份不一致，请重新登录。".into());
        }
        let xsts_token = token(&xsts, "Token")?;
        let mc = self.request(
            Endpoint::Minecraft,
            Body::Json(json!({"identityToken":format!("XBL3.0 x={user_hash};{xsts_token}")})),
            None,
            pending,
        )?;
        let access = token(&mc, "access_token")?;
        let seconds = mc["expires_in"]
            .as_i64()
            .filter(|n| *n > 0 && *n <= 86400)
            .ok_or("Minecraft 服务未返回有效凭据期限。")?;
        let xuid = field(&xsts["DisplayClaims"]["xui"][0], "xid");
        let xuid = if xuid.is_empty() {
            field(&mc, "username")
        } else {
            xuid
        };
        let profile = self.validate_game(&access, xuid, pending)?;
        Ok(Session {
            client_id: client_id.into(),
            refresh_token: refresh,
            access_token: access,
            expires_at: epoch() + seconds,
            profile,
            broker_account_id: String::new(),
        })
    }
    fn validate_game(&self, access: &str, xuid: &str, pending: Option<u64>) -> R<Profile> {
        let entitlement = self.request(Endpoint::License, Body::Empty, Some(access), pending)?;
        if !entitlement["items"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| matches!(field(item, "name"), "game_minecraft" | "product_minecraft"))
        }) {
            return Err("此账户没有可用的 Minecraft Java 版授权，或订阅已失效。".into());
        }
        profile(
            &self.request(Endpoint::Profile, Body::Empty, Some(access), pending)?,
            xuid,
        )
    }
    pub fn preview(&self) -> R<Option<Identity>> {
        let state = self.state.lock().unwrap();
        if matches!(state.config.mode.as_str(), "littleskin" | "yggdrasil") {
            let credential = state.credentials.get(&state.config.active_account_id)
                .ok_or("此外置账户需要重新验证。")?;
            return match credential {
                Credential::Yggdrasil(session) => Ok(Some(ygg_identity(session)?)),
                _ => Err("此外置账户需要重新验证。".into()),
            };
        }
        if state.config.mode != "microsoft" {
            return Ok(None);
        }
        let profile = state
            .config
            .profile
            .as_ref()
            .ok_or("请先完成微软正版登录。")?;
        Ok(Some(Identity {
            name: profile.name.clone(),
            uuid: profile.id.clone(),
            access_token: "0".into(),
            client_id: state.config.client_id.clone(),
            xuid: profile.xuid.clone(),
            user_type: "msa".into(),
            api_root: None,
            prefetched_metadata: None,
        }))
    }
    pub fn identity_for_launch(&self) -> R<Option<Identity>> {
        let _op = self.operation()?;
        let external = {
            let state = self.state.lock().unwrap();
            if state.pending.is_some() {
                return Err("请先完成或取消当前登录，再启动游戏。".into());
            }
            if matches!(state.config.mode.as_str(), "littleskin" | "yggdrasil") {
                match state.credentials.get(&state.config.active_account_id) {
                    Some(Credential::Yggdrasil(session)) => Some(session.clone()),
                    _ => return Err("此外置账户需要重新验证。".into()),
                }
            } else {
                None
            }
        };
        if let Some(session) = external {
            let session = ygg_validate(&session)?;
            let mut state = self.state.lock().unwrap();
            let id = state.config.active_account_id.clone();
            state.credentials.insert(id, Credential::Yggdrasil(session.clone()));
            state.config.profile = Some(session.profile.clone());
            self.save_credentials(&state)?;
            self.save_config(&state)?;
            return Ok(Some(ygg_identity(&session)?));
        }
        let cached = {
            let state = self.state.lock().unwrap();
            if state.pending.is_some() {
                return Err("请先完成或取消当前登录，再启动游戏。".into());
            }
            if state.config.mode != "microsoft" {
                return Ok(None);
            }
            if state.needs_login {
                return Err("保存的正版登录已失效，请重新登录。".into());
            }
            state.session.clone().ok_or("请先完成微软正版登录。")?
        };
        let mut session = cached.clone();
        let outcome = (|| {
            if session.expires_at <= epoch() + 120 {
                let reply = self.http.send(
                    Endpoint::Microsoft,
                    Body::Form(vec![
                        ("client_id", session.client_id.clone()),
                        ("grant_type", "refresh_token".into()),
                        ("refresh_token", session.refresh_token.clone()),
                        ("scope", SCOPE.into()),
                    ]),
                    None,
                )?;
                if matches!(
                    field(&reply.data, "error"),
                    "invalid_grant" | "interaction_required"
                ) {
                    self.state.lock().unwrap().needs_login = true;
                }
                let data = response(reply, Endpoint::Microsoft)?;
                let msa = token(&data, "access_token")?;
                if !field(&data, "refresh_token").is_empty() {
                    session.refresh_token = token(&data, "refresh_token")?;
                    self.save_session(&session)?;
                    self.state.lock().unwrap().session = Some(session.clone());
                }
                session =
                    self.exchange(&session.client_id, msa, session.refresh_token.clone(), None)?;
            } else {
                session.profile =
                    self.validate_game(&session.access_token, &session.profile.xuid, None)?;
            }
            if session.profile.id != cached.profile.id {
                return Err("正版角色身份发生变化，请重新登录。".into());
            }
            self.save_session(&session)?;
            let mut state = self.state.lock().unwrap();
            state.config.profile = Some(session.profile.clone());
            state.session = Some(session.clone());
            self.save_config(&state)?;
            Ok(Some(Identity {
                name: session.profile.name,
                uuid: session.profile.id,
                access_token: session.access_token,
                client_id: session.client_id,
                xuid: session.profile.xuid,
                user_type: "msa".into(),
                api_root: None,
                prefetched_metadata: None,
            }))
        })();
        outcome
    }
    pub fn redact(&self, line: &str) -> String {
        let state = self.state.lock().unwrap();
        let mut text = line.to_string();
        if !state.config.client_id.is_empty() {
            text = text.replace(&state.config.client_id, "[应用标识已隐藏]");
        }
        if let Some(session) = &state.session {
            for value in [&session.access_token, &session.refresh_token] {
                if !value.is_empty() {
                    text = text.replace(value, "[已隐藏]");
                }
            }
        }
        for credential in state.credentials.values() {
            match credential {
                Credential::Microsoft(session) => {
                    for value in [&session.access_token, &session.refresh_token] {
                        if !value.is_empty() { text = text.replace(value, "[已隐藏]"); }
                    }
                }
                Credential::Yggdrasil(session) => {
                    if !session.access_token.is_empty() { text = text.replace(&session.access_token, "[已隐藏]"); }
                    if !session.client_token.is_empty() { text = text.replace(&session.client_token, "[已隐藏]"); }
                }
            }
        }
        text
    }
}

#[cfg(windows)]
fn dpapi(bytes: &[u8], encrypt: bool) -> R<Vec<u8>> {
    use std::ffi::c_void;
    #[repr(C)]
    struct Blob {
        size: u32,
        data: *mut u8,
    }
    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            input: *const Blob,
            description: *const u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
        fn CryptUnprotectData(
            input: *const Blob,
            description: *mut *mut u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }
    if bytes.len() > 131072 {
        return Err("登录凭据格式无效。".into());
    }
    let input = Blob {
        size: bytes.len() as u32,
        data: bytes.as_ptr() as *mut u8,
    };
    let mut output = Blob {
        size: 0,
        data: std::ptr::null_mut(),
    };
    unsafe {
        let ok = if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                1,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                1,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(if encrypt {
                "Windows 无法加密保存登录凭据。"
            } else {
                "无法解密此 Windows 用户的登录凭据，请重新登录。"
            }
            .into());
        }
        let result = std::slice::from_raw_parts(output.data, output.size as usize).to_vec();
        std::ptr::write_bytes(output.data, 0, output.size as usize);
        LocalFree(output.data.cast());
        Ok(result)
    }
}
pub(crate) fn protect(bytes: &[u8]) -> R<Vec<u8>> {
    #[cfg(windows)]
    {
        dpapi(bytes, true)
    }
    #[cfg(not(windows))]
    {
        let _ = bytes;
        Err("当前凭据存储仅支持 Windows。".into())
    }
}
pub(crate) fn unprotect(bytes: &[u8]) -> R<Vec<u8>> {
    #[cfg(windows)]
    {
        dpapi(bytes, false)
    }
    #[cfg(not(windows))]
    {
        let _ = bytes;
        Err("当前凭据存储仅支持 Windows。".into())
    }
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
