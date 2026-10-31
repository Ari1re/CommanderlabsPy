//! Android only: opens the system installer for a downloaded update APK.
//! Android installs it over the running app and keeps the app's data.

use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

#[cfg(target_os = "android")]
use tauri::Manager;

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "com.commanderlab.apkinstaller";

#[cfg(target_os = "android")]
struct Installer<R: Runtime>(tauri::plugin::PluginHandle<R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("apk-installer")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                let handle = _api.register_android_plugin(PLUGIN_IDENTIFIER, "ApkInstallerPlugin")?;
                _app.manage(Installer(handle));
            }
            Ok(())
        })
        .build()
}

/// Open Android's installer for the APK at `path`. Returns
/// `{"status": "started"}`, or `{"status": "permission"}` after sending the
/// user to the "install unknown apps" setting for this app.
/// Blocks until Android answers, so call it off the main thread.
#[cfg(target_os = "android")]
pub fn install<R: Runtime>(app: &tauri::AppHandle<R>, path: &str) -> Result<serde_json::Value, String> {
    let installer = app.try_state::<Installer<R>>().ok_or("The installer isn't ready yet.")?;
    installer
        .0
        .run_mobile_plugin::<serde_json::Value>("install", serde_json::json!({ "path": path }))
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "android"))]
pub fn install<R: Runtime>(_app: &tauri::AppHandle<R>, _path: &str) -> Result<serde_json::Value, String> {
    Err("Installing an APK only works on Android.".into())
}
