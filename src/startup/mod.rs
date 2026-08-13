#[cfg(windows)]
mod imp;

#[cfg(target_os = "linux")]
mod imp_linux;

#[cfg(windows)]
pub use imp::*;

#[cfg(target_os = "linux")]
pub use imp_linux::*;

#[cfg(not(any(windows, target_os = "linux")))]
pub fn is_enabled() -> bool {
    false
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn enable() -> Result<(), String> {
    Err("Startup is not supported on this platform".to_string())
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn disable() -> Result<(), String> {
    Err("Startup is not supported on this platform".to_string())
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn ensure_enabled() {}
