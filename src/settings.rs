use std::fs;
use std::path::PathBuf;

pub const DEFAULT_HOTKEY: &str = "Ctrl+Shift+R";
pub const DEFAULT_BITRATE_KBPS: u32 = 64;
pub const DEFAULT_AGC: bool = true;

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
    pub recordings_dir: Option<PathBuf>,
    /// Level the microphone and desktop streams towards a common target before
    /// mixing, so neither one buries the other. On by default.
    pub agc: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.to_string(),
            format: OutputFormat::Opus,
            bitrate_kbps: DEFAULT_BITRATE_KBPS,
            recordings_dir: None,
            agc: DEFAULT_AGC,
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

        let recordings_dir = self
            .recordings_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();

        let content = format!(
            "hotkey={}\nformat={}\nbitrate={}\nagc={}\nrecordings_dir={recordings_dir}\n",
            self.hotkey,
            self.format.as_str(),
            self.bitrate_kbps,
            if self.agc { "on" } else { "off" }
        );
        fs::write(&path, content).map_err(|e| e.to_string())
    }

    pub fn set_recordings_dir(path: PathBuf) -> Result<(), String> {
        let mut settings = Self::load();
        settings.recordings_dir = Some(path);
        settings.save()
    }

    /// Flips the AGC setting and persists it. Returns the new value.
    pub fn toggle_agc() -> Result<bool, String> {
        let mut settings = Self::load();
        settings.agc = !settings.agc;
        settings.save()?;
        Ok(settings.agc)
    }
}

fn parse_settings(content: &str) -> Option<Settings> {
    let mut hotkey = None;
    let mut format = OutputFormat::Opus;
    let mut bitrate_kbps = DEFAULT_BITRATE_KBPS;
    let mut recordings_dir = None;
    let mut agc = DEFAULT_AGC;

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
        } else if let Some(value) = line.strip_prefix("agc=") {
            agc = parse_bool(value).unwrap_or(DEFAULT_AGC);
        } else if let Some(value) = line.strip_prefix("recordings_dir=") {
            let value = value.trim();
            if !value.is_empty() {
                recordings_dir = Some(PathBuf::from(value));
            }
        }
    }

    Some(Settings {
        hotkey: hotkey.unwrap_or_else(|| DEFAULT_HOTKEY.to_string()),
        format,
        bitrate_kbps,
        recordings_dir,
        agc,
    })
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "yes" => Some(true),
        "off" | "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

pub fn settings_path() -> PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("com", "localrecord", "LocalRecord") {
        return dirs.config_dir().join("settings.ini");
    }

    home_dir()
        .join("Documents")
        .join("LocalRecord")
        .join("settings.ini")
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
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

    #[test]
    fn agc_is_on_unless_turned_off() {
        assert!(parse_settings("hotkey=Ctrl+R\n").unwrap().agc);
        assert!(!parse_settings("agc=off\n").unwrap().agc);
        assert!(parse_settings("agc=on\n").unwrap().agc);
        // An unreadable value must not silently disable levelling.
        assert!(parse_settings("agc=maybe\n").unwrap().agc);
    }

    #[test]
    fn parses_recordings_dir() {
        let settings =
            parse_settings("hotkey=Ctrl+R\nrecordings_dir=D:\\Audio\\LocalRecord\n").unwrap();
        assert_eq!(
            settings.recordings_dir,
            Some(PathBuf::from(r"D:\Audio\LocalRecord"))
        );
    }
}
