use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::config;
use crate::icon;
use crate::log;
use crate::startup;

pub const MENU_START: &str = "start_recording";
pub const MENU_STOP: &str = "stop_recording";
pub const MENU_OPEN: &str = "open_folder";
pub const MENU_STARTUP: &str = "toggle_startup";
pub const MENU_HOTKEY: &str = "change_hotkey";
pub const MENU_EXIT: &str = "exit";

pub struct TrayController {
    tray: TrayIcon,
    start_item: MenuItem,
    stop_item: MenuItem,
    startup_item: CheckMenuItem,
    hotkey_item: MenuItem,
    hotkey_label: String,
}

impl TrayController {
    pub fn new(recording: bool, hotkey_label: &str) -> Result<Self, String> {
        let start_item = MenuItem::with_id(MENU_START, "Start recording", true, None);
        let stop_item = MenuItem::with_id(MENU_STOP, "Stop recording", recording, None);
        let open_item = MenuItem::with_id(MENU_OPEN, "Open recordings folder", true, None);
        let hotkey_item = MenuItem::with_id(
            MENU_HOTKEY,
            format!("Change shortcut ({hotkey_label})"),
            true,
            None,
        );
        let startup_item = CheckMenuItem::with_id(
            MENU_STARTUP,
            "Launch at Windows startup",
            true,
            startup::is_enabled(),
            None,
        );
        let exit_item = MenuItem::with_id(MENU_EXIT, "Exit", true, None);

        let menu = Menu::with_items(&[
            &start_item,
            &stop_item,
            &PredefinedMenuItem::separator(),
            &open_item,
            &hotkey_item,
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
            hotkey_item,
            hotkey_label: hotkey_label.to_string(),
        })
    }

    pub fn set_recording(&mut self, recording: bool) -> Result<(), String> {
        self.start_item.set_enabled(!recording);
        self.stop_item.set_enabled(recording);
        self.tray
            .set_icon(Some(icon::tray_icon(recording)))
            .map_err(|e| e.to_string())?;
        self.update_tooltip(recording)
    }

    fn update_tooltip(&self, recording: bool) -> Result<(), String> {
        self.tray
            .set_tooltip(Some(if recording {
                "LocalRecord: recording...".to_string()
            } else {
                format!("LocalRecord ({})", self.hotkey_label)
            }))
            .map_err(|e| e.to_string())
    }

    pub fn set_hotkey_label(&mut self, label: &str) -> Result<(), String> {
        self.hotkey_label = label.to_string();
        self.hotkey_item
            .set_text(format!("Change shortcut ({label})"));
        self.update_tooltip(false)
    }

    pub fn notify(&self, message: &str) {
        let _ = self.tray.set_tooltip(Some(message));
    }

    pub fn handle_menu_event(&self, event: &MenuEvent) -> Option<TrayAction> {
        match event.id.0.as_str() {
            MENU_START => Some(TrayAction::Start),
            MENU_STOP => Some(TrayAction::Stop),
            MENU_OPEN => {
                open_recordings_folder();
                None
            }
            MENU_STARTUP => Some(TrayAction::ToggleStartup),
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
}

pub enum TrayAction {
    Start,
    Stop,
    Toggle,
    ToggleStartup,
    ChangeHotkey,
    Exit,
}

pub fn open_recordings_folder() {
    let path = config::recordings_dir();
    #[cfg(windows)]
    if let Err(err) = open_folder_windows(&path) {
        log::error(&format!("Failed to open {}: {err}", path.display()));
    }

    #[cfg(not(windows))]
    log::info(&format!("Recordings folder: {}", path.display()));
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

        // ShellExecuteW returns a value <= 32 on failure.
        if result.0 as isize <= 32 {
            return Err(format!("ShellExecuteW failed (code {})", result.0 as isize));
        }
    }

    Ok(())
}
