//! Bounded native downloads. Credentials are attached only to CurseForge's CDN.
use crate::core::R;
use serde_json::Value;
use sha1::Sha1;
use sha2::{Digest, Sha512};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        OnceLock,
    },
    time::Duration,
};
pub fn allowed(value: &str) -> R<reqwest::Url> {
    let u = reqwest::Url::parse(value).map_err(|_| "下载地址无效。")?;
    let h = u.host_str().unwrap_or("");
    let trusted = [
        "piston-meta.mojang.com",
        "piston-data.mojang.com",
        "launchermeta.mojang.com",
        "launcher.mojang.com",
        "resources.download.minecraft.net",
        "libraries.minecraft.net",
        "meta.fabricmc.net",
        "maven.fabricmc.net",
        "meta.quiltmc.org",
        "maven.quiltmc.org",
        "maven.minecraftforge.net",
        "files.minecraftforge.net",
        // OptiFine's official download endpoint.
        "optifine.net",
        "maven.neoforged.net",
        "cdn.modrinth.com",
        "api.modrinth.com",
        "bmclapi2.bangbang93.com",
        "github.com",
        "raw.githubusercontent.com",
        "objects.githubusercontent.com",
        "release-assets.githubusercontent.com",
        "gitlab.com",
        "authlib-injector.yushi.moe",
    ];
    if u.scheme() != "https"
        || u.port().is_some()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.fragment().is_some()
        || !(trusted.contains(&h)
            || h.ends_with(".forgecdn.net")
            || h.ends_with(".bmclapi.com")
            || h.ends_with(".bmclapi.net"))
    {
        return Err(format!("文件源不在受信任下载列表中：{h}"));
    }
    Ok(u)
}
pub(crate) fn is_optifine_mirror_redirect(from: &reqwest::Url, to: &reqwest::Url) -> bool {
    from.host_str() == Some("bmclapi2.bangbang93.com")
        && to.scheme() == "https"
        && to.host_str() == Some("bmclapi1.sakura.exp.dog")
        && to.port().is_none()
        && to.username().is_empty()
        && to.password().is_none()
        && to.fragment().is_none()
}
pub fn client() -> R<reqwest::blocking::Client> {
    static CLIENT: OnceLock<Result<reqwest::blocking::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .user_agent("XUMOB-Minecraft-Launcher/0.7.0 (https://github.com/qaz3440798292/XUMOB-Minecraft-Launcher)")
                .connect_timeout(Duration::from_secs(12))
                .timeout(Duration::from_secs(45))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| "无法创建下载连接。".to_string())
        })
        .clone()
        .map_err(Into::into)
}

pub(crate) fn download_client() -> R<reqwest::blocking::Client> {
    static CLIENT: OnceLock<Result<reqwest::blocking::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .user_agent("XUMOB-Minecraft-Launcher/0.7.0")
                .connect_timeout(Duration::from_secs(12))
                .timeout(Duration::from_secs(900))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| "无法创建下载连接。".to_string())
        })
        .clone()
        .map_err(Into::into)
}

