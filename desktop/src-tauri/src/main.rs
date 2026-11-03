// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Desktop entry point. Everything lives in `lib.rs` so Android can share it.
fn main() {
    commander_lab_app::run();
}
