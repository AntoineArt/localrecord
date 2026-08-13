#[cfg(windows)]
mod imp;

#[cfg(target_os = "linux")]
mod imp_linux;

#[cfg(windows)]
pub use imp::{init, show_message, show_recording_saved};

#[cfg(target_os = "linux")]
pub use imp_linux::{init, show_message, show_recording_saved};

#[cfg(not(any(windows, target_os = "linux")))]
pub fn show_message(_headline: &str, _detail: &str) -> bool {
    false
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn init() {}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn show_recording_saved(_path: &std::path::Path, _clipboard_ok: bool) -> bool {
    false
}