fn streaming_client() -> R<reqwest::Client> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .user_agent("XUMOB-Minecraft-Launcher/0.7.0")
                .connect_timeout(Duration::from_secs(12))
                .read_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(900))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| "无法创建下载连接。".to_string())
        })
        .clone()
        .map_err(Into::into)
}
pub fn response(
    client: &reqwest::blocking::Client,
    url: &str,
    key: Option<&str>,
) -> R<reqwest::blocking::Response> {
    let mut url = allowed(url)?;
    for _ in 0..8 {
        let mut r = client.get(url.clone());
        if url.host_str().unwrap_or("").ends_with(".forgecdn.net") {
            if let Some(k) = key {
                r = r.header("x-api-key", k)
            }
        }
        let res = r.send().map_err(|_| "下载连接失败或超时，请重试。")?;
        if res.status().is_redirection() {
            let next = res
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or("文件源重定向无效。")?;
            url = allowed(url.join(next).map_err(|_| "文件源重定向无效。")?.as_str())?;
            continue;
        }
        if !res.status().is_success() {
            return Err(format!(
                "文件源返回 HTTP {}，请稍后重试。",
                res.status().as_u16()
            ));
        }
        return Ok(res);
    }
    Err("文件源重定向次数过多。".into())
}
/// Fetch a small HTML page using browser-like request headers. This is used to
/// resolve OptiFine's public download page, which contains the actual file URL.
pub fn browser_page(url: &str, referer: &str, limit: u64) -> R<String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .connect_timeout(Duration::from_secs(12))
        .timeout(Duration::from_secs(45))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法创建下载连接。")?;
    let mut url = allowed(url)?;
    for _ in 0..8 {
        let res = client
            .get(url.clone())
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .header("Referer", referer)
            .header("Upgrade-Insecure-Requests", "1")
            .send()
            .map_err(|_| "无法获取 OptiFine 官方下载页面。")?;
        if res.status().is_redirection() {
            let location = res
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or("OptiFine 下载页面重定向无效。")?;
            url = allowed(
                url.join(location)
                    .map_err(|_| "OptiFine 下载页面重定向无效。")?
                    .as_str(),
            )?;
            continue;
        }
        if !res.status().is_success() {
            return Err(format!("OptiFine 官方页面返回 HTTP {}。", res.status().as_u16()).into());
        }
        let mut data = Vec::new();
        res.take(limit + 1)
            .read_to_end(&mut data)
            .map_err(|_| "无法读取 OptiFine 官方下载页面。")?;
        if data.len() as u64 > limit {
            return Err("OptiFine 官方下载页面内容过大。".into());
        }
        return Ok(String::from_utf8_lossy(&data).into_owned());
    }
    Err("OptiFine 官方页面重定向次数过多。".into())
}
pub fn bytes(url: &str, limit: u64) -> R<Vec<u8>> {
    let r = response(&client()?, url, None)?;
    let mut data = Vec::new();
    r.take(limit + 1)
        .read_to_end(&mut data)
        .map_err(|_| "无法读取下载数据。")?;
    if data.len() as u64 > limit {
        return Err("下载数据过大。".into());
    }
    Ok(data)
}
pub fn json(url: &str) -> R<Value> {
    serde_json::from_slice(&bytes(url, 16 * 1024 * 1024)?)
        .map_err(|_| "服务返回了无法读取的数据。".into())
}
pub fn hash(path: &Path, long: bool) -> R<String> {
    let mut f = File::open(path).map_err(|_| "无法读取文件进行校验。")?;
    let (mut a, mut b) = (Sha1::new(), Sha512::new());
    let mut buf = [0; 65536];
    loop {
        let n = f.read(&mut buf).map_err(|_| "校验文件失败。")?;
        if n == 0 {
            break;
        }
        if long {
            b.update(&buf[..n])
        } else {
            a.update(&buf[..n])
        }
    }
    Ok(if long {
        hex::encode(b.finalize())
    } else {
        hex::encode(a.finalize())
    })
}
pub fn stream(
    url: &str,
    out: &mut File,
    size: u64,
    cancel: &AtomicBool,
    key: Option<&str>,
    progress: &mut dyn FnMut(u64),
) -> R<()> {
    stream_inner(url, out, size, cancel, key, progress, false)
}

/// OptiFine's BMCLAPI endpoint can redirect to its HTTPS volunteer download
/// node. Keep that exception local to OptiFine and validate the downloaded JAR
/// before using it; do not add the mirror node to the global host allowlist.
pub fn stream_optifine(
    url: &str,
    out: &mut File,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64),
) -> R<()> {
    stream_inner(url, out, 0, cancel, None, progress, true)
}

