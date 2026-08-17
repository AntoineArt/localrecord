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
        #[cfg(target_os = "linux")]
        crate::state::init();

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

    /// Toggle on SIGUSR1, which is how a Wayland compositor reaches us — the
    /// X11 key grab behind `poll_hotkey` never fires there. SIGUSR2 flips
    /// auto-levelling, for desktop widgets. See [`crate::signals`].
    #[cfg(target_os = "linux")]
    pub fn poll_signal_toggle(&mut self) {
        if crate::signals::take_toggle_request() {
            self.toggle_recording();
        }
        if crate::signals::take_agc_toggle_request() {
            self.toggle_agc();
        }
    }

    /// Acts on what a desktop widget queued. Every arm routes through the same
    /// method the tray menu calls, so the menu, the panel and the state file
    /// can never disagree about what happened. See [`crate::command`].
    #[cfg(target_os = "linux")]
    pub fn poll_commands(&mut self) {
        for command in crate::command::take_pending() {
            match command {
                crate::command::Command::ToggleRecording => self.toggle_recording(),
                crate::command::Command::ToggleAgc => self.toggle_agc(),
                crate::command::Command::ToggleStartup => self.toggle_startup(),
                crate::command::Command::ChangeFolder => self.change_recordings_folder(),
                crate::command::Command::ChangeShortcut => self.change_hotkey(),
                crate::command::Command::SetFormat(format) => self.set_format(format),
                crate::command::Command::SetBitrate(kbps) => self.set_bitrate(kbps),
                crate::command::Command::ToggleTray => self.toggle_tray(),
                crate::command::Command::Quit => std::process::exit(0),
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn set_format(&mut self, format: crate::settings::OutputFormat) {
        match Settings::set_format(format) {
            Ok(()) => {
                crate::state::refresh();
                let msg = format!(
                    "Recording format set to {}{}",
                    format.as_str().to_uppercase(),
                    self.next_recording_suffix()
                );
                self.tray.notify(&msg, false);
                log::info(&msg);
            }
            Err(err) => {
                log::error(&format!("Failed to save format: {err}"));
                self.tray
                    .notify("Could not save the recording format", false);
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn set_bitrate(&mut self, kbps: u32) {
        match Settings::set_bitrate(kbps) {
            Ok(()) => {
                crate::state::refresh();
                let msg = format!(
                    "Bitrate set to {} kbps{}",
                    Settings::load().bitrate_kbps,
                    self.next_recording_suffix()
                );
                self.tray.notify(&msg, false);
                log::info(&msg);
            }
            Err(err) => {
                log::error(&format!("Failed to save bitrate: {err}"));
                self.tray.notify("Could not save the bitrate", false);
            }
        }
    }

    /// Hiding the icon only makes sense where something else can reach the app,
    /// so the message names what is left driving it.
    #[cfg(target_os = "linux")]
    fn toggle_tray(&mut self) {
        match Settings::toggle_tray() {
            Ok(visible) => {
                if let Err(err) = self.tray.set_visible(visible) {
                    log::error(&err);
                }
                crate::state::refresh();
                let msg = if visible {
                    "Tray icon shown".to_string()
                } else {
                    format!("Tray icon hidden — {} still works", Settings::load().hotkey)
                };
                self.tray.notify(&msg, false);
                log::info(&msg);
            }
            Err(err) => {
                log::error(&format!("Failed to save tray setting: {err}"));
                self.tray
                    .notify("Could not update the tray icon setting", false);
            }
        }
    }

    /// The encoder reads both settings once, when a recording starts.
    #[cfg(target_os = "linux")]
    fn next_recording_suffix(&self) -> &'static str {
        if matches!(
            self.state,
            AppState::Recording { .. } | AppState::Finalizing
        ) {
            " (applies to the next recording)"
        } else {
            ""
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

        #[cfg(target_os = "linux")]
        crate::state::set_recording_finished(match &outcome {
            RecordingFinishedOutcome::Saved { path, .. } => Some(path.as_path()),
            _ => None,
        });

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
        // The menu item is disabled where this cannot work, but the action is
        // still reachable, so refuse rather than save a binding that never fires.
        if !hotkey::shortcut_configurable() {
            self.tray.notify(hotkey::WAYLAND_SHORTCUT_HINT, false);
            return;
        }

        if matches!(
            self.state,
            AppState::Recording { .. } | AppState::Finalizing
        ) {
            self.tray
                .notify("Stop recording before changing shortcut", false);
            return;
        }

        // On Wayland the compositor holds the binding, so the grab we own is
        // inert: nothing to pause around the picker, and the shortcut in force
        // is the saved one rather than whatever that grab holds.
        #[cfg(target_os = "linux")]
        if !hotkey::global_shortcut_supported() {
            let current = Settings::load().hotkey;
            match hotkey::pick_hotkey_interactive(&current) {
                Some(new_binding) => self.set_compositor_hotkey(&new_binding),
                None => log::info("Shortcut change cancelled"),
            }
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
                    #[cfg(target_os = "linux")]
                    crate::state::refresh();
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

    /// Writes the picked shortcut into the compositor, which is the only thing
    /// that can deliver it on Wayland. See [`crate::hypr`].
    #[cfg(target_os = "linux")]
    fn set_compositor_hotkey(&mut self, new_binding: &str) {
        if let Err(err) = crate::hypr::set_toggle_binding(new_binding) {
            log::error(&format!("Failed to set shortcut in Hyprland: {err}"));
            self.tray
                .notify("Could not set that shortcut in Hyprland", false);
            return;
        }

        if let Err(err) = Settings::set_hotkey(new_binding) {
            log::error(&format!("Failed to save shortcut: {err}"));
        }
        // Keeps the inert grab in step with the setting, so an X11 session
        // started later picks up the same key. Failure here changes nothing.
        let _ = self.hotkeys.replace(new_binding);
        let _ = self.tray.set_hotkey_label(new_binding);
        crate::state::refresh();

        let mut msg = format!("Shortcut changed to {new_binding}");
        let conflicts = crate::hypr::manual_binding_conflicts();
        if !conflicts.is_empty() {
            // Ours is additive: a hand-written binding keeps firing on its own
            // key, which reads as the shortcut not having changed at all.
            msg.push_str(&format!(
                " — also remove the localrecord binding in {}",
                conflicts.join(", ")
            ));
        }
        self.tray.notify(&msg, false);
        log::info(&msg);
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
                    #[cfg(target_os = "linux")]
                    crate::state::refresh();
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
                #[cfg(target_os = "linux")]
                crate::state::refresh();
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
                #[cfg(target_os = "linux")]
                crate::state::refresh();
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
                #[cfg(target_os = "linux")]
                crate::state::set_recording_started();
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
