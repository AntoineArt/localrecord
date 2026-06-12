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

#[cfg(not(windows))]
pub fn pick_hotkey_interactive(_current: &str) -> Option<String> {
    None
}
