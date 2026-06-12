#[cfg(windows)]
#[path = "clipboard/imp.rs"]
mod imp;

#[cfg(windows)]
pub use imp::{copy_recording_to_clipboard, init_clipboard};

#[cfg(not(windows))]
pub fn init_clipboard() {}

#[cfg(not(windows))]
pub fn copy_recording_to_clipboard(
    _wav_bytes: &[u8],
    _file_path: &std::path::Path,
    _owner: (),
) -> Result<(), String> {
    Err("Clipboard copy is only supported on Windows".to_string())
}
