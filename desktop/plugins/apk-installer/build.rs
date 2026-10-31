// The plugin exposes no commands of its own to the web view; the app calls
// `install` from Rust. This only links the Android library into the app.
const COMMANDS: &[&str] = &[];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).android_path("android").build();
}
