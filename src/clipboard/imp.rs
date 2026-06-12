use std::mem;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use std::thread;
use std::time::Duration;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::OleInitialize;
use windows::Win32::System::Ole::{CF_HDROP, CF_WAVE};

pub fn init_clipboard() {
    unsafe {
        let _ = OleInitialize(None);
    }
}

pub fn copy_recording_to_clipboard(
    wav_bytes: Option<&[u8]>,
    file_path: &Path,
    owner: HWND,
) -> Result<(), String> {
    if let Some(wav_bytes) = wav_bytes {
        if wav_bytes.is_empty() {
            return Err("No audio data to copy".to_string());
        }
    }

    for attempt in 0..10 {
        if attempt > 0 {
            thread::sleep(Duration::from_millis(40 * attempt));
        }

        unsafe {
            if OpenClipboard(owner).is_err() {
                continue;
            }

            let result = (|| -> Result<(), String> {
                EmptyClipboard().map_err(|e| format!("EmptyClipboard failed: {e}"))?;

                if let Some(wav_bytes) = wav_bytes {
                    let wave_registered = register_wave_format()?;
                    let wave_handle = alloc_moveable(wav_bytes)?;
                    SetClipboardData(wave_registered, HANDLE(wave_handle.0))
                        .map_err(|e| format!("SetClipboardData(custom WAVE) failed: {e}"))?;

                    if wave_registered != u32::from(CF_WAVE.0) {
                        let standard_handle = alloc_moveable(wav_bytes)?;
                        SetClipboardData(u32::from(CF_WAVE.0), HANDLE(standard_handle.0))
                            .map_err(|e| format!("SetClipboardData(CF_WAVE) failed: {e}"))?;
                    }
                }

                let drop_data = build_hdrop_data(file_path)?;
                let drop_handle = alloc_moveable(&drop_data)?;
                SetClipboardData(u32::from(CF_HDROP.0), HANDLE(drop_handle.0))
                    .map_err(|e| format!("SetClipboardData(CF_HDROP) failed: {e}"))?;

                Ok(())
            })();

            CloseClipboard().ok();
            if result.is_ok() {
                if wav_bytes.is_some() {
                    crate::log::info("Recording copied to clipboard (audio + file)");
                } else {
                    crate::log::info("Recording file copied to clipboard");
                }
                return Ok(());
            }
        }
    }

    Err("Could not open clipboard".to_string())
}

unsafe fn register_wave_format() -> Result<u32, String> {
    let name: Vec<u16> = "WAVE\0".encode_utf16().collect();
    let format = RegisterClipboardFormatW(PCWSTR(name.as_ptr()));
    if format == 0 {
        return Err("RegisterClipboardFormatW(WAVE) failed".to_string());
    }
    Ok(format)
}

unsafe fn alloc_moveable(data: &[u8]) -> Result<HGLOBAL, String> {
    let handle =
        GlobalAlloc(GMEM_MOVEABLE, data.len()).map_err(|e| format!("GlobalAlloc failed: {e}"))?;
    let locked = GlobalLock(handle);
    if locked.is_null() {
        return Err("GlobalLock failed".to_string());
    }
    ptr::copy_nonoverlapping(data.as_ptr(), locked as *mut u8, data.len());
    GlobalUnlock(handle).ok();
    Ok(handle)
}

fn build_hdrop_data(file_path: &Path) -> Result<Vec<u8>, String> {
    let path = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.to_path_buf());
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .chain(std::iter::once(0))
        .collect();

    #[repr(C)]
    struct DropFiles {
        p_files: u32,
        pt_x: i32,
        pt_y: i32,
        f_nc: i32,
        f_wide: i32,
    }

    let header_size = mem::size_of::<DropFiles>();
    let mut data = vec![0u8; header_size + wide.len() * 2];
    let header = DropFiles {
        p_files: header_size as u32,
        pt_x: 0,
        pt_y: 0,
        f_nc: 0,
        f_wide: 1,
    };

    unsafe {
        ptr::copy_nonoverlapping(
            &header as *const DropFiles as *const u8,
            data.as_mut_ptr(),
            header_size,
        );
    }

    let path_bytes =
        unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };
    data[header_size..header_size + path_bytes.len()].copy_from_slice(path_bytes);
    Ok(data)
}
