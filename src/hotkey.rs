use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};

use crate::hotkey_format::format_hotkey;
use crate::settings::Settings;

pub struct HotkeyManager {
    manager: GlobalHotKeyManager,
    hotkey: HotKey,
}

impl HotkeyManager {
    pub fn from_settings() -> Result<Self, String> {
        let settings = Settings::load();
        Self::with_binding(&settings.hotkey)
    }

    pub fn with_binding(binding: &str) -> Result<Self, String> {
        let hotkey = parse_binding(binding)?;
        let manager = GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
        manager.register(hotkey).map_err(|e| e.to_string())?;
        Ok(Self { manager, hotkey })
    }

    pub fn label(&self) -> String {
        format_hotkey(self.hotkey)
    }

    pub fn binding(&self) -> String {
        format_hotkey(self.hotkey)
    }

    pub fn replace(&mut self, binding: &str) -> Result<(), String> {
        let new_hotkey = parse_binding(binding)?;
        if new_hotkey.id() == self.hotkey.id() {
            return Ok(());
        }

        let _ = self.manager.unregister(self.hotkey);
        self.manager
            .register(new_hotkey)
            .map_err(|e| e.to_string())?;
        self.hotkey = new_hotkey;

        let mut settings = Settings::load();
        settings.hotkey = self.binding();
        settings.save()?;

        Ok(())
    }

    pub fn pause(&mut self) -> Result<(), String> {
        self.manager
            .unregister(self.hotkey)
            .map_err(|e| e.to_string())
    }

    pub fn resume(&mut self) -> Result<(), String> {
        self.manager
            .register(self.hotkey)
            .map_err(|e| e.to_string())
    }

    pub fn drain_pending_events(&self) {
        while GlobalHotKeyEvent::receiver().try_recv().is_ok() {}
    }

    pub fn poll_toggle(&self) -> bool {
        GlobalHotKeyEvent::receiver().try_iter().any(|event| {
            event.id == self.hotkey.id() && event.state == global_hotkey::HotKeyState::Pressed
        })
    }
}

fn parse_binding(binding: &str) -> Result<HotKey, String> {
    binding
        .parse::<HotKey>()
        .map_err(|e| format!("Invalid shortcut \"{binding}\": {e}"))
}

#[cfg(windows)]
pub fn pick_hotkey_interactive(current: &str) -> Option<String> {
    crate::hotkey_picker::pick_hotkey(current)
}

#[cfg(target_os = "linux")]
pub fn pick_hotkey_interactive(current: &str) -> Option<String> {
    crate::hotkey_picker_linux::pick_hotkey(current)
}

/// Whether the global shortcut can actually reach us.
///
/// `global-hotkey` grabs keys through X11. A native Wayland session never
/// routes anything to that grab, so the shortcut is registered and silently
/// never fires. Callers use this to avoid offering a setting that cannot work —
/// see [`crate::signals`] for what to bind instead.
#[cfg(target_os = "linux")]
pub fn global_shortcut_supported() -> bool {
    let wayland_display = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
    let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    wayland_display.is_empty() && !session_type.eq_ignore_ascii_case("wayland")
}

#[cfg(not(target_os = "linux"))]
pub fn global_shortcut_supported() -> bool {
    true
}

/// What to tell the user instead, when the shortcut cannot work.
pub const WAYLAND_SHORTCUT_HINT: &str =
    "Global shortcut unavailable on Wayland — bind `pkill -USR1 -x localrecord` in your compositor";
