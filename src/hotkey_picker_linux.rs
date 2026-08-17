use std::process::Command;

use crate::hotkey_format::format_hotkey;
use crate::log;

pub fn pick_hotkey(current: &str) -> Option<String> {
    let prompt = format!(
        "Enter a new shortcut (example: Ctrl+Shift+R)\nCurrent: {current}"
    );

    let output = Command::new("zenity")
        .args([
            "--entry",
            "--title=LocalRecord shortcut",
            "--text",
            &prompt,
            "--entry-text",
            current,
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    // Confirming the shortcut already in force is a real answer, not a cancel:
    // on Wayland it is how the binding gets written into the compositor for the
    // first time. Callers treat re-applying the same key as a no-op anyway.
    let picked = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if picked.is_empty() {
        return None;
    }

    match picked.parse::<global_hotkey::hotkey::HotKey>() {
        Ok(hotkey) => Some(format_hotkey(hotkey)),
        Err(err) => {
            log::error(&format!("Invalid shortcut \"{picked}\": {err}"));
            None
        }
    }
}
