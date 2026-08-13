use std::env;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;

use windows::core::PCWSTR;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "LocalRecord";

pub fn exe_path() -> Option<PathBuf> {
    env::current_exe().ok()
}

pub fn is_enabled() -> bool {
    read_run_value().is_some()
}

pub fn enable() -> Result<(), String> {
    let exe = exe_path().ok_or("Could not resolve executable path")?;
    let command = format!("\"{}\"", exe.display());
    write_run_value(&command)
}

pub fn disable() -> Result<(), String> {
    delete_run_value()
}

pub fn ensure_enabled() {
    if !is_enabled() {
        let _ = enable();
    }
}

fn read_run_value() -> Option<String> {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(wide(RUN_KEY).as_ptr()),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        )
        .is_err()
        {
            return None;
        }

        let mut buf = [0u16; 512];
        let mut size = (buf.len() * 2) as u32;
        if RegQueryValueExW(
            key,
            PCWSTR(wide(VALUE_NAME).as_ptr()),
            None,
            None,
            Some(buf.as_mut_ptr() as *mut u8),
            Some(&mut size),
        )
        .is_err()
        {
            let _ = RegCloseKey(key);
            return None;
        }
        let _ = RegCloseKey(key);

        let len = (size as usize / 2).saturating_sub(1);
        Some(String::from_utf16_lossy(&buf[..len.min(buf.len())]))
    }
}

fn write_run_value(command: &str) -> Result<(), String> {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(wide(RUN_KEY).as_ptr()),
            0,
            KEY_SET_VALUE,
            &mut key,
        )
        .is_err()
        {
            return Err("RegOpenKeyExW failed".to_string());
        }

        let wide_cmd = wide(command);
        let bytes = wide_to_bytes(&wide_cmd);
        if RegSetValueExW(
            key,
            PCWSTR(wide(VALUE_NAME).as_ptr()),
            0,
            REG_SZ,
            Some(&bytes),
        )
        .is_err()
        {
            let _ = RegCloseKey(key);
            return Err("RegSetValueExW failed".to_string());
        }
        let _ = RegCloseKey(key);
        Ok(())
    }
}

fn delete_run_value() -> Result<(), String> {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(wide(RUN_KEY).as_ptr()),
            0,
            KEY_SET_VALUE,
            &mut key,
        )
        .is_err()
        {
            return Err("RegOpenKeyExW failed".to_string());
        }
        if RegDeleteValueW(key, PCWSTR(wide(VALUE_NAME).as_ptr())).is_err() {
            let _ = RegCloseKey(key);
            return Err("RegDeleteValueW failed".to_string());
        }
        let _ = RegCloseKey(key);
        Ok(())
    }
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn wide_to_bytes(wide: &[u16]) -> Vec<u8> {
    wide.iter().flat_map(|c| c.to_le_bytes()).collect()
}
