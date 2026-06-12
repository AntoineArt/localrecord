use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassW, HWND_MESSAGE, WNDCLASSW,
};

const CLASS_NAME: &str = "LocalRecordHidden";

pub fn create() -> HWND {
    unsafe {
        register_class();
        CreateWindowExW(
            Default::default(),
            PCWSTR(class_name_wide().as_ptr()),
            PCWSTR::null(),
            Default::default(),
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            None,
            GetModuleHandleW(None).expect("module handle"),
            None,
        )
        .expect("hidden window")
    }
}

unsafe fn register_class() {
    static REGISTERED: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    if REGISTERED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }

    let wc = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: GetModuleHandleW(None).unwrap().into(),
        lpszClassName: PCWSTR(class_name_wide().as_ptr()),
        ..Default::default()
    };
    RegisterClassW(&wc);
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn class_name_wide() -> Vec<u16> {
    OsStr::new(CLASS_NAME)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
