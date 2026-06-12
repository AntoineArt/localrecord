use global_hotkey::hotkey::{Code, HotKey, Modifiers};

/// User-facing label, e.g. "Ctrl+Shift+R".
pub fn format_hotkey(hotkey: HotKey) -> String {
    let mut parts: Vec<String> = Vec::new();

    if hotkey.mods.contains(Modifiers::CONTROL) {
        parts.push("Ctrl".to_string());
    }
    if hotkey.mods.contains(Modifiers::ALT) {
        parts.push("Alt".to_string());
    }
    if hotkey.mods.contains(Modifiers::SHIFT) {
        parts.push("Shift".to_string());
    }
    if hotkey.mods.contains(Modifiers::SUPER) {
        parts.push("Win".to_string());
    }

    parts.push(format_key(hotkey.key));
    parts.join("+")
}

fn format_key(code: Code) -> String {
    let raw = code.to_string();
    if let Some(stripped) = raw.strip_prefix("Key") {
        return stripped.to_string();
    }
    if let Some(stripped) = raw.strip_prefix("Digit") {
        return stripped.to_string();
    }
    raw
}
