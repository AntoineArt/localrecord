//! A command file the desktop can write.
//!
//! The mirror of [`crate::state`]: state goes out, instructions come back.
//! Signals cover the two actions a keybinding needs, but they carry nothing —
//! a format or a bitrate has no room in a signal number. A file is the smallest
//! channel that carries a value: no port, no bus name, no dependency.
//!
//! One command per line, written by anything that can append to a file:
//!
//! ```text
//! record          # start or stop, whichever applies
//! agc             # toggle auto-levelling
//! startup         # toggle launch at login
//! tray            # show or hide the tray icon
//! folder          # open the recordings folder picker
//! shortcut        # open the shortcut picker
//! format wav      # or `format opus`
//! bitrate 96      # kbps, clamped to what the encoder accepts
//! quit
//! ```
//!
//! The file is renamed aside before being read, so a writer appending during
//! the read loses nothing already queued. A write landing in the window between
//! the rename and the delete is dropped — a UI action the user can simply take
//! again, which is worth more than a lock file on the fast path.

use std::fs;
use std::path::PathBuf;

use crate::settings::OutputFormat;

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    ToggleRecording,
    ToggleAgc,
    ToggleStartup,
    ChangeFolder,
    ChangeShortcut,
    SetFormat(OutputFormat),
    SetBitrate(u32),
    ToggleTray,
    Quit,
}

/// Everything queued since the last call, in the order it was written.
pub fn take_pending() -> Vec<Command> {
    let path = command_file();
    if !path.exists() {
        return Vec::new();
    }

    let taken = path.with_extension("taken");
    if fs::rename(&path, &taken).is_err() {
        return Vec::new();
    }

    let contents = fs::read_to_string(&taken).unwrap_or_default();
    let _ = fs::remove_file(&taken);

    contents.lines().filter_map(parse).collect()
}

fn parse(line: &str) -> Option<Command> {
    let line = line.trim();
    let (verb, argument) = match line.split_once(char::is_whitespace) {
        Some((verb, argument)) => (verb, argument.trim()),
        None => (line, ""),
    };

    match verb.to_ascii_lowercase().as_str() {
        "record" => Some(Command::ToggleRecording),
        "agc" => Some(Command::ToggleAgc),
        "startup" => Some(Command::ToggleStartup),
        "tray" => Some(Command::ToggleTray),
        "folder" => Some(Command::ChangeFolder),
        "shortcut" => Some(Command::ChangeShortcut),
        "quit" => Some(Command::Quit),
        "format" if !argument.is_empty() => {
            Some(Command::SetFormat(OutputFormat::from_setting(argument)))
        }
        "bitrate" => argument.parse().ok().map(Command::SetBitrate),
        _ => None,
    }
}

pub fn command_file() -> PathBuf {
    crate::config::log_file().with_file_name("command")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_verbs_a_widget_sends() {
        assert_eq!(parse("record"), Some(Command::ToggleRecording));
        assert_eq!(parse("  agc  "), Some(Command::ToggleAgc));
        assert_eq!(parse("QUIT"), Some(Command::Quit));
        assert_eq!(parse("tray"), Some(Command::ToggleTray));
        assert_eq!(
            parse("format wav"),
            Some(Command::SetFormat(OutputFormat::Wav))
        );
        assert_eq!(parse("bitrate 96"), Some(Command::SetBitrate(96)));
    }

    #[test]
    fn drops_what_it_cannot_act_on() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("# a comment"), None);
        assert_eq!(parse("format"), None);
        assert_eq!(parse("bitrate soon"), None);
        // An unknown format is not an error worth dropping the command over:
        // the settings parser already falls back to Opus.
        assert_eq!(
            parse("format flac"),
            Some(Command::SetFormat(OutputFormat::Opus))
        );
    }
}
