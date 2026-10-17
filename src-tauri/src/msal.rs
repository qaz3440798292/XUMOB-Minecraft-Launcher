use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};
use windows::Win32::Foundation::HWND;

/// Acquires an Xbox-scoped Microsoft token through MSAL.NET's Windows broker.
/// MSAL/WAM owns the refresh token; the launcher stores only the opaque
/// broker account reference and its encrypted Minecraft session.
pub fn request_microsoft_token(
    hwnd: HWND,
    client_id: &str,
    broker_account_id: Option<&str>,
) -> Result<(String, String), String> {
    let helper = helper_path()?;
    let request = json!({
        "action": if broker_account_id.is_some() { "refresh" } else { "login" },
        "clientId": client_id,
        "parentWindow": hwnd.0 as isize as i64,
        "accountId": broker_account_id,
    });
    let mut child = Command::new(&helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|error| format!("无法启动 MSAL 登录组件：{error}"))?;
    child
        .stdin
        .take()
        .ok_or("无法连接 MSAL 登录组件。")?
        .write_all(format!("{request}\n").as_bytes())
        .map_err(|error| format!("无法发送 Microsoft 登录请求：{error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("无法读取 Microsoft 登录结果：{error}"))?;
    let response: Value = serde_json::from_slice(&output.stdout).map_err(|_| {
        let details = String::from_utf8_lossy(&output.stderr);
        let details = details.trim();
        if details.is_empty() {
            "MSAL 登录组件没有返回有效结果，请重新构建启动器。".to_string()
        } else {
            format!("MSAL 登录组件运行失败：{details}")
        }
    })?;
    if !response["success"].as_bool().unwrap_or(false) {
        return Err(response["error"]
            .as_str()
            .unwrap_or("Microsoft 账户授权未完成。")
            .to_string());
    }
    let token = response["accessToken"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or("MSAL 没有返回 Microsoft 访问令牌。")?;
    let account_id = response["accountId"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or("MSAL 没有返回可续期的账户信息。")?;
    Ok((token.to_string(), account_id.to_string()))
}

fn helper_path() -> Result<PathBuf, String> {
    let executable =
        std::env::current_exe().map_err(|error| format!("无法定位启动器程序：{error}"))?;
    let path = executable
        .parent()
        .ok_or("无法定位启动器目录。")?
        .join("msal-broker")
        .join("Xumob.MsalBroker.exe");
    if !path.is_file() {
        return Err("找不到 MSAL 登录组件，请重新构建启动器并保留同目录文件。".into());
    }
    Ok(path)
}
