use std::cell::Cell;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::mpsc::sync_channel;
use std::thread;

use global_hotkey::hotkey::HotKey;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
    VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetSystemMetrics,
    GetWindowLongPtrW, PostQuitMessage, RegisterClassW, SetForegroundWindow, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CREATESTRUCTW,
    CW_USEDEFAULT, GWLP_USERDATA, MSG, SM_CXSCREEN, SM_CYSCREEN, SWP_NOZORDER, SW_SHOWNORMAL,
    WINDOW_EX_STYLE, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_KEYDOWN, WM_SYSKEYDOWN, WNDCLASSW,
    WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW, WS_EX_TOPMOST, WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
    HWND_TOP,
};

use crate::log;

const CLASS_NAME: &str = "LocalRecordHotkeyPicker";
const WINDOW_WIDTH: i32 = 520;
const WINDOW_HEIGHT: i32 = 200;

struct PickerContext {
    current: String,
    selected: Cell<Option<String>>,
}

pub fn pick_hotkey(current: &str) -> Option<String> {
    let (tx, rx) = sync_channel::<Option<String>>(1);
    let current = current.to_string();

    let handle = thread::Builder::new()
        .name("hotkey-picker".into())
        .spawn(move || {
            let result = run_picker(&current);
            let _ = tx.send(result);
        });

    let handle = match handle {
        Ok(handle) => handle,
        Err(err) => {
            log::error(&format!("Failed to start shortcut picker thread: {err}"));
            return None;
        }
    };

    let result = match rx.recv() {
        Ok(result) => result,
        Err(err) => {
            log::error(&format!("Shortcut picker channel closed: {err}"));
            None
        }
    };
    let _ = handle.join();
    result
}

fn run_picker(current: &str) -> Option<String> {
    unsafe {
        register_picker_class();

        let context = Box::new(PickerContext {
            current: current.to_string(),
            selected: Cell::new(None),
        });
        let context_ptr = Box::into_raw(context);

        let title = to_wide("Change LocalRecord Shortcut");
        let hwnd = match CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_APPWINDOW,
            PCWSTR(class_name_wide().as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            None,
            None,
            GetModuleHandleW(None).expect("module handle"),
            Some(context_ptr as _),
        ) {
            Ok(hwnd) => hwnd,
            Err(err) => {
                log::error(&format!("Shortcut picker window failed: {err}"));
                let _ = Box::from_raw(context_ptr);
                return None;
            }
        };

        center_window(hwnd);
        let _ = SetWindowPos(hwnd, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOZORDER);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let context = Box::from_raw(context_ptr);
        context.selected.take()
    }
}

unsafe fn register_picker_class() {
    static REGISTERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if REGISTERED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }

    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(picker_wnd_proc),
        hInstance: GetModuleHandleW(None).unwrap().into(),
        lpszClassName: PCWSTR(class_name_wide().as_ptr()),
        ..Default::default()
    };
    let _ = RegisterClassW(&wc);
}

