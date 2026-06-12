use windows::Win32::Foundation::HWND;

use crate::audio::{self, Recorder};
use crate::clipboard;
use crate::config;
use crate::hotkey::{self, HotkeyManager};
use crate::log;
use crate::startup;
use crate::tray::{TrayAction, TrayController};

enum AppState {
    Idle,
    Recording {
        recorder: Recorder,
    },
}

pub struct App {
    tray: TrayController,
    hotkeys: HotkeyManager,
    state: AppState,
    clipboard_owner: HWND,
}

impl App {
    pub fn new(clipboard_owner: HWND) -> Result<Self, String> {
        startup::ensure_enabled();
        let hotkeys = HotkeyManager::from_settings()?;
        let hotkey_label = hotkeys.label();

        Ok(Self {
            tray: TrayController::new(false, &hotkey_label)?,
            hotkeys,
            state: AppState::Idle,
            clipboard_owner,
        })
    }

    pub fn poll_hotkey(&mut self) {
        if self.hotkeys.poll_toggle() {
            self.toggle_recording();
        }
    }

    pub fn handle_menu_event(&mut self, event: &tray_icon::menu::MenuEvent) {
        if let Some(action) = self.tray.handle_menu_event(event) {
            self.handle_tray_action(action);
        }
    }

    pub fn handle_tray_event(&mut self, event: &tray_icon::TrayIconEvent) {
        if let Some(action) = self.tray.handle_tray_event(event) {
            self.handle_tray_action(action);
        }
    }

    fn handle_tray_action(&mut self, action: TrayAction) {
        match action {
            TrayAction::Start => self.start_recording(),
            TrayAction::Stop => self.stop_recording(),
            TrayAction::Toggle => self.toggle_recording(),
            TrayAction::ToggleStartup => self.toggle_startup(),
            TrayAction::ChangeHotkey => self.change_hotkey(),
            TrayAction::Exit => std::process::exit(0),
        }
    }

    fn change_hotkey(&mut self) {
        if matches!(self.state, AppState::Recording { .. }) {
            self.tray.notify("Stop recording before changing shortcut");
            return;
        }

        let current = self.hotkeys.binding();
        if let Err(err) = self.hotkeys.pause() {
            log::error(&format!("Failed to pause shortcut for picker: {err}"));
            self.tray.notify("Could not open shortcut picker");
            return;
        }

        let picked = hotkey::pick_hotkey_interactive(&current);

        match picked {
            None => {
                if let Err(err) = self.hotkeys.resume() {
                    log::error(&format!("Failed to restore shortcut after cancel: {err}"));
                }
                self.hotkeys.drain_pending_events();
                log::info("Shortcut change cancelled");
            }
            Some(new_binding) => {
                match self.hotkeys.replace(&new_binding) {
                    Ok(()) => {
                        self.hotkeys.drain_pending_events();
                        let label = self.hotkeys.label();
                        let _ = self.tray.set_hotkey_label(&label);
                        let msg = format!("Shortcut changed to {label}");
                        self.tray.notify(&msg);
                        log::info(&msg);
                    }
                    Err(err) => {
                        log::error(&format!("Failed to set shortcut: {err}"));
                        if let Err(resume_err) = self.hotkeys.resume() {
                            log::error(&format!(
                                "Failed to restore previous shortcut: {resume_err}"
                            ));
                        }
                        self.hotkeys.drain_pending_events();
                        self.tray.notify("Could not register that shortcut");
                    }
                }
            }
        }
    }

    fn toggle_startup(&mut self) {
        let result = if startup::is_enabled() {
            startup::disable()
        } else {
            startup::enable()
        };

        match result {
            Ok(()) => {
                let enabled = startup::is_enabled();
                self.tray.set_startup_checked(enabled);
                let msg = if enabled {
                    "LocalRecord will start with Windows"
                } else {
                    "LocalRecord startup disabled"
                };
                self.tray.notify(msg);
                log::info(msg);
            }
            Err(err) => {
                log::error(&format!("Startup toggle failed: {err}"));
                self.tray.notify("Could not update startup setting");
            }
        }
    }

    fn toggle_recording(&mut self) {
        match &self.state {
            AppState::Idle => self.start_recording(),
            AppState::Recording { .. } => self.stop_recording(),
        }
    }

    fn start_recording(&mut self) {
        if matches!(self.state, AppState::Recording { .. }) {
            return;
        }

        match Recorder::start() {
            Ok(recorder) => {
                let _ = self.tray.set_recording(true);
                self.state = AppState::Recording { recorder };
                log::info("Recording started");
            }
            Err(err) => {
                log::error(&format!("Failed to start recording: {err}"));
                self.tray.notify("Could not start recording");
            }
        }
    }

    fn stop_recording(&mut self) {
        let AppState::Recording { recorder } =
            std::mem::replace(&mut self.state, AppState::Idle)
        else {
            return;
        };

        let _ = self.tray.set_recording(false);

        match recorder.stop() {
            Ok(result) => {
                if result.samples.is_empty() {
                    log::error(&format!(
                        "Recording was empty ({:.1}s)",
                        result.duration_secs
                    ));
                    self.tray.notify("Recording was empty");
                    return;
                }

                let path = config::recording_filename();
                if let Err(err) = audio::wav::save_wav(&path, &result.samples) {
                    log::error(&format!("Failed to save recording: {err}"));
                    self.tray.notify("Failed to save recording");
                    return;
                }

                log::info(&format!(
                    "Saved {:.1}s recording to {}",
                    result.duration_secs,
                    path.display()
                ));

                let wav_bytes = std::fs::read(&path).unwrap_or_else(|_| {
                    audio::wav::encode_wav(&result.samples)
                });

                match clipboard::copy_recording_to_clipboard(
                    &wav_bytes,
                    &path,
                    self.clipboard_owner,
                ) {
                    Ok(()) => {
                        self.tray.notify_recording_saved(&path, true);
                    }
                    Err(err) => {
                        log::error(&format!("Clipboard copy failed: {err}"));
                        self.tray.notify_recording_saved(&path, false);
                    }
                }
            }
            Err(err) => {
                log::error(&format!("Failed to stop recording: {err}"));
                self.tray.notify("Failed to stop recording");
            }
        }
    }
}
