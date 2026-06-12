use std::sync::mpsc::sync_channel;
use std::thread;

use global_hotkey::hotkey::HotKey;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::platform::windows::EventLoopBuilderExtWindows;

pub fn pick_hotkey(current: &str) -> Option<String> {
    let (tx, rx) = sync_channel::<Option<String>>(1);
    let current = current.to_string();

    thread::Builder::new()
        .name("hotkey-picker".into())
        .spawn(move || {
            let result = run_picker(&current);
            let _ = tx.send(result);
        })
        .ok()?;

    rx.recv().ok().flatten()
}

fn run_picker(current: &str) -> Option<String> {
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().ok()?;

    let title = format!("Press a new shortcut (current: {current})");

    let mut selected: Option<String> = None;
    let mut modifiers = ModifiersState::empty();

    event_loop
        .run(|event, elwt| match event {
            Event::NewEvents(winit::event::StartCause::Init) => {
                let attrs = winit::window::WindowAttributes::default()
                    .with_title(&title)
                    .with_inner_size(LogicalSize::new(420.0, 120.0))
                    .with_resizable(false)
                    .with_active(true);

                if let Ok(window) = elwt.create_window(attrs) {
                    window.set_visible(true);
                    window.focus_window();
                    let _ = window.request_user_attention(Some(
                        winit::window::UserAttentionType::Informational,
                    ));
                }
            }
            Event::WindowEvent {
                event: WindowEvent::ModifiersChanged(m),
                ..
            } => {
                modifiers = m.state();
            }
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { event, .. },
                ..
            } => {
                if event.state != ElementState::Pressed {
                    return;
                }

                if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                    elwt.exit();
                    return;
                }

                if let Some(binding) = binding_from_key_event(&event.logical_key, modifiers) {
                    if binding.parse::<HotKey>().is_ok() {
                        selected = Some(binding);
                        elwt.exit();
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => elwt.exit(),
            _ => {}
        })
        .ok()?;

    selected
}

fn binding_from_key_event(key: &Key, modifiers: ModifiersState) -> Option<String> {
    if is_modifier_key(key) {
        return None;
    }

    let key_token = key_to_token(key)?;
    let mut parts = Vec::new();

    if modifiers.control_key() {
        parts.push("Ctrl".to_string());
    }
    if modifiers.alt_key() {
        parts.push("Alt".to_string());
    }
    if modifiers.shift_key() {
        parts.push("Shift".to_string());
    }
    if modifiers.super_key() {
        parts.push("Win".to_string());
    }

    if parts.is_empty() {
        return None;
    }

    parts.push(key_token);
    Some(parts.join("+"))
}

fn is_modifier_key(key: &Key) -> bool {
    matches!(
        key,
        Key::Named(
            NamedKey::Shift
                | NamedKey::Control
                | NamedKey::Alt
                | NamedKey::Meta
                | NamedKey::AltGraph
        )
    )
}

fn key_to_token(key: &Key) -> Option<String> {
    match key {
        Key::Character(text) => {
            let upper = text.to_uppercase();
            if upper.len() == 1 {
                Some(upper)
            } else {
                None
            }
        }
        Key::Named(named) => Some(match named {
            NamedKey::Space => "Space".to_string(),
            NamedKey::Enter => "Enter".to_string(),
            NamedKey::Tab => "Tab".to_string(),
            NamedKey::Backspace => "Backspace".to_string(),
            NamedKey::Delete => "Delete".to_string(),
            NamedKey::Insert => "Insert".to_string(),
            NamedKey::Home => "Home".to_string(),
            NamedKey::End => "End".to_string(),
            NamedKey::PageUp => "PageUp".to_string(),
            NamedKey::PageDown => "PageDown".to_string(),
            NamedKey::ArrowUp => "Up".to_string(),
            NamedKey::ArrowDown => "Down".to_string(),
            NamedKey::ArrowLeft => "Left".to_string(),
            NamedKey::ArrowRight => "Right".to_string(),
            NamedKey::F1 => "F1".to_string(),
            NamedKey::F2 => "F2".to_string(),
            NamedKey::F3 => "F3".to_string(),
            NamedKey::F4 => "F4".to_string(),
            NamedKey::F5 => "F5".to_string(),
            NamedKey::F6 => "F6".to_string(),
            NamedKey::F7 => "F7".to_string(),
            NamedKey::F8 => "F8".to_string(),
            NamedKey::F9 => "F9".to_string(),
            NamedKey::F10 => "F10".to_string(),
            NamedKey::F11 => "F11".to_string(),
            NamedKey::F12 => "F12".to_string(),
            _ => return None,
        }),
        _ => None,
    }
}
