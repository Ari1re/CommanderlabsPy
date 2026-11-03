//! Commander Lab native shell: hosts the shared UI and forwards every call to
//! the core engine. Same code for Windows, Linux and Android.

use commander_lab_core::api::{default_data_dir, Engine};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};
use tauri_plugin_deep_link::DeepLinkExt;

/// commanderlab:// links (share QR codes) that arrived before the UI asked
/// for them: the one the app was launched with, or ones opened since.
#[derive(Default)]
struct PendingLinks(Mutex<Vec<String>>);

/// The UI's single entry point. Work runs off the UI thread.
#[tauri::command]
async fn api(engine: State<'_, Arc<Engine>>, cmd: String, args: Option<Value>) -> Result<Value, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.call(&cmd, args.unwrap_or(Value::Null)))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

/// The UI takes the links waiting to be opened (at start-up, and whenever
/// the "deep-link" event says a new one arrived).
#[tauri::command]
fn take_deep_links(links: State<'_, PendingLinks>) -> Vec<String> {
    std::mem::take(&mut *links.0.lock().unwrap())
}

/// Android: open the system installer for a downloaded update APK.
#[tauri::command]
async fn install_apk(app: tauri::AppHandle, path: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || tauri_plugin_apk_installer::install(&app, &path))
        .await
        .map_err(|e| e.to_string())?
}

/// App start-up (desktop `main` and the Android entry point). It registers
/// the opener and APK-installer plugins, opens the engine on the data folder
/// and exposes the `api` and `install_apk` commands. The engine is shut down
/// when a window closes (this stops a running llama/Ollama server we started).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Opens web links and update downloads in the system browser / installer.
        .plugin(tauri_plugin_opener::init())
        // Android updates: hands the downloaded APK to the package installer.
        .plugin(tauri_plugin_apk_installer::init())
        // Scanning a share QR with the phone camera opens
        // commanderlab://deck/CLD2.… here (see core/src/share.rs).
        .plugin(tauri_plugin_deep_link::init())
        .manage(PendingLinks::default())
        .setup(|app| {
            // A link the app was started with, then any opened while running.
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                app.state::<PendingLinks>().0.lock().unwrap().extend(urls.iter().map(|u| u.to_string()));
            }
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                handle.state::<PendingLinks>().0.lock().unwrap().extend(event.urls().iter().map(|u| u.to_string()));
                let _ = handle.emit("deep-link", ());
            });
            let platform = if cfg!(target_os = "android") { "android" } else { "desktop" };
            // Desktop keeps using the same folder as the browser edition so
            // both see the same decks; Android uses the app's private storage.
            let data = if cfg!(mobile) { app.path().app_data_dir()? } else { default_data_dir() };
            let engine = Engine::new(data, platform).map_err(|e| e.to_string())?;
            app.manage(engine);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![api, install_apk, take_deep_links])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if let Some(engine) = window.app_handle().try_state::<Arc<Engine>>() {
                    engine.shutdown();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Commander Lab");
}
