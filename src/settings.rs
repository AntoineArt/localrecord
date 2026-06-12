use std::fs;
use std::path::PathBuf;

pub const DEFAULT_HOTKEY: &str = "Ctrl+Shift+R";

#[derive(Clone, Debug)]
pub struct Settings {
    pub hotkey: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.to_string(),
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let path = settings_path();
        if !path.exists() {
            return Self::default();
        }

        match fs::read_to_string(&path) {
            Ok(content) => parse_settings(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let content = format!("hotkey={}\n", self.hotkey);
        fs::write(&path, content).map_err(|e| e.to_string())
    }
}

fn parse_settings(content: &str) -> Option<Settings> {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("hotkey=") {
            let hotkey = value.trim();
            if !hotkey.is_empty() {
                return Some(Settings {
                    hotkey: hotkey.to_string(),
                });
            }
        }
    }
    None
}

pub fn settings_path() -> PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("com", "localrecord", "LocalRecord") {
        return dirs.config_dir().join("settings.ini");
    }

    PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default())
        .join("Documents")
        .join("LocalRecord")
        .join("settings.ini")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hotkey_line() {
        let settings = parse_settings("hotkey=Alt+F9\n").unwrap();
        assert_eq!(settings.hotkey, "Alt+F9");
    }
}
