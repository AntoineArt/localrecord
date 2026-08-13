#[cfg(windows)]
mod imp;

#[cfg(target_os = "linux")]
mod imp_linux;

#[cfg(windows)]
pub use imp::{copy_recording_to_clipboard, init_clipboard};

#[cfg(target_os = "linux")]
pub use imp_linux::{copy_recording_to_clipboard, init_clipboard};

#[cfg(not(any(windows, target_os = "linux")))]
pub fn init_clipboard() {}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn copy_recording_to_clipboard(
    _wav_bytes: Option<&[u8]>,
    _file_path: &std::path::Path,
    _owner: (),
) -> Result<(), String> {
    Err("Clipboard copy is not supported on this platform".to_string())
}
