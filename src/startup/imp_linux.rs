use std::env;
use std::fs;
use std::path::PathBuf;

const DESKTOP_FILE: &str = "localrecord.desktop";

pub fn exe_path() -> Option<PathBuf> {
    env::current_exe().ok()
}

pub fn is_enabled() -> bool {
    autostart_dir()
        .ok()
        .map(|dir| dir.join(DESKTOP_FILE))
        .is_some_and(|path| path.exists())
}

pub fn enable() -> Result<(), String> {
    let exe = exe_path().ok_or("Could not resolve executable path")?;
    let path = autostart_dir()?.join(DESKTOP_FILE);
    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=LocalRecord\n\
         Comment=Record microphone and desktop audio\n\
         Exec={}\n\
         Terminal=false\n\
         Categories=AudioVideo;Audio;\n\
         X-GNOME-Autostart-enabled=true\n",
        exe.display()
    );
    fs::write(&path, content).map_err(|e| e.to_string())
}

pub fn disable() -> Result<(), String> {
    let path = autostart_dir()?.join(DESKTOP_FILE);
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn ensure_enabled() {
    if !is_enabled() {
        let _ = enable();
    }
}

fn autostart_dir() -> Result<PathBuf, String> {
    let config = env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|_| env::var("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map_err(|e| e.to_string())?;
    let dir = config.join("autostart");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}
