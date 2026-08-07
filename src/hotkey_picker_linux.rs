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

    let picked = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if picked.is_empty() || picked == current {
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