unsafe extern "system" fn picker_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let create = lparam.0 as *const CREATESTRUCTW;
            if create.is_null() {
                return LRESULT(-1);
            }

            let context = &mut *((*create).lpCreateParams as *mut PickerContext);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, context as *mut _ as _);

            let text = format!(
                "Press a new shortcut combination.\r\n\r\n\
                 Current shortcut: {}\r\n\r\n\
                 Include at least one modifier (Ctrl, Alt, Shift, or Win).\r\n\
                 Press Esc to cancel.",
                context.current
            );

            let label = to_wide(&text);
            let _ = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                PCWSTR(to_wide("STATIC").as_ptr()),
                PCWSTR(label.as_ptr()),
                WS_CHILD | WS_VISIBLE,
                16,
                16,
                WINDOW_WIDTH - 32,
                WINDOW_HEIGHT - 48,
                hwnd,
                None,
                GetModuleHandleW(None).unwrap(),
                None,
            );

            LRESULT(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let vk = wparam.0 as u16;
            if vk == VK_ESCAPE.0 as u16 {
                PostQuitMessage(0);
                return LRESULT(0);
            }

            if let Some(binding) = binding_from_vk(vk) {
                if binding.parse::<HotKey>().is_ok() {
                    if let Some(context) = context_from_hwnd(hwnd) {
                        context.selected.set(Some(binding));
                    }
                    PostQuitMessage(0);
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_DESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn context_from_hwnd(hwnd: HWND) -> Option<&'static PickerContext> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    if ptr == 0 {
        return None;
    }
    Some(&*(ptr as *const PickerContext))
}

unsafe fn center_window(hwnd: HWND) {
    let screen_w = GetSystemMetrics(SM_CXSCREEN);
    let screen_h = GetSystemMetrics(SM_CYSCREEN);
    let x = (screen_w - WINDOW_WIDTH) / 2;
    let y = (screen_h - WINDOW_HEIGHT) / 2;
    let _ = SetWindowPos(
        hwnd,
        None,
        x,
        y,
        WINDOW_WIDTH,
        WINDOW_HEIGHT,
        SWP_NOZORDER,
    );
}

fn binding_from_vk(vk: u16) -> Option<String> {
    if is_modifier_vk(vk) {
        return None;
    }

    let key_token = vk_to_token(vk)?;
    let mut parts = Vec::new();

    if modifier_pressed(VK_CONTROL) || modifier_pressed(VK_LCONTROL) || modifier_pressed(VK_RCONTROL)
    {
        parts.push("Ctrl".to_string());
    }
    if modifier_pressed(VK_MENU) || modifier_pressed(VK_LMENU) || modifier_pressed(VK_RMENU) {
        parts.push("Alt".to_string());
    }
    if modifier_pressed(VK_SHIFT) || modifier_pressed(VK_LSHIFT) || modifier_pressed(VK_RSHIFT) {
        parts.push("Shift".to_string());
    }
    if modifier_pressed(VK_LWIN) || modifier_pressed(VK_RWIN) {
        parts.push("Win".to_string());
    }

    if parts.is_empty() {
        return None;
    }

    parts.push(key_token);
    Some(parts.join("+"))
}

fn modifier_pressed(vk: VIRTUAL_KEY) -> bool {
    unsafe { (GetAsyncKeyState(vk.0 as i32) as u16) & 0x8000 != 0 }
}

fn is_modifier_vk(vk: u16) -> bool {
    matches!(
        vk,
        x if x == VK_SHIFT.0 as u16
            || x == VK_LSHIFT.0 as u16
            || x == VK_RSHIFT.0 as u16
            || x == VK_CONTROL.0 as u16
            || x == VK_LCONTROL.0 as u16
            || x == VK_RCONTROL.0 as u16
            || x == VK_MENU.0 as u16
            || x == VK_LMENU.0 as u16
            || x == VK_RMENU.0 as u16
            || x == VK_LWIN.0 as u16
            || x == VK_RWIN.0 as u16
    )
}

fn vk_to_token(vk: u16) -> Option<String> {
    if (0x41..=0x5A).contains(&vk) {
        return Some(((vk as u8) as char).to_string());
    }
    if (0x30..=0x39).contains(&vk) {
        return Some(((vk as u8) as char).to_string());
    }

    Some(match vk {
        0x20 => "Space".to_string(),
        0x0D => "Enter".to_string(),
        0x09 => "Tab".to_string(),
        0x08 => "Backspace".to_string(),
        0x2E => "Delete".to_string(),
        0x2D => "Insert".to_string(),
        0x24 => "Home".to_string(),
        0x23 => "End".to_string(),
        0x21 => "PageUp".to_string(),
        0x22 => "PageDown".to_string(),
        0x26 => "Up".to_string(),
        0x28 => "Down".to_string(),
        0x25 => "Left".to_string(),
        0x27 => "Right".to_string(),
        0x70 => "F1".to_string(),
        0x71 => "F2".to_string(),
        0x72 => "F3".to_string(),
        0x73 => "F4".to_string(),
        0x74 => "F5".to_string(),
        0x75 => "F6".to_string(),
        0x76 => "F7".to_string(),
        0x77 => "F8".to_string(),
        0x78 => "F9".to_string(),
        0x79 => "F10".to_string(),
        0x7A => "F11".to_string(),
        0x7B => "F12".to_string(),
        _ => return None,
    })
}

fn class_name_wide() -> Vec<u16> {
    to_wide(CLASS_NAME)
}

fn to_wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
