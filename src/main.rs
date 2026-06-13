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
mod hotkey_picker;
mod icon;
mod log;
mod notification;
mod settings;
mod startup;
mod tray;

fn main() {
    #[cfg(not(windows))]
    {
        eprintln!("LocalRecord is a Windows application.");
        eprintln!("Build with: cargo build --release --target x86_64-pc-windows-gnu");
        std::process::exit(1);
    }

    #[cfg(windows)]
    run();
}

#[cfg(windows)]
fn run() {
    use app::{App, UserEvent};
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::ActiveEventLoop;
    use winit::window::WindowId;

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

        fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
            self.app.poll_hotkey();
        }
    }

    if !acquire_single_instance() {
        std::process::exit(0);
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
