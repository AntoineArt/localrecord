use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::config;
use crate::hotkey;
use crate::icon;
use crate::log;
use crate::notification;
use crate::settings::Settings;
use crate::startup;

pub const MENU_START: &str = "start_recording";
pub const MENU_STOP: &str = "stop_recording";
pub const MENU_OPEN: &str = "open_folder";
pub const MENU_CHANGE_FOLDER: &str = "change_folder";
pub const MENU_STARTUP: &str = "toggle_startup";
pub const MENU_AGC: &str = "toggle_agc";
pub const MENU_HOTKEY: &str = "change_hotkey";
pub const MENU_EXIT: &str = "exit";

const TOOLTIP_MAX_CHARS: usize = 120;

pub struct TrayController {
    tray: TrayIcon,
    start_item: MenuItem,
    stop_item: MenuItem,
    startup_item: CheckMenuItem,
    agc_item: CheckMenuItem,
    hotkey_item: MenuItem,
    hotkey_label: String,
    shortcut_supported: bool,
}

impl TrayController {
    pub fn new(recording: bool, hotkey_label: &str) -> Result<Self, String> {
        let start_item = MenuItem::with_id(MENU_START, "Start recording", true, None);
        let stop_item = MenuItem::with_id(MENU_STOP, "Stop recording", recording, None);
        let open_item = MenuItem::with_id(MENU_OPEN, "Open recordings folder", true, None);
        let change_folder_item =
            MenuItem::with_id(MENU_CHANGE_FOLDER, "Change recordings folder...", true, None);
        // A Wayland session cannot deliver the X11-grabbed shortcut, and where
        // no compositor binding can stand in for it the picker would happily
        // save a shortcut that never fires. Grey it out rather than offer a
        // setting with no effect.
        let shortcut_supported = hotkey::shortcut_configurable();
        let hotkey_item = MenuItem::with_id(
            MENU_HOTKEY,
            hotkey_menu_label(hotkey_label, shortcut_supported),
            shortcut_supported,
            None,
        );
        let startup_item = CheckMenuItem::with_id(
            MENU_STARTUP,
            startup_menu_label(),
            true,
            startup::is_enabled(),
            None,
        );
        let agc_item = CheckMenuItem::with_id(
            MENU_AGC,
            "Auto-level mic and desktop audio",
            true,
            Settings::load().agc,
            None,
        );
        let exit_item = MenuItem::with_id(MENU_EXIT, "Exit", true, None);

        let menu = Menu::with_items(&[
            &start_item,
            &stop_item,
            &PredefinedMenuItem::separator(),
            &open_item,
            &change_folder_item,
            &hotkey_item,
            &agc_item,
            &startup_item,
            &PredefinedMenuItem::separator(),
            &exit_item,
        ])
        .map_err(|e| e.to_string())?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(&format!("LocalRecord ({hotkey_label})"))
            .with_icon(icon::tray_icon(recording))
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Self {
            tray,
            start_item,
            stop_item,
            startup_item,
            agc_item,
            hotkey_item,
            hotkey_label: hotkey_label.to_string(),
            shortcut_supported,
        })
    }

    pub fn set_recording(&mut self, recording: bool) -> Result<(), String> {
        self.sync_recording_state(recording)
    }

    pub fn repair_tray_after_stop(&mut self) -> Result<(), String> {
        self.repair_tray_icon()
    }

    pub fn set_hotkey_label(&mut self, label: &str) -> Result<(), String> {
        self.hotkey_label = label.to_string();
        self.hotkey_item
            .set_text(hotkey_menu_label(label, self.shortcut_supported));
        self.sync_recording_state(false)
    }

    /// Re-applies icon, menu items, and tooltip after notifications.
    pub fn refresh_after_notification(&mut self, recording: bool) -> Result<(), String> {
        self.sync_recording_state(recording)?;
        self.repair_tray_icon()
    }

    /// Re-registers the shell tray icon so right-click menu works again after toasts.
    fn repair_tray_icon(&mut self) -> Result<(), String> {
        #[cfg(windows)]
        {
            crate::balloon::invalidate_tray_target();
        }
        self.tray
            .set_visible(false)
            .map_err(|e| format!("Failed to hide tray icon during repair: {e}"))?;
        self.tray
            .set_visible(true)
            .map_err(|e| format!("Failed to restore tray icon during repair: {e}"))?;
        #[cfg(windows)]
        crate::balloon::focus_tray_for_menu();
        Ok(())
    }

    fn sync_recording_state(&mut self, recording: bool) -> Result<(), String> {
        self.start_item.set_enabled(!recording);
        self.stop_item.set_enabled(recording);
        self.tray
            .set_icon(Some(icon::tray_icon(recording)))
            .map_err(|e| e.to_string())?;
        self.update_tooltip(recording)
    }

    fn update_tooltip(&self, recording: bool) -> Result<(), String> {
        let tooltip = if recording {
            "LocalRecord: recording...".to_string()
        } else {
            format!("LocalRecord ({})", self.hotkey_label)
        };
        self.tray
            .set_tooltip(Some(truncate_tooltip(&tooltip)))
            .map_err(|e| e.to_string())
    }

    pub fn notify(&mut self, headline: &str, recording: bool) {
        let _ = notification::show_message(headline, "");
        let _ = self.refresh_after_notification(recording);
    }

    pub fn notify_recording_saved(
        &mut self,
        path: &std::path::Path,
        clipboard_ok: bool,
        recording: bool,
    ) {
        let _ = notification::show_recording_saved(path, clipboard_ok);
        let _ = self.refresh_after_notification(recording);
    }

    pub fn handle_menu_event(&self, event: &MenuEvent) -> Option<TrayAction> {
        match event.id.0.as_str() {
            MENU_START => Some(TrayAction::Start),
            MENU_STOP => Some(TrayAction::Stop),
            MENU_OPEN => {
                open_recordings_folder();
                None
            }
            MENU_CHANGE_FOLDER => Some(TrayAction::ChangeRecordingsFolder),
            MENU_STARTUP => Some(TrayAction::ToggleStartup),
            MENU_AGC => Some(TrayAction::ToggleAgc),
            MENU_HOTKEY => Some(TrayAction::ChangeHotkey),
            MENU_EXIT => Some(TrayAction::Exit),
            _ => None,
        }
    }

    pub fn handle_tray_event(&self, event: &TrayIconEvent) -> Option<TrayAction> {
        if matches!(event, TrayIconEvent::DoubleClick { .. }) {
            return Some(TrayAction::Toggle);
        }
        None
    }

    pub fn set_startup_checked(&mut self, enabled: bool) {
        let _ = self.startup_item.set_checked(enabled);
    }

    pub fn set_agc_checked(&mut self, enabled: bool) {
        let _ = self.agc_item.set_checked(enabled);
    }
}

