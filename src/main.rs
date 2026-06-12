#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod balloon;

mod app;
mod audio;
mod clipboard;
mod config;
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

use app::App;

enum UserEvent {
    Menu(tray_icon::menu::MenuEvent),
    Tray(tray_icon::TrayIconEvent),
}

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
    if !acquire_single_instance() {
        std::process::exit(0);
    }

    clipboard::init_clipboard();
    notification::init();
    let clipboard_owner = hidden_window::create();

    let event_loop = winit::event_loop::EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("event loop");

    let proxy = event_loop.create_proxy();
    tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Tray(event));
    }));

    let proxy = event_loop.create_proxy();
    tray_icon::menu::MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Menu(event));
    }));

    let mut app = App::new(clipboard_owner).expect("initialize LocalRecord");
    log::info("LocalRecord started");

    let _ = event_loop.run(move |event, elwt| {
        match event {
            winit::event::Event::UserEvent(UserEvent::Menu(menu_event)) => {
                app.handle_menu_event(&menu_event);
            }
            winit::event::Event::UserEvent(UserEvent::Tray(tray_event)) => {
                app.handle_tray_event(&tray_event);
            }
            winit::event::Event::AboutToWait => app.poll_hotkey(),
            winit::event::Event::WindowEvent {
                event: winit::event::WindowEvent::CloseRequested,
                ..
            } => elwt.exit(),
            _ => {}
        }
    });
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
