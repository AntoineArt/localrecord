use std::path::PathBuf;
use std::thread;

use winit::event_loop::EventLoopProxy;

use crate::audio::Recorder;
use crate::clipboard;
use crate::config;
use crate::folder_picker;
use crate::hotkey::{self, HotkeyManager};
use crate::log;
use crate::settings::Settings;
use crate::startup;
use crate::tray::{TrayAction, TrayController};

pub enum UserEvent {
    Menu(tray_icon::menu::MenuEvent),
    Tray(tray_icon::TrayIconEvent),
    RecordingFinished(RecordingFinishedOutcome),
}

pub enum RecordingFinishedOutcome {
    Saved { path: PathBuf, clipboard_ok: bool },
    Empty { duration_secs: f64 },
    Failed { message: String },
}

enum AppState {
    Idle,
    Recording { recorder: Recorder },
    Finalizing,
}

pub struct App {
    tray: TrayController,
    hotkeys: HotkeyManager,
    state: AppState,
    clipboard_owner: crate::hidden_window::ClipboardOwner,
    event_proxy: EventLoopProxy<UserEvent>,
}

impl App {
    pub fn new(
        clipboard_owner: crate::hidden_window::ClipboardOwner,
        event_proxy: EventLoopProxy<UserEvent>,
    ) -> Result<Self, String> {
        startup::ensure_enabled();
        let hotkeys = HotkeyManager::from_settings()?;
        let hotkey_label = hotkeys.label();

        Ok(Self {
            tray: TrayController::new(false, &hotkey_label)?,
            hotkeys,
            state: AppState::Idle,
            clipboard_owner,
            event_proxy,
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

    pub fn handle_recording_finished(&mut self, outcome: RecordingFinishedOutcome) {
        self.state = AppState::Idle;

        match outcome {
            RecordingFinishedOutcome::Saved { path, clipboard_ok } => {
                log::info(&format!("Recording saved to {}", path.display()));
                self.tray.notify_recording_saved(&path, clipboard_ok, false);
            }
            RecordingFinishedOutcome::Empty { duration_secs } => {
                log::error(&format!("Recording was empty ({duration_secs:.1}s)"));
                self.tray.notify("Recording was empty", false);
            }
            RecordingFinishedOutcome::Failed { message } => {
                log::error(&message);
                self.tray.notify(&message, false);
            }
        }
    }

    fn handle_tray_action(&mut self, action: TrayAction) {
        match action {
            TrayAction::Start => self.start_recording(),
            TrayAction::Stop => self.stop_recording(),
            TrayAction::Toggle => self.toggle_recording(),
            TrayAction::ToggleStartup => self.toggle_startup(),
            TrayAction::ToggleAgc => self.toggle_agc(),
            TrayAction::ChangeHotkey => self.change_hotkey(),
            TrayAction::ChangeRecordingsFolder => self.change_recordings_folder(),
            TrayAction::Exit => std::process::exit(0),
        }
    }

    fn change_hotkey(&mut self) {
        if matches!(
            self.state,
            AppState::Recording { .. } | AppState::Finalizing
        ) {
            self.tray
                .notify("Stop recording before changing shortcut", false);
            return;
        }

        let current = self.hotkeys.binding();
        if let Err(err) = self.hotkeys.pause() {
            log::error(&format!("Failed to pause shortcut for picker: {err}"));
            self.tray.notify("Could not open shortcut picker", false);
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
            Some(new_binding) => match self.hotkeys.replace(&new_binding) {
                Ok(()) => {
                    self.hotkeys.drain_pending_events();
                    let label = self.hotkeys.label();
                    let _ = self.tray.set_hotkey_label(&label);
                    let msg = format!("Shortcut changed to {label}");
                    self.tray.notify(&msg, false);
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
                    self.tray.notify("Could not register that shortcut", false);
                }
            },
        }
    }

    fn change_recordings_folder(&mut self) {
        if matches!(
            self.state,
            AppState::Recording { .. } | AppState::Finalizing
        ) {
            self.tray
                .notify("Stop recording before changing folder", false);
            return;
        }

        let current = config::recordings_dir();
        let picked = folder_picker::pick_folder(Some(&current));

        match picked {
            None => {
                log::info("Recordings folder change cancelled");
            }
            Some(path) => match Settings::set_recordings_dir(path.clone()) {
                Ok(()) => {
                    let msg = format!("Recordings will be saved to {}", path.display());
                    self.tray.notify(&msg, false);
                    log::info(&msg);
                }
                Err(err) => {
                    log::error(&format!("Failed to save recordings folder: {err}"));
                    self.tray
                        .notify("Could not save recordings folder setting", false);
                }
            },
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
                    startup_enabled_message()
                } else {
                    "LocalRecord startup disabled"
                };
                self.tray.notify(msg, false);
                log::info(msg);
            }
            Err(err) => {
                log::error(&format!("Startup toggle failed: {err}"));
                self.tray.notify("Could not update startup setting", false);
            }
        }
    }

    /// The mixer reads this setting once when a recording starts, so a change
    /// made mid-recording only takes effect on the next one.
    fn toggle_agc(&mut self) {
        match Settings::toggle_agc() {
            Ok(enabled) => {
                self.tray.set_agc_checked(enabled);
                let msg = if enabled {
                    "Auto-levelling enabled"
                } else {
                    "Auto-levelling disabled"
                };
                let msg = if matches!(
                    self.state,
                    AppState::Recording { .. } | AppState::Finalizing
                ) {
                    format!("{msg} (applies to the next recording)")
                } else {
                    msg.to_string()
                };
                self.tray.notify(&msg, false);
                log::info(&msg);
            }
            Err(err) => {
                log::error(&format!("Auto-levelling toggle failed: {err}"));
                self.tray
                    .notify("Could not update auto-levelling setting", false);
            }
        }
    }

    fn toggle_recording(&mut self) {
        match &self.state {
            AppState::Idle => self.start_recording(),
            AppState::Recording { .. } | AppState::Finalizing => self.stop_recording(),
        }
    }

    fn start_recording(&mut self) {
        if !matches!(self.state, AppState::Idle) {
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
                self.tray.notify("Could not start recording", false);
            }
        }
    }

