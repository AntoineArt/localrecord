use std::fs::OpenOptions;
use std::io::Write;

use crate::config;

pub fn info(message: &str) {
    write("INFO", message);
}

pub fn error(message: &str) {
    write("ERROR", message);
}

fn write(level: &str, message: &str) {
    let path = config::log_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let line = format!(
        "[{}] {} {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        level,
        message
    );
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(config::log_file())
    {
        let _ = file.write_all(line.as_bytes());
    }
}
