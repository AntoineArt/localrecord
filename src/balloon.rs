use std::sync::Mutex;

use windows::Win32::Foundation::{BOOL, HWND, LPARAM, TRUE};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconGetRect, Shell_NotifyIconW, NIF_INFO, NIIF_INFO, NIM_MODIFY, NIM_SETVERSION,
    NOTIFYICONDATAW, NOTIFYICONIDENTIFIER, NOTIFYICON_VERSION_4,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowThreadProcessId,
};

use crate::log;

static TRAY_TARGET: Mutex<Option<TrayTarget>> = Mutex::new(None);
static VERSION_SET: Mutex<bool> = Mutex::new(false);

#[derive(Clone, Copy)]
struct TrayTarget {
    hwnd: isize,
    id: u32,
}

/// Shows a tray balloon notification (works for tray apps without toast setup).
pub fn show(title: &str, message: &str) -> Result<(), String> {
    let target = resolve_tray_target()?;
    show_on_target(target, title, message)
}

fn show_on_target(target: TrayTarget, title: &str, message: &str) -> Result<(), String> {
    let hwnd = HWND(target.hwnd as isize as *mut _);
    ensure_notifyicon_version(hwnd, target.id)?;

    let title_wide = truncate_wide(title, 63);
    let message_wide = truncate_wide(message, 255);

    let mut info_title = [0u16; 64];
    let mut info_text = [0u16; 256];
    copy_wide_into(&title_wide, &mut info_title);
    copy_wide_into(&message_wide, &mut info_text);

    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: target.id,
        // Only NIF_INFO — never NIF_TIP without szTip (that corrupts the tray icon / menu).
        uFlags: NIF_INFO,
        dwInfoFlags: NIIF_INFO,
        szInfoTitle: info_title,
        szInfo: info_text,
        ..Default::default()
    };

    unsafe {
        if Shell_NotifyIconW(NIM_MODIFY, &mut nid as *mut _) != TRUE {
            return Err(format!(
                "Shell_NotifyIconW balloon failed: {}",
                std::io::Error::last_os_error()
            ));
        }
    }

    Ok(())
}

fn ensure_notifyicon_version(hwnd: HWND, id: u32) -> Result<(), String> {
    let mut set = VERSION_SET
        .lock()
        .map_err(|_| "Notification version lock poisoned".to_string())?;
    if *set {
        return Ok(());
    }

    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: id,
        ..Default::default()
    };
    nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;

    unsafe {
        if Shell_NotifyIconW(NIM_SETVERSION, &mut nid as *mut _) != TRUE {
            return Err(format!(
                "Shell_NotifyIconW set version failed: {}",
                std::io::Error::last_os_error()
            ));
        }
    }

    *set = true;
    Ok(())
}

fn resolve_tray_target() -> Result<TrayTarget, String> {
    if let Ok(guard) = TRAY_TARGET.lock() {
        if let Some(target) = *guard {
            return Ok(target);
        }
    }

    let target = discover_tray_target().ok_or("Could not find tray icon window")?;

    if let Ok(mut guard) = TRAY_TARGET.lock() {
        *guard = Some(target);
    }

    Ok(target)
}

fn discover_tray_target() -> Option<TrayTarget> {
    let hwnd = find_tray_window()?;
    let id = find_tray_icon_id(hwnd)?;
    log::info("Discovered tray icon for notifications");
    Some(TrayTarget {
        hwnd: hwnd.0 as isize,
        id,
    })
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

fn find_tray_icon_id(hwnd: HWND) -> Option<u32> {
    for id in 1..=32 {
        let identifier = NOTIFYICONIDENTIFIER {
            cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: hwnd,
            uID: id,
            ..Default::default()
        };

        unsafe {
            if Shell_NotifyIconGetRect(&identifier).is_ok() {
                return Some(id);
            }
        }
    }
    None
}

fn truncate_wide(text: &str, max_chars: usize) -> Vec<u16> {
    text.encode_utf16()
        .take(max_chars)
        .chain(std::iter::once(0))
        .collect()
}

fn copy_wide_into(source: &[u16], dest: &mut [u16]) {
    let copy_len = source.len().min(dest.len());
    dest[..copy_len].copy_from_slice(&source[..copy_len]);
    if copy_len < dest.len() {
        dest[copy_len] = 0;
    }
}
