#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod balloon;

mod app;
mod audio;
mod clipboard;
mod config;
mod folder_picker;
mod hidden_window;
mod hotkey;
mod hotkey_format;
#[cfg(windows)]
mod hotkey_picker;
#[cfg(target_os = "linux")]
mod hotkey_picker_linux;
#[cfg(target_os = "linux")]
mod hypr;
mod icon;
mod log;
mod notification;
#[cfg(target_os = "linux")]
mod signals;
mod settings;
mod startup;
#[cfg(target_os = "linux")]
mod state;
mod tray;

fn main() {
    run();
}

fn run() {
    use app::{App, UserEvent};
    use std::time::{Duration, Instant};
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, ControlFlow};
    use winit::window::WindowId;

    /// How often the event loop wakes to pump GTK and poll the hotkey. Short
    /// enough that the shortcut feels instant, long enough to stay idle-cheap.
    const TICK: Duration = Duration::from_millis(30);

    struct LocalRecordApp {
        app: App,
    }

    impl ApplicationHandler<UserEvent> for LocalRecordApp {
        fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

        fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
            match event {
                UserEvent::Menu(menu_event) => self.app.handle_menu_event(&menu_event),
                UserEvent::Tray(tray_event) => self.app.handle_tray_event(&tray_event),
                UserEvent::RecordingFinished(outcome) => self.app.handle_recording_finished(outcome),
            }
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            _window_id: WindowId,
            event: WindowEvent,
        ) {
            if matches!(event, WindowEvent::CloseRequested) {
                event_loop.exit();
            }
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            // A tray-only app receives almost no winit events, so the default
            // `ControlFlow::Wait` would park here indefinitely. That starves the
            // two things this callback drives: the GTK main loop, without which
            // libappindicator never finishes registering with the StatusNotifier
            // watcher (no tray icon at all on Linux), and the hotkey poll, which
            // is what makes the global shortcut fire. Tick instead.
            event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + TICK));

            #[cfg(target_os = "linux")]
            {
                pump_gtk_events();
                self.app.poll_signal_toggle();
            }
            self.app.poll_hotkey();
        }
    }

    if !acquire_single_instance() {
        std::process::exit(0);
    }

    #[cfg(target_os = "linux")]
    {
        init_linux_gtk();
        signals::install();
    }

    clipboard::init_clipboard();
    notification::init();
    let clipboard_owner = hidden_window::create();

    let event_loop = winit::event_loop::EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("event loop");

    let app_proxy = event_loop.create_proxy();

    tray_icon::TrayIconEvent::set_event_handler(Some({
        let proxy = event_loop.create_proxy();
        move |event| {
            let _ = proxy.send_event(UserEvent::Tray(event));
        }
    }));

    tray_icon::menu::MenuEvent::set_event_handler(Some({
        let proxy = event_loop.create_proxy();
        move |event| {
            let _ = proxy.send_event(UserEvent::Menu(event));
        }
    }));

    let app = App::new(clipboard_owner, app_proxy).expect("initialize LocalRecord");
    log::info("LocalRecord started");
    if !hotkey::shortcut_configurable() {
        log::info(hotkey::WAYLAND_SHORTCUT_HINT);
    }

    let mut handler = LocalRecordApp { app };
    let _ = event_loop.run_app(&mut handler);
}

#[cfg(windows)]
fn acquire_single_instance() -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
    use windows::Win32::System::Threading::CreateMutexW;

    let name: Vec<u16> = OsStr::new("LocalRecord_SingleInstance_Mutex")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let handle = CreateMutexW(None, true, PCWSTR(name.as_ptr()));
        match handle {
            Ok(h) => {
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    let _ = CloseHandle(h);
                    false
                } else {
                    let _ = HANDLE(h.0);
                    true
                }
            }
            Err(_) => false,
        }
    }
}

#[cfg(target_os = "linux")]
fn acquire_single_instance() -> bool {
    use std::fs::OpenOptions;
    use std::os::unix::fs::OpenOptionsExt;
    use std::sync::Mutex;

    static LOCK_FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);

    let path = single_instance_lock_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let file = match OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&path)
    {
        Ok(file) => file,
        Err(_) => return true,
    };

    if !try_lock_file(&file) {
        return false;
    }

    if let Ok(mut guard) = LOCK_FILE.lock() {
        *guard = Some(file);
    }
    true
}

#[cfg(target_os = "linux")]
fn single_instance_lock_path() -> std::path::PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("com", "localrecord", "LocalRecord") {
        return dirs.cache_dir().join("localrecord.lock");
    }
    std::env::temp_dir().join("localrecord.lock")
}

#[cfg(target_os = "linux")]
fn try_lock_file(file: &std::fs::File) -> bool {
    use std::os::unix::io::AsRawFd;

    let fd = file.as_raw_fd();
    unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) == 0 }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn acquire_single_instance() -> bool {
    true
}

#[cfg(target_os = "linux")]
fn init_linux_gtk() {
    if gtk::init().is_err() {
        eprintln!("Failed to initialize GTK");
        std::process::exit(1);
    }
}

#[cfg(target_os = "linux")]
fn pump_gtk_events() {
    while gtk::events_pending() {
        gtk::main_iteration();
    }
}
