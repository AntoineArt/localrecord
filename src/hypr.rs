//! Hyprland owns the shortcut, so this is how we set it.
//!
//! Wayland gives a client no way to grab a key globally — the X11 grab behind
//! [`crate::hotkey`] never receives anything there. What a compositor does
//! offer is a binding that runs a command, and [`crate::signals`] already turns
//! `pkill -USR1 -x localrecord` into a toggle. So the tray's picker stays
//! useful on Hyprland: we write that binding into a file of our own, loaded
//! once from the entry config, and apply it live over `hyprctl`.
//!
//! Hyprland 0.56 reads either `hyprland.lua` or the legacy `hyprland.conf`, and
//! the two share nothing — `hyprctl keyword` is refused outright under the Lua
//! parser, and a `source` line in `hyprland.conf` is dead weight when that file
//! is not the one being read. Hence [`ConfigFlavor`], and two spellings of the
//! same binding throughout.
//!
//! Only our own file is ever rewritten. Bindings the user maintains by hand are
//! left alone, and a leftover one is reported instead — see
//! [`manual_binding_conflicts`], because it would keep firing alongside ours.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const TOGGLE_COMMAND: &str = "pkill -USR1 -x localrecord";
const BINDING_DESCRIPTION: &str = "LocalRecord: toggle recording";

/// Whether we are running under Hyprland, i.e. whether the tray picker can
/// write a binding that actually fires.
pub fn available() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some_and(|value| !value.is_empty())
}

/// Points the compositor's toggle binding at `binding`, live and on disk.
///
/// The write is followed by a reload rather than a live `bind`, because
/// Hyprland reloads on its own as soon as a config file changes: binding on top
/// of that lands the same shortcut twice, and a doubled bind toggles recording
/// on and straight back off. A reload rebuilds every binding from the config,
/// so it is idempotent — and it drops the previous key without us having to
/// unbind it.
pub fn set_toggle_binding(binding: &str) -> Result<(), String> {
    let key = KeySpec::parse(binding)?;
    let dir = hypr_dir()?;

    match ConfigFlavor::detect(&dir) {
        ConfigFlavor::Lua => write_lua_binding(&dir, &key)?,
        ConfigFlavor::Conf => write_conf_binding(&dir, &key)?,
    }

    hyprctl(&["reload"])
}

/// Hand-written `localrecord` bindings still sitting in the user's own config.
///
/// Ours is additive, so one of these keeps toggling on its own key after a
/// change from the tray — which reads as "the shortcut did not change".
/// Returns the file names, for a message that says where to look.
pub fn manual_binding_conflicts() -> Vec<String> {
    let Ok(dir) = hypr_dir() else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut files: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !is_managed(name) && is_live_config(name))
        .filter(|name| mentions_localrecord(&dir.join(name)))
        .collect();

    files.sort();
    files
}

/// Which config Hyprland actually reads. Lua wins when both are present, the
/// same way Hyprland picks between them.
enum ConfigFlavor {
    Lua,
    Conf,
}

impl ConfigFlavor {
    fn detect(dir: &Path) -> Self {
        if dir.join("hyprland.lua").is_file() {
            Self::Lua
        } else {
            Self::Conf
        }
    }

    fn managed_file(&self) -> &'static str {
        match self {
            Self::Lua => "localrecord.lua",
            Self::Conf => "localrecord.conf",
        }
    }
}

fn write_lua_binding(dir: &Path, key: &KeySpec) -> Result<(), String> {
    let managed = dir.join(ConfigFlavor::Lua.managed_file());
    let content = format!(
        "-- Managed by LocalRecord — rewritten whenever you pick a shortcut from the\n\
         -- tray menu, so edits here do not survive. To take the binding back into\n\
         -- your own hands, drop the `dofile` line for this file from hyprland.lua.\n\
         {}\n",
        key.lua_bind()
    );
    write_managed(&managed, &content)?;

    ensure_loaded(
        &dir.join("hyprland.lua"),
        ConfigFlavor::Lua.managed_file(),
        "--",
        &format!("dofile(\"{}\")\n", managed.display()),
    )
}

fn write_conf_binding(dir: &Path, key: &KeySpec) -> Result<(), String> {
    let managed = dir.join(ConfigFlavor::Conf.managed_file());
    let content = format!(
        "# Managed by LocalRecord — rewritten whenever you pick a shortcut from the\n\
         # tray menu, so edits here do not survive. To take the binding back into\n\
         # your own hands, drop the `source` line for this file from hyprland.conf.\n\
         bindd = {}\n",
        key.conf_bindd()
    );
    write_managed(&managed, &content)?;

    ensure_loaded(
        &dir.join("hyprland.conf"),
        ConfigFlavor::Conf.managed_file(),
        "#",
        &format!("source = {}\n", managed.display()),
    )
}

