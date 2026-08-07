use std::path::Path;

use arboard::Clipboard;

pub fn init_clipboard() {}

pub fn copy_recording_to_clipboard(
    _wav_bytes: Option<&[u8]>,
    file_path: &Path,
    _owner: (),
) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
    let uri = format!("file://{}", file_path.display());
    clipboard
        .set_text(uri)
        .map_err(|e| e.to_string())?;

    crate::log::info("Recording file path copied to clipboard");
    Ok(())
}
