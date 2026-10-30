//! Clipboard access that compiles on every target.
//!
//! Desktop builds use `arboard`. Android has no arboard backend, so the
//! fallback reports an error and callers fall back to writing files in
//! `./exports` (every export path already does this).

#[cfg(not(target_os = "android"))]
pub use arboard::Clipboard;

#[cfg(target_os = "android")]
pub struct Clipboard;

#[cfg(target_os = "android")]
#[derive(Debug)]
pub struct ClipboardUnavailable;

#[cfg(target_os = "android")]
impl std::fmt::Display for ClipboardUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("clipboard is not available on Android; see ./exports")
    }
}

#[cfg(target_os = "android")]
impl std::error::Error for ClipboardUnavailable {}

#[cfg(target_os = "android")]
impl Clipboard {
    pub fn new() -> Result<Self, ClipboardUnavailable> {
        Err(ClipboardUnavailable)
    }

    pub fn set_text<T: Into<String>>(&mut self, _text: T) -> Result<(), ClipboardUnavailable> {
        Err(ClipboardUnavailable)
    }

    #[allow(dead_code)]
    pub fn get_text(&mut self) -> Result<String, ClipboardUnavailable> {
        Err(ClipboardUnavailable)
    }
}
