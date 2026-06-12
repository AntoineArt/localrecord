#[cfg(windows)]
#[path = "notification/imp.rs"]
mod imp;

#[cfg(windows)]
pub use imp::{init, show_recording_saved};

#[cfg(not(windows))]
pub fn init() {}

#[cfg(not(windows))]
pub fn show_recording_saved(_path: &std::path::Path, _clipboard_ok: bool) -> bool {
    false
}
