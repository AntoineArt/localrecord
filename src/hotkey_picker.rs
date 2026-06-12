#[cfg(windows)]
mod imp;

#[cfg(windows)]
pub use imp::pick_hotkey;
