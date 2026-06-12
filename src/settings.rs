use std::fs;
use std::path::PathBuf;

pub const DEFAULT_HOTKEY: &str = "Ctrl+Shift+R";
pub const DEFAULT_BITRATE_KBPS: u32 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Opus,
    Wav,
}

impl OutputFormat {
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "wav" => Self::Wav,
            _ => Self::Opus,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Opus => "opus",
            Self::Wav => "wav",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Opus => "opus",
            Self::Wav => "wav",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub hotkey: String,
    pub format: OutputFormat,
    pub bitrate_kbps: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.to_string(),
            format: OutputFormat::Opus,
            bitrate_kbps: DEFAULT_BITRATE_KBPS,
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

        let content = format!(
            "hotkey={}\nformat={}\nbitrate={}\n",
            self.hotkey,
            self.format.as_str(),
            self.bitrate_kbps
        );
        fs::write(&path, content).map_err(|e| e.to_string())
    }
}

fn parse_settings(content: &str) -> Option<Settings> {
    let mut hotkey = None;
    let mut format = OutputFormat::Opus;
    let mut bitrate_kbps = DEFAULT_BITRATE_KBPS;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("hotkey=") {
            let value = value.trim();
            if !value.is_empty() {
                hotkey = Some(value.to_string());
            }
        } else if let Some(value) = line.strip_prefix("format=") {
            format = OutputFormat::from_setting(value);
        } else if let Some(value) = line.strip_prefix("bitrate=") {
            if let Ok(kbps) = value.trim().parse::<u32>() {
                bitrate_kbps = kbps.clamp(32, 128);
            }
        }
    }

    hotkey.map(|hotkey| Settings {
        hotkey,
        format,
        bitrate_kbps,
    })
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
        assert_eq!(settings.format, OutputFormat::Opus);
    }

    #[test]
    fn parses_format_and_bitrate() {
        let settings = parse_settings("hotkey=Ctrl+R\nformat=wav\nbitrate=96\n").unwrap();
        assert_eq!(settings.format, OutputFormat::Wav);
        assert_eq!(settings.bitrate_kbps, 96);
    }
}
