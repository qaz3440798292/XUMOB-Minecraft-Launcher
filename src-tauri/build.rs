use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=XUMOB_BUILD_CURSEFORGE_API_KEY");
    println!("cargo:rerun-if-env-changed=XUMOB_BUILD_MICROSOFT_CLIENT_ID");
    if let Ok(client_id) = env::var("XUMOB_BUILD_MICROSOFT_CLIENT_ID") {
        let client_id = client_id.trim();
        if valid_client_id(client_id) {
            println!("cargo:rustc-env=XUMOB_BUILD_MICROSOFT_CLIENT_ID={client_id}");
        }
    }

    // The local GNU toolchain uses Microsoft's static WebView2 loader. Its
    // MSVC runtime references are supplied by the companion CRT shim.
    println!("cargo:rerun-if-env-changed=XUMOB_WEBVIEW2_STATIC_DIR");
    if let Some(directory) = env::var_os("XUMOB_WEBVIEW2_STATIC_DIR") {
        let directory = PathBuf::from(directory);
        let library = directory.join("lib").join("libWebView2LoaderStatic.a");
        let shim = directory.join("webview2_crt_shim.o");
        if !library.is_file() || !shim.is_file() {
            panic!(
                "XUMOB_WEBVIEW2_STATIC_DIR must contain lib/libWebView2LoaderStatic.a and webview2_crt_shim.o"
            );
        }
        println!(
            "cargo:rustc-link-search=native={}",
            directory.join("lib").display()
        );
        println!("cargo:rustc-link-arg-bins={}", shim.display());
    }

    tauri_build::build()
}

fn valid_client_id(value: &str) -> bool {
    let parts: Vec<_> = value.split('-').collect();
    matches!(parts.as_slice(), [a, b, c, d, e]
        if a.len() == 8
            && b.len() == 4
            && c.len() == 4
            && d.len() == 4
            && e.len() == 12
            && parts.iter().all(|part| part.bytes().all(|byte| byte.is_ascii_hexdigit())))
}
