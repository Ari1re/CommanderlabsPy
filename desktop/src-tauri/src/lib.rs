//! Commander Lab native shell: hosts the shared UI and forwards every call to
//! the core engine. Same code for Windows, Linux and Android.

use commander_lab_core::api::{default_data_dir, Engine};
use serde_json::Value;
use std::sync::Arc;
use tauri::{Manager, State};

/// The UI's single entry point. Work runs off the UI thread.
#[tauri::command]
async fn api(engine: State<'_, Arc<Engine>>, cmd: String, args: Option<Value>) -> Result<Value, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.call(&cmd, args.unwrap_or(Value::Null)))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Opens web links and update downloads in the system browser / installer.
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let platform = if cfg!(target_os = "android") { "android" } else { "desktop" };
            // Desktop keeps using the same folder as the browser edition so
            // both see the same decks; Android uses the app's private storage.
            let data = if cfg!(mobile) { app.path().app_data_dir()? } else { default_data_dir() };
            let engine = Engine::new(data, platform).map_err(|e| e.to_string())?;
            app.manage(engine);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![api])
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