fn stream_inner(
    url: &str,
    out: &mut File,
    size: u64,
    cancel: &AtomicBool,
    key: Option<&str>,
    progress: &mut dyn FnMut(u64),
    allow_optifine_node: bool,
) -> R<()> {
    let mut url = allowed(url)?;
    let key = key.map(str::to_owned);
    let mut out = out.try_clone().map_err(|_| "无法写入下载文件。")?;
    let (tx, rx) = std::sync::mpsc::sync_channel(16);
    let task = tauri::async_runtime::spawn(async move {
        let client = streaming_client()?;
        let mut response = None;
        let mut mirror_hash: Option<String> = None;
        for _ in 0..8 {
            let mut req = client.get(url.clone());
            if url.host_str().unwrap_or("").ends_with(".forgecdn.net") {
                if let Some(key) = &key {
                    req = req.header("x-api-key", key)
                }
            }
            let res = req
                .send()
                .await
                .map_err(|_| "无法连接下载源，请重试。".to_string())?;
            if res.status().is_redirection() {
                let location = res
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or("下载地址重定向无效。".to_string())?;
                let next = url
                    .join(location)
                    .map_err(|_| "下载地址无效。".to_string())?;
                match allowed(next.as_str()) {
                    Ok(next) => url = next,
                    Err(error) => {
                        // BMCLAPI's authenticated HTTPS redirect identifies volunteer
                        // nodes by a SHA-1 path. Verify that digest before accepting
                        // bytes; no API credentials are sent to these nodes.
                        let hash = next.path().strip_prefix("/download/").unwrap_or("");
                        let host = next.host_str().unwrap_or("");
                        let optifine_node =
                            allow_optifine_node && is_optifine_mirror_redirect(&url, &next);
                        if url.host_str() != Some("bmclapi2.bangbang93.com")
                            || (!optifine_node
                                && (hash.len() != 40
                                    || !hash.bytes().all(|b| b.is_ascii_hexdigit())))
                            || next.scheme() != "https"
                            || next.port().is_some()
                            || !next.username().is_empty()
                            || next.password().is_some()
                            || host.parse::<std::net::IpAddr>().is_ok()
                            || !host.contains('.')
                            || host.ends_with(".local")
                            || host.ends_with(".internal")
                        {
                            return Err(error);
                        }
                        if !optifine_node {
                            mirror_hash = Some(hash.to_ascii_lowercase());
                        }
                        url = next;
                    }
                }
                continue;
            }
            if !res.status().is_success() {
                return Err(format!("文件源返回 HTTP {}。", res.status().as_u16()));
            }
            response = Some(res);
            break;
        }
        let mut r = response.ok_or("文件源重定向次数过多。".to_string())?;
        if size > 0 && r.content_length().is_some_and(|n| n != size) {
            return Err("文件大小与平台记录不一致。".to_string());
        }
        let mut n = 0;
        let mut mirror_digest = Sha1::new();
        while let Some(bytes) = r
            .chunk()
            .await
            .map_err(|_| "文件下载中断，请重试。".to_string())?
        {
            n += bytes.len() as u64;
            if n > if size > 0 { size } else { 512 * 1024 * 1024 } {
                return Err("下载内容超过允许大小。".to_string());
            }
            out.write_all(&bytes)
                .map_err(|_| "无法写入下载文件，请检查磁盘空间。".to_string())?;
            if mirror_hash.is_some() {
                mirror_digest.update(&bytes);
            }
            let _ = tx.try_send(n);
        }
        if size > 0 && n != size {
            return Err("下载文件不完整。".to_string());
        }
        if mirror_hash.is_some_and(|hash| hex::encode(mirror_digest.finalize()) != hash) {
            return Err("镜像下载文件 SHA-1 校验失败。".into());
        }
        Ok(())
    });
    loop {
        if cancel.load(Ordering::Relaxed) {
            task.abort();
            let _ = tauri::async_runtime::block_on(task);
            return Err("下载已取消。".into());
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(n) => progress(n),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    tauri::async_runtime::block_on(task).map_err(|_| "下载任务异常结束。".to_string())?
}

/// Equivalent public CurseForge CDN nodes, preserving the exact file and query.
/// Only these known nodes are interchangeable; other download hosts stay intact.
pub(crate) fn download_sources(value: &str) -> Vec<String> {
    let Ok(mut url) = allowed(value) else {
        return Vec::new();
    };
    let mut sources = vec![url.to_string()];
    if matches!(
        url.host_str(),
        Some("mediafilez.forgecdn.net" | "edge.forgecdn.net" | "media.forgecdn.net")
    ) && url.path().starts_with("/files/")
    {
        for host in [
            "mediafilez.forgecdn.net",
            "edge.forgecdn.net",
            "media.forgecdn.net",
        ] {
            if url.set_host(Some(host)).is_ok() && !sources.contains(&url.to_string()) {
                sources.push(url.to_string());
            }
        }
    }
    sources
}

pub fn fetch(
    url: &str,
    target: &Path,
    sha1: &str,
    sha512: &str,
    size: u64,
    cancel: &AtomicBool,
    key: Option<&str>,
    progress: &mut dyn FnMut(u64),
) -> R<()> {
    fetch_sources(
        &[url.to_owned()],
        target,
        sha1,
        sha512,
        size,
        cancel,
        key,
        progress,
    )
}

pub(crate) fn fetch_sources(
    urls: &[String],
    target: &Path,
    sha1: &str,
    sha512: &str,
    size: u64,
    cancel: &AtomicBool,
    key: Option<&str>,
    progress: &mut dyn FnMut(u64),
) -> R<()> {
    let long = !sha512.is_empty();
    let expected = if long { sha512 } else { sha1 };
    let len = if long { 128 } else { 40 };
    if expected.len() != len || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("文件缺少有效的校验信息。".into());
    }
    if target.is_file() && hash(target, long)?.eq_ignore_ascii_case(expected) {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|_| "无法创建下载目录。")?
    }
    let temp = target.with_extension("xumob-part");
    let mut sources = Vec::new();
    for url in urls {
        for candidate in download_sources(url) {
            if !sources.contains(&candidate) {
                sources.push(candidate);
            }
        }
    }
    if sources.is_empty() {
        return Err("整合包未提供可用的文件下载地址。".into());
    }
    let mut attempt = |url: &str| {
        let mut out = File::create(&temp).map_err(|_| "无法创建下载文件。")?;
        stream(url, &mut out, size, cancel, key, progress)?;
        out.sync_all().map_err(|_| "无法保存下载文件。")?;
        drop(out);
        if !hash(&temp, long)?.eq_ignore_ascii_case(expected) {
            return Err("下载文件校验失败。".into());
        }
        fs::rename(&temp, target).map_err(|_| "无法保存校验后的下载文件。")?;
        Ok(())
    };
    let mut result = Err("文件下载失败。".to_string());
    'retry: for round in 0..3 {
        for source in &sources {
            if cancel.load(Ordering::Relaxed) {
                result = Err("下载已取消。".into());
                break 'retry;
            }
            result = attempt(source);
            if result.is_ok() {
                break 'retry;
            }
        }
        if round < 2 {
            std::thread::sleep(Duration::from_millis(300));
        }
    }
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curseforge_nodes_preserve_file_query_and_do_not_duplicate_sources() {
        let sources = download_sources("https://edge.forgecdn.net/files/1234/5/a%20b.jar?x=1");
        assert_eq!(sources.len(), 3);
        assert!(sources
            .iter()
            .all(|s| s.ends_with("/files/1234/5/a%20b.jar?x=1")));
        let unchanged = "https://cdn.modrinth.com/data/project/versions/id/a.jar";
        assert_eq!(download_sources(unchanged), vec![unchanged]);
        assert_eq!(
            download_sources("https://mediafilez.forgecdn.net/avatars/a.png").len(),
            1
        );
        assert!(download_sources("https://edge.forgecdn.net.evil.invalid/files/a.jar").is_empty());
    }
}
