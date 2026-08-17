//! A state file the desktop can read.
//!
//! On Linux the tray icon is all LocalRecord shows, and that backend has no
//! click action (see [`crate::signals`]), so anything richer — an Omarchy bar
//! widget, a waybar module, a status script — has no way to learn what the app
//! is doing. Publishing a small JSON file next to the log gives them one, and
//! costs nothing: it is rewritten only when something actually changes.
//!
//! Written atomically, because a reader watching the file would otherwise be
//! able to parse a half-written one.
//!
//! `pid` and `exe` are part of the contract: a reader cannot tell a crashed app
//! from an idle one by the fields alone, so it checks that the pid is still
//! alive and uses `exe` to offer relaunching it.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::settings::Settings;

const VERSION: u32 = 1;

#[derive(Default)]
struct State {
    recording: bool,
    started_at: u64,
    last_file: String,
    last_saved_at: u64,
}

static CURRENT: Mutex<State> = Mutex::new(State {
    recording: false,
    started_at: 0,
    last_file: String::new(),
    last_saved_at: 0,
});

/// Publishes the idle state, so a reader that starts before the first recording
/// still finds the pid, the paths and the settings.
pub fn init() {
    publish(|_| {});
}

pub fn set_recording_started() {
    publish(|state| {
        state.recording = true;
        state.started_at = now();
    });
}

/// One write per transition: the file a recording produced, if any, lands in the
/// same publish that clears the recording flag.
pub fn set_recording_finished(saved: Option<&Path>) {
    publish(|state| {
        state.recording = false;
        state.started_at = 0;
        if let Some(path) = saved {
            state.last_file = path.display().to_string();
            state.last_saved_at = now();
        }
    });
}

/// Republishes without touching the recording fields, for the settings the
/// reader mirrors — auto-levelling, the shortcut, the recordings folder.
pub fn refresh() {
    publish(|_| {});
}

fn publish(update: impl FnOnce(&mut State)) {
    let Ok(mut state) = CURRENT.lock() else {
        return;
    };
    update(&mut state);

    let settings = Settings::load();
    let json = format!(
        concat!(
            "{{\n",
            "  \"version\": {},\n",
            "  \"pid\": {},\n",
            "  \"exe\": \"{}\",\n",
            "  \"recording\": {},\n",
            "  \"started_at\": {},\n",
            "  \"last_file\": \"{}\",\n",
            "  \"last_saved_at\": {},\n",
            "  \"agc\": {},\n",
            "  \"hotkey\": \"{}\",\n",
            "  \"format\": \"{}\",\n",
            "  \"recordings_dir\": \"{}\"\n",
            "}}\n"
        ),
        VERSION,
        std::process::id(),
        escape(&exe_path()),
        state.recording,
        state.started_at,
        escape(&state.last_file),
        state.last_saved_at,
        settings.agc,
        escape(&settings.hotkey),
        settings.format.as_str(),
        escape(&crate::config::recordings_dir().display().to_string()),
    );

    write_atomically(&state_file(), &json);
}

/// Rename over the real path, so a reader either sees the previous file or the
/// new one — never a truncated one.
fn write_atomically(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }

    let temporary = path.with_extension("json.tmp");
    if fs::write(&temporary, contents).is_err() {
        return;
    }
    if fs::rename(&temporary, path).is_err() {
        let _ = fs::remove_file(&temporary);
    }
}

pub fn state_file() -> PathBuf {
    crate::config::log_file().with_file_name("state.json")
}

fn exe_path() -> String {
    std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_default()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            // Control characters are not valid raw in a JSON string, and a path
            // is free to contain them.
            c if (c as u32) < 0x20 => escaped.push_str(&format!("\\u{:04x}", c as u32)),
            c => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_what_json_cannot_hold_raw() {
        assert_eq!(escape("plain/path"), "plain/path");
        assert_eq!(escape("with \"quotes\""), "with \\\"quotes\\\"");
        assert_eq!(escape("back\\slash"), "back\\\\slash");
        assert_eq!(escape("line\nbreak"), "line\\nbreak");
        assert_eq!(escape("bell\u{7}"), "bell\\u0007");
    }

    #[test]
    fn state_file_sits_next_to_the_log() {
        let state = state_file();
        assert_eq!(state.file_name().unwrap(), "state.json");
        assert_eq!(state.parent(), crate::config::log_file().parent());
    }
}
