use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, HGDIOBJ, InvalidateRect,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, LoadCursorW, PeekMessageW,
    PM_REMOVE, RegisterClassW, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, IDC_ARROW,
    SW_SHOWNA, WM_DESTROY, WM_PAINT, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

const OVERLAY_SIZE: i32 = 18;
const MARGIN: i32 = 16;
const CLASS_NAME: &str = "LocalRecordOverlay";

static CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);

pub struct OverlayHandle {
    thread: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}

pub struct RecordingOverlay;

impl RecordingOverlay {
    pub fn show() -> OverlayHandle {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);

        let thread = thread::Builder::new()
            .name("overlay".into())
            .spawn(move || {
                let hwnd = unsafe { create_overlay_window() };

                unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOWNA);
                    let _ = InvalidateRect(hwnd, None, true);
                }

                while !stop_thread.load(Ordering::SeqCst) {
                    unsafe {
                        let mut msg = std::mem::zeroed();
                        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).into() {
                            let _ = TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }
                    thread::sleep(std::time::Duration::from_millis(16));
                }

                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            })
            .expect("overlay thread");

        OverlayHandle {
            thread: Some(thread),
            stop,
        }
    }
}

impl Drop for OverlayHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

unsafe fn create_overlay_window() -> HWND {
    register_class();

    let x = MARGIN;
    let y = MARGIN;

    CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        PCWSTR(class_name_wide().as_ptr()),
        PCWSTR(wide("REC").as_ptr()),
        WS_POPUP,
        x,
        y,
        OVERLAY_SIZE,
        OVERLAY_SIZE,
        None,
        None,
        GetModuleHandleW(None).expect("module handle"),
        None,
    )
    .expect("overlay window")
}

unsafe fn register_class() {
    if CLASS_REGISTERED.swap(true, Ordering::SeqCst) {
        return;
    }

    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: GetModuleHandleW(None).unwrap().into(),
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
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
    match msg {
        WM_PAINT => {
            let mut ps = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let brush = CreateSolidBrush(COLORREF(0x000000FF));
            let rect = windows::Win32::Foundation::RECT {
                left: 0,
                top: 0,
                right: OVERLAY_SIZE,
                bottom: OVERLAY_SIZE,
            };
            let _ = FillRect(hdc, &rect, brush);
            let _ = DeleteObject(HGDIOBJ(brush.0));
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn class_name_wide() -> Vec<u16> {
    wide(CLASS_NAME)
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