    fn stop_recording(&mut self) {
        let AppState::Recording { recorder } =
            std::mem::replace(&mut self.state, AppState::Finalizing)
        else {
            return;
        };

        let _ = self.tray.set_recording(false);
        let _ = self.tray.repair_tray_after_stop();
        let owner = self.clipboard_owner;
        let proxy = self.event_proxy.clone();

        thread::spawn(move || {
            let outcome = finalize_recording(recorder, owner);
            let _ = proxy.send_event(UserEvent::RecordingFinished(outcome));
        });
    }
}

fn finalize_recording(
    recorder: Recorder,
    clipboard_owner: crate::hidden_window::ClipboardOwner,
) -> RecordingFinishedOutcome {
    let result = match recorder.stop() {
        Ok(result) => result,
        Err(err) => {
            return RecordingFinishedOutcome::Failed {
                message: format!("Failed to stop recording: {err}"),
            };
        }
    };

    if result.sample_frames == 0 {
        let _ = std::fs::remove_file(&result.path);
        return RecordingFinishedOutcome::Empty {
            duration_secs: result.duration_secs,
        };
    }

    let path = result.path;
    let is_wav = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"));

    let wav_bytes = if is_wav {
        match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(err) => {
                return RecordingFinishedOutcome::Failed {
                    message: format!("Failed to read saved recording: {err}"),
                };
            }
        }
    } else {
        None
    };

    let clipboard_ok =
        clipboard::copy_recording_to_clipboard(wav_bytes.as_deref(), &path, clipboard_owner)
            .is_ok();

    RecordingFinishedOutcome::Saved { path, clipboard_ok }
}

#[cfg(windows)]
fn startup_enabled_message() -> &'static str {
    "LocalRecord will start with Windows"
}

#[cfg(target_os = "linux")]
fn startup_enabled_message() -> &'static str {
    "LocalRecord will start with your session"
}

#[cfg(not(any(windows, target_os = "linux")))]
fn startup_enabled_message() -> &'static str {
    "LocalRecord will start automatically"
}