fn hotkey_menu_label(label: &str, supported: bool) -> String {
    if supported {
        format!("Change shortcut ({label})")
    } else {
        "Change shortcut (unavailable on Wayland)".to_string()
    }
}

fn truncate_tooltip(text: &str) -> String {
    if text.chars().count() <= TOOLTIP_MAX_CHARS {
        return text.to_string();
    }
    text.chars()
        .take(TOOLTIP_MAX_CHARS.saturating_sub(1))
        .chain(std::iter::once('…'))
        .collect()
}

pub enum TrayAction {
    Start,
    Stop,
    Toggle,
    ToggleStartup,
    ToggleAgc,
    ChangeHotkey,
    ChangeRecordingsFolder,
    Exit,
}

pub fn open_recordings_folder() {
    let path = config::recordings_dir();
    if let Err(err) = open_folder(&path) {
        log::error(&format!("Failed to open {}: {err}", path.display()));
    }
}

fn open_folder(path: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| format!("create_dir_all failed: {e}"))?;

    #[cfg(windows)]
    return open_folder_windows(path);

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        log::info(&format!("Recordings folder: {}", path.display()));
        Ok(())
    }
}

#[cfg(windows)]
fn open_folder_windows(path: &std::path::Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    std::fs::create_dir_all(path).map_err(|e| format!("create_dir_all failed: {e}"))?;

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let result = ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(wide.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        );

        if result.0 as isize <= 32 {
            return Err(format!("ShellExecuteW failed (code {})", result.0 as isize));
        }
    }

    Ok(())
}

#[cfg(windows)]
fn startup_menu_label() -> &'static str {
    "Launch at Windows startup"
}

#[cfg(target_os = "linux")]
fn startup_menu_label() -> &'static str {
    "Launch at login"
}

#[cfg(not(any(windows, target_os = "linux")))]
fn startup_menu_label() -> &'static str {
    "Launch at startup"
}
