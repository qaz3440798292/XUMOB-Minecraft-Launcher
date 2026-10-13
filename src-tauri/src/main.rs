#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use blockfield_core::core::Service;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{Emitter, Manager, State, WebviewWindow};

type Backend = Arc<Service>;
#[tauri::command]
async fn account_configure(
    service: State<'_, Backend>,
    client_id: String,
) -> Result<Value, String> {
    let s = service.inner().clone();
    run(move || {
        s.ensure_account_editable()?;
        s.auth.configure(&client_id)
    })
    .await
}
#[tauri::command]
async fn account_select(service: State<'_, Backend>, mode: String) -> Result<Value, String> {
    let s = service.inner().clone();
    run(move || {
        s.ensure_account_editable()?;
        s.auth.select(&mode)
    })
    .await
}
#[tauri::command]
async fn account_logout(service: State<'_, Backend>) -> Result<Value, String> {
    let s = service.inner().clone();
    run(move || {
        s.ensure_account_editable()?;
        s.auth.logout()
    })
    .await
}
#[tauri::command]
async fn auth_start(service: State<'_, Backend>) -> Result<Value, String> {
    let s = service.inner().clone();
    run(move || {
        s.ensure_account_editable()?;
        s.auth.start()
    })
    .await
}
#[tauri::command]
async fn auth_poll(service: State<'_, Backend>, session_id: String) -> Result<Value, String> {
    let s = service.inner().clone();
    run(move || {
        s.ensure_account_editable()?;
        s.auth.poll(&session_id)
    })
    .await
}
#[tauri::command]
fn auth_cancel(service: State<'_, Backend>) {
    service.auth.cancel();
}
#[tauri::command]
async fn auth_open_browser(service: State<'_, Backend>, session_id: String) -> Result<(), String> {
    let s = service.inner().clone();
    run(move || {
        let uri = s.auth.browser_uri(&session_id)?;
        let mut command = std::process::Command::new("explorer.exe");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
            .arg(uri)
            .spawn()
            .map_err(|_| "无法打开系统浏览器，请手动打开微软登录网址。".to_string())?;
        Ok(())
    })
    .await
}
async fn run<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn snapshot(service: State<'_, Backend>) -> Result<Value, String> {
    let s = service.inner().clone();
    run(move || s.snapshot()).await
}
#[tauri::command]
async fn save_settings(service: State<'_, Backend>, patch: Value) -> Result<Value, String> {
    let s = service.inner().clone();
    run(move || s.save(patch)).await
}
#[tauri::command]
async fn add_directory(service: State<'_, Backend>, selected: String) -> Result<Value, String> {
    let s = service.inner().clone();
    run(move || s.add_directory(&selected)).await
}
#[tauri::command]
async fn find_java(service: State<'_, Backend>, force: Option<bool>) -> Result<Vec<Value>, String> {
    let s = service.inner().clone();
    run(move || s.javas(force.unwrap_or(false))).await
}
#[tauri::command]
async fn check_environment(service: State<'_, Backend>) -> Result<Value, String> {
    let s = service.inner().clone();
    run(move || s.check()).await
}
#[tauri::command]
async fn repair_files(service: State<'_, Backend>) -> Result<Value, String> {
    let s = service.inner().clone();
    run(move || s.repair()).await
}
#[tauri::command]
async fn launch_game(service: State<'_, Backend>) -> Result<Value, String> {
    let s = service.inner().clone();
    run(move || s.launch()).await
}
fn confirm_stop(close: bool, window: &tauri::Window) -> bool {
    let stop = if close {
        "停止游戏并退出"
    } else {
        "停止游戏"
    };
    let cancel = if close { "返回启动器" } else { "取消" };
    let result = rfd::MessageDialog::new()
        .set_parent(window)
        .set_title(if close {
            "游戏仍在运行"
        } else {
            "停止游戏"
        })
        .set_description(if close {
            "请先在游戏中保存进度。\n\n停止游戏进程会直接关闭游戏，未保存的进度可能丢失。"
        } else {
            "请先在游戏内保存进度。\n\n停止进程会直接关闭游戏，未保存的进度可能丢失。"
        })
        .set_level(rfd::MessageLevel::Warning)
        .set_buttons(rfd::MessageButtons::OkCancelCustom(
            cancel.into(),
            stop.into(),
        ))
        .show();
    result == rfd::MessageDialogResult::Custom(stop.into())
}
#[tauri::command]
async fn stop_game(service: State<'_, Backend>, window: WebviewWindow) -> Result<bool, String> {
    let s = service.inner().clone();
    run(move || {
        if !s.has_child() {
            return Ok(false);
        }
        if !confirm_stop(false, &window.as_ref().window()) {
            return Ok(false);
        }
        s.stop()?;
        Ok(true)
    })
    .await
}
#[tauri::command]
async fn list_mods(service: State<'_, Backend>) -> Result<Value, String> {
    let s = service.inner().clone();
    run(move || s.mods()).await
}
#[tauri::command]
async fn modrinth_search(service: State<'_, Backend>, query: String, kind: String, offset: u32, sort: String) -> Result<Value, String> {
    let s=service.inner().clone();run(move||s.modrinth_search(&query,&kind,offset,&sort)).await
}
#[tauri::command]
async fn modrinth_versions(service: State<'_, Backend>, project_id: String, kind: String) -> Result<Value, String> {
    let s=service.inner().clone();run(move||s.modrinth_versions(&project_id,&kind)).await
}
#[tauri::command]
async fn modrinth_prepare(service: State<'_, Backend>, project_id: String, version_id: String, kind: String) -> Result<Value, String> {
    let s=service.inner().clone();run(move||s.modrinth_prepare(&project_id,&version_id,&kind)).await
}
#[tauri::command]
async fn modrinth_install(service: State<'_, Backend>, plan_id: String) -> Result<Value, String> {
    let s=service.inner().clone();run(move||s.modrinth_install(&plan_id)).await
}
#[tauri::command]
fn modrinth_cancel(service: State<'_, Backend>) { service.modrinth.cancel(); }
#[tauri::command]
async fn modrinth_open(project_id: String, kind: String) -> Result<(), String> {
    let uri=blockfield_core::modrinth::project_url(&project_id,&kind)?;
    run(move||{
        let mut command=std::process::Command::new("explorer.exe");
        #[cfg(windows)] {use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
        command.arg(uri).spawn().map_err(|_|"无法打开 Modrinth 项目页。".to_string())?;Ok(())
    }).await
}
#[tauri::command]
async fn choose_folder(window: WebviewWindow) -> Result<Option<String>, String> {
    run(move || {
        Ok(rfd::FileDialog::new()
            .set_parent(&window)
            .set_title("选择整合包文件夹或 .minecraft 文件夹")
            .pick_folder()
            .map(|p| p.to_string_lossy().into_owned()))
    })
    .await
}
#[tauri::command]
async fn choose_java(window: WebviewWindow) -> Result<Option<String>, String> {
    run(move || {
        Ok(rfd::FileDialog::new()
            .set_parent(&window)
            .set_title("选择 Java 可执行文件")
            .add_filter("Java", &["exe"])
            .pick_file()
            .map(|p| p.to_string_lossy().into_owned()))
    })
    .await
}
#[tauri::command]
async fn open_folder(service: State<'_, Backend>, kind: String) -> Result<(), String> {
    let s = service.inner().clone();
    run(move || {
        let folder = s.folder(&kind)?;
        if !folder.is_dir() {
            return Err("找不到这个文件夹。".into());
        }
        let mut command = std::process::Command::new("explorer.exe");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command.arg(folder).spawn().map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
}
#[tauri::command]
fn window_action(window: WebviewWindow, action: String) -> Result<(), String> {
    match action.as_str() {
        "minimize" => window.minimize(),
        "maximize" => {
            if window.is_maximized().map_err(|e| e.to_string())? {
                window.unmaximize()
            } else {
                window.maximize()
            }
        }
        "close" => window.close(),
        "drag" => window.start_dragging(),
        _ => return Err("窗口操作无效。".into()),
    }
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn memory_mode(window: WebviewWindow, low: bool) -> Result<(), String> {
    #[cfg(windows)]
    window
        .with_webview(move |webview| {
            use webview2_com::Microsoft::Web::WebView2::Win32::{
                ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
            };
            use windows_core::Interface;
            // This supported API releases idle caches while keeping scripts and logs alive.
            // Older runtimes continue normally if they do not expose this interface.
            unsafe {
                if let Ok(core) = webview.controller().CoreWebView2() {
                    if let Ok(memory) = core.cast::<ICoreWebView2_19>() {
                        let _ = memory.SetMemoryUsageTargetLevel(if low {
                            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
                        } else {
                            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
                        });
                    }
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn main() {
    let data = std::env::var_os("BLOCKFIELD_DATA_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .join("data")
        });
    let service = Service::new(data.clone()).unwrap_or_else(|error| {
        rfd::MessageDialog::new()
            .set_title("启动器无法启动")
            .set_description(error)
            .show();
        std::process::exit(1)
    });
    let close_service = service.clone();
    tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|app,_,_|{if let Some(window)=app.get_webview_window("main"){let _=window.unminimize();let _=window.set_focus();}})).manage(service.clone()).setup(move|app|{
        let handle=app.handle().clone();let pending=Arc::new(Mutex::new(Vec::<Value>::new()));let logs=pending.clone();let events=handle.clone();service.set_sink(Arc::new(move|kind,value|{if kind=="log"{let mut pending=logs.lock().unwrap();if pending.len()<2048{pending.push(value);}}else{let _=events.emit("launcher:state",value);}}));
        let emitter=handle.clone();std::thread::spawn(move||loop{std::thread::sleep(Duration::from_millis(100));let entries={let mut pending=pending.lock().unwrap();std::mem::take(&mut *pending)};if !entries.is_empty(){let _=emitter.emit("launcher:logs",entries);}});
        let mut builder=tauri::WebviewWindowBuilder::new(app,"main",tauri::WebviewUrl::App("index.html".into())).title("XUMOB Minecraft Launcher").inner_size(1280.,850.).min_inner_size(1050.,740.).decorations(false).resizable(true).background_color(tauri::webview::Color(16,23,20,255)).data_directory(data.join("webview2"));
        // Debugging is opt-in for automated validation; normal runs open no debugging port.
        if let Ok(port)=std::env::var("BLOCKFIELD_DEBUG_PORT"){if port.parse::<u16>().is_ok(){builder=builder.additional_browser_args(&format!("--remote-debugging-port={port} --remote-debugging-address=127.0.0.1 --disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection"));}}
        let window=builder.build()?;
        let scale=window.scale_factor()?;
        // Electron's frameless Windows client includes two physical border pixels.
        // Preserve that viewport at the current system scale without changing CSS.
        window.set_size(tauri::PhysicalSize::new((1280.*scale).round() as u32+1,(850.*scale).round() as u32+2))?;
        Ok(())
    }).invoke_handler(tauri::generate_handler![snapshot,save_settings,add_directory,find_java,check_environment,repair_files,launch_game,stop_game,list_mods,modrinth_search,modrinth_versions,modrinth_prepare,modrinth_install,modrinth_cancel,modrinth_open,choose_folder,choose_java,open_folder,window_action,memory_mode,account_configure,account_select,account_logout,auth_start,auth_poll,auth_cancel,auth_open_browser]).on_window_event(move|window,event|{if let tauri::WindowEvent::CloseRequested{api,..}=event{if close_service.state()["phase"]=="downloading"{api.prevent_close();close_service.modrinth.cancel();let service=close_service.clone();let window=window.clone();std::thread::spawn(move||{while service.state()["phase"]=="downloading"{std::thread::sleep(Duration::from_millis(100));}let _=window.destroy();});return;}if close_service.has_child(){api.prevent_close();let service=close_service.clone();let window=window.clone();std::thread::spawn(move||{if confirm_stop(true,&window){let _=service.stop();for _ in 0..100{if !service.has_child(){break;}std::thread::sleep(Duration::from_millis(25));}let _=window.destroy();}});}}}).run(tauri::generate_context!()).expect("Tauri failed");
}
