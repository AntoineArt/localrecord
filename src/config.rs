use std::path::PathBuf;

use crate::log;
use crate::settings::Settings;

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

pub fn default_recordings_dir() -> PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("com", "localrecord", "LocalRecord") {
        let path = dirs.data_dir().join("recordings");
        std::fs::create_dir_all(&path).ok();
        return path;
    }

    let fallback = home_dir().join("Documents").join("LocalRecord");
    std::fs::create_dir_all(&fallback).ok();
    fallback
}

pub fn recordings_dir() -> PathBuf {
    let settings = Settings::load();
    if let Some(path) = settings.recordings_dir {
        if std::fs::create_dir_all(&path).is_ok() {
            return path;
        }
        log::error(&format!(
            "Configured recordings folder is unavailable: {}",
            path.display()
        ));
    }

    default_recordings_dir()
}

pub fn recording_filename() -> PathBuf {
    let settings = Settings::load();
    recording_filename_for(&settings)
}

pub fn recording_filename_for(settings: &Settings) -> PathBuf {
    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    recordings_dir().join(format!(
        "recording_{timestamp}.{}",
        settings.format.extension()
    ))
}

pub fn log_file() -> PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("com", "localrecord", "LocalRecord") {
        return dirs.data_dir().join("localrecord.log");
    }
    home_dir()
        .join("Documents")
        .join("LocalRecord")
        .join("localrecord.log")
}
