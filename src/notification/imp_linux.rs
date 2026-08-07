use std::path::Path;

pub fn init() {}

pub fn show_recording_saved(path: &Path, clipboard_ok: bool) -> bool {
    let headline = if clipboard_ok {
        "Recording saved and copied to clipboard"
    } else {
        "Recording saved (clipboard copy failed)"
    };
    let filename = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let folder = path
        .parent()
        .map(|parent| parent.display().to_string())
        .unwrap_or_default();

    show_message(headline, &format!("{filename}\n{folder}"))
}

pub fn show_message(headline: &str, detail: &str) -> bool {
    match notify_rust::Notification::new()
        .summary("LocalRecord")
        .body(&format!("{headline}\n{detail}"))
        .timeout(notify_rust::Timeout::Milliseconds(8000))
        .show()
    {
        Ok(_) => true,
        Err(err) => {
            crate::log::error(&format!("Notification failed: {err}"));
            false
        }
    }
}