fn write_managed(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    fs::write(path, content).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

/// Adds a single load line to the entry config, once, so the binding survives a
/// reload. Everything else in that file is left untouched.
fn ensure_loaded(
    config: &Path,
    managed_file: &str,
    comment: &str,
    load_line: &str,
) -> Result<(), String> {
    let content = fs::read_to_string(config)
        .map_err(|e| format!("Failed to read {}: {e}", config.display()))?;

    if already_loads(&content, comment, managed_file) {
        return Ok(());
    }

    let separator = if content.ends_with('\n') { "" } else { "\n" };
    let updated = format!(
        "{content}{separator}\n{comment} Added by LocalRecord: shortcut picked from the tray menu.\n{load_line}"
    );
    fs::write(config, updated).map_err(|e| format!("Failed to update {}: {e}", config.display()))
}

fn already_loads(content: &str, comment: &str, managed_file: &str) -> bool {
    content.lines().any(|line| {
        let line = line.trim();
        !line.starts_with(comment) && line.contains(managed_file)
    })
}

/// Skips the `.bak` copies Omarchy and friends leave behind; only files the
/// compositor actually reads can conflict.
fn is_live_config(name: &str) -> bool {
    if name.contains(".bak") {
        return false;
    }
    name.ends_with(".conf") || name.ends_with(".lua")
}

fn is_managed(name: &str) -> bool {
    name == ConfigFlavor::Lua.managed_file() || name == ConfigFlavor::Conf.managed_file()
}

fn mentions_localrecord(path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };

    content.lines().any(is_conflicting_line)
}

/// The line that loads our own file mentions us without conflicting with us —
/// the entry config would otherwise report itself.
fn is_conflicting_line(line: &str) -> bool {
    let line = line.trim();
    !line.starts_with('#')
        && !line.starts_with("--")
        && line.contains("localrecord")
        && !line.contains(ConfigFlavor::Lua.managed_file())
        && !line.contains(ConfigFlavor::Conf.managed_file())
}

fn hyprctl(args: &[&str]) -> Result<(), String> {
    let output = Command::new("hyprctl")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run hyprctl: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && stdout.trim() == "ok" {
        return Ok(());
    }

    // hyprctl reports a rejected keyword or a Lua error on stdout, and exits 0
    // either way, so the "ok" above is the only real success signal.
    let detail = if stdout.trim().is_empty() {
        String::from_utf8_lossy(&output.stderr).trim().to_string()
    } else {
        stdout.trim().to_string()
    };
    Err(format!("hyprctl {} failed: {detail}", args.join(" ")))
}

fn hypr_dir() -> Result<PathBuf, String> {
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        if !config_home.is_empty() {
            return Ok(PathBuf::from(config_home).join("hypr"));
        }
    }

    let home = std::env::var_os("HOME").ok_or_else(|| "HOME is not set".to_string())?;
    Ok(PathBuf::from(home).join(".config").join("hypr"))
}

/// A binding in Hyprland's own terms: modifiers plus an xkb keysym name, in
/// whichever of the two config dialects is being spoken.
struct KeySpec {
    mods: Vec<&'static str>,
    key: String,
}

impl KeySpec {
    fn parse(binding: &str) -> Result<Self, String> {
        let mut parts: Vec<&str> = binding.split('+').map(str::trim).collect();
        let key = parts
            .pop()
            .filter(|key| !key.is_empty())
            .ok_or_else(|| format!("Empty shortcut \"{binding}\""))?;

        Ok(Self {
            mods: parts
                .iter()
                .map(|part| hypr_modifier(part))
                .collect::<Result<Vec<_>, _>>()?,
            key: hypr_key(key)?,
        })
    }

    /// What legacy `unbind` takes, and the head of a `bindd` value.
    fn conf_trigger(&self) -> String {
        format!("{}, {}", self.mods.join(" "), self.key)
    }

    fn conf_bindd(&self) -> String {
        format!(
            "{}, {BINDING_DESCRIPTION}, exec, {TOGGLE_COMMAND}",
            self.conf_trigger()
        )
    }

    /// The Lua API spells the same trigger `"CTRL + SHIFT + R"`.
    fn lua_keys(&self) -> String {
        let mut parts: Vec<&str> = self.mods.iter().copied().collect();
        parts.push(&self.key);
        parts.join(" + ")
    }

    fn lua_bind(&self) -> String {
        format!(
            "hl.bind(\"{}\", hl.dsp.exec_cmd(\"{TOGGLE_COMMAND}\"), {{ description = \"{BINDING_DESCRIPTION}\" }})",
            self.lua_keys()
        )
    }
}

