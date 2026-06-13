use std::sync::Mutex;

use windows::Win32::Foundation::{BOOL, HWND, LPARAM, WPARAM, TRUE};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowThreadProcessId, PostMessageW, SetForegroundWindow,
};

static TRAY_TARGET: Mutex<Option<TrayTarget>> = Mutex::new(None);

#[derive(Clone, Copy)]
struct TrayTarget {
    hwnd: isize,
}

/// Clears cached tray window handle (e.g. after icon re-registration).
pub fn invalidate_tray_target() {
    if let Ok(mut guard) = TRAY_TARGET.lock() {
        *guard = None;
    }
}

/// Helps the tray context menu receive focus after a toast notification.
pub fn focus_tray_for_menu() {
    let Some(target) = discover_tray_target() else {
        return;
    };

    unsafe {
        let hwnd = HWND(target.hwnd as *mut _);
        let _ = SetForegroundWindow(hwnd);
        let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
    }
}

const WM_NULL: u32 = 0;

fn discover_tray_target() -> Option<TrayTarget> {
    if let Ok(guard) = TRAY_TARGET.lock() {
        if let Some(target) = *guard {
            return Some(target);
        }
    }

    let hwnd = find_tray_window()?;
    let target = TrayTarget {
        hwnd: hwnd.0 as isize,
    };

    if let Ok(mut guard) = TRAY_TARGET.lock() {
        *guard = Some(target);
    }

    Some(target)
}

fn find_tray_window() -> Option<HWND> {
    let mut found = None;
    let current_pid = std::process::id();

    unsafe {
        let _ = EnumWindows(
            Some(enum_tray_window),
            LPARAM(&mut found as *mut _ as isize),
        );
    }

    found.filter(|hwnd| window_belongs_to_process(*hwnd, current_pid))
}

unsafe extern "system" fn enum_tray_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let found = &mut *(lparam.0 as *mut Option<HWND>);
    if found.is_some() {
        return TRUE;
    }

    let mut class_name = [0u16; 64];
    let len = GetClassNameW(hwnd, &mut class_name);
    if len == 0 {
        return TRUE;
    }

    let name = String::from_utf16_lossy(&class_name[..len as usize]);
    if name == "tray_icon_app" {
        *found = Some(hwnd);
    }

    TRUE
}

fn window_belongs_to_process(hwnd: HWND, pid: u32) -> bool {
    unsafe {
        let mut window_pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut window_pid));
        window_pid == pid
    }
}
