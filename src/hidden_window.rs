#[cfg(windows)]
mod imp;

#[cfg(windows)]
pub use imp::create;

#[cfg(not(windows))]
pub fn create() {}

#[cfg(windows)]
pub type ClipboardOwner = windows::Win32::Foundation::HWND;

#[cfg(not(windows))]
pub type ClipboardOwner = ();