fn hypr_modifier(part: &str) -> Result<&'static str, String> {
    match part.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Ok("CTRL"),
        "alt" => Ok("ALT"),
        "shift" => Ok("SHIFT"),
        "win" | "super" | "meta" | "cmd" => Ok("SUPER"),
        other => Err(format!("Unknown modifier \"{other}\"")),
    }
}

/// Maps our label for a key onto the xkb keysym name Hyprland expects.
///
/// Letters, digits and function keys pass straight through. The rest is a
/// lookup, and an unknown key is refused rather than written out as a binding
/// Hyprland would silently drop.
fn hypr_key(key: &str) -> Result<String, String> {
    if key.len() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Ok(key.to_ascii_uppercase());
    }

    if let Some(number) = key.strip_prefix('F') {
        if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) {
            return Ok(key.to_string());
        }
    }

    let keysym = match key {
        "Space" => "space",
        "Enter" | "Return" => "return",
        "Tab" => "tab",
        "Escape" => "escape",
        "Backspace" => "backspace",
        "Delete" => "delete",
        "Insert" => "insert",
        "Home" => "home",
        "End" => "end",
        "PageUp" => "prior",
        "PageDown" => "next",
        "ArrowUp" => "up",
        "ArrowDown" => "down",
        "ArrowLeft" => "left",
        "ArrowRight" => "right",
        "Minus" => "minus",
        "Equal" => "equal",
        "Comma" => "comma",
        "Period" => "period",
        "Slash" => "slash",
        "Backslash" => "backslash",
        "Semicolon" => "semicolon",
        "Quote" => "apostrophe",
        "BracketLeft" => "bracketleft",
        "BracketRight" => "bracketright",
        "Backquote" => "grave",
        "PrintScreen" => "print",
        "Pause" => "pause",
        other => return Err(format!("Key \"{other}\" cannot be bound in Hyprland")),
    };

    Ok(keysym.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaks_both_config_dialects() {
        let key = KeySpec::parse("Ctrl+Shift+R").unwrap();
        assert_eq!(key.conf_trigger(), "CTRL SHIFT, R");
        assert_eq!(
            key.conf_bindd(),
            "CTRL SHIFT, R, LocalRecord: toggle recording, exec, pkill -USR1 -x localrecord"
        );
        assert_eq!(key.lua_keys(), "CTRL + SHIFT + R");
        assert_eq!(
            key.lua_bind(),
            "hl.bind(\"CTRL + SHIFT + R\", hl.dsp.exec_cmd(\"pkill -USR1 -x localrecord\"), \
             { description = \"LocalRecord: toggle recording\" })"
        );
    }

    #[test]
    fn maps_super_and_named_keys() {
        let key = KeySpec::parse("Win+Alt+Space").unwrap();
        assert_eq!(key.conf_trigger(), "SUPER ALT, space");
        assert_eq!(key.lua_keys(), "SUPER + ALT + space");
        assert_eq!(KeySpec::parse("F9").unwrap().conf_trigger(), ", F9");
    }

    #[test]
    fn refuses_keys_hyprland_would_drop() {
        assert!(KeySpec::parse("Ctrl+MediaPlayPause").is_err());
        assert!(KeySpec::parse("Hyper+R").is_err());
        assert!(KeySpec::parse("").is_err());
    }

    #[test]
    fn only_hand_written_bindings_count_as_conflicts() {
        assert!(is_conflicting_line(
            "o.bind(\"CTRL + SHIFT + R\", \"LocalRecord\", \"pkill -USR1 -x localrecord\")"
        ));
        assert!(is_conflicting_line(
            "bindd = CTRL SHIFT, R, Audio, exec, pkill -USR1 -x localrecord"
        ));
        // Our own load line, in the config we just edited.
        assert!(!is_conflicting_line(
            "dofile(\"/home/x/.config/hypr/localrecord.lua\")"
        ));
        assert!(!is_conflicting_line(
            "source = /home/x/.config/hypr/localrecord.conf"
        ));
        assert!(!is_conflicting_line(
            "-- o.bind(\"CTRL + R\", \"x\", \"pkill -USR1 -x localrecord\")"
        ));
    }

    #[test]
    fn recognises_a_config_that_already_loads_us() {
        let lua = "require(\"hypr.bindings\")\ndofile(\"/home/x/.config/hypr/localrecord.lua\")\n";
        assert!(already_loads(lua, "--", "localrecord.lua"));
        assert!(!already_loads(
            "-- dofile(\"/home/x/.config/hypr/localrecord.lua\")\n",
            "--",
            "localrecord.lua"
        ));
        assert!(already_loads(
            "source = /home/x/.config/hypr/localrecord.conf\n",
            "#",
            "localrecord.conf"
        ));
        assert!(!already_loads(
            "source = ~/.config/hypr/bindings.conf\n",
            "#",
            "localrecord.conf"
        ));
    }
}
