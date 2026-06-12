use std::path::PathBuf;

pub fn recordings_dir() -> PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("com", "localrecord", "LocalRecord") {
        let path = dirs.data_dir().join("recordings");
        std::fs::create_dir_all(&path).ok();
        return path;
    }

    let fallback = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default())
        .join("Documents")
        .join("LocalRecord");
    std::fs::create_dir_all(&fallback).ok();
    fallback
}

pub fn recording_filename() -> PathBuf {
    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    recordings_dir().join(format!("recording_{timestamp}.wav"))
}

pub fn log_file() -> PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("com", "localrecord", "LocalRecord") {
        return dirs.data_dir().join("localrecord.log");
    }
    PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default())
        .join("Documents")
        .join("LocalRecord")
        .join("localrecord.log")
}
