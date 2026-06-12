#[cfg(windows)]
#[path = "overlay/imp.rs"]
mod imp;

#[cfg(windows)]
pub use imp::{OverlayHandle, RecordingOverlay};

#[cfg(not(windows))]
pub struct OverlayHandle;

#[cfg(not(windows))]
pub struct RecordingOverlay;

#[cfg(not(windows))]
impl RecordingOverlay {
    pub fn show() -> OverlayHandle {
        OverlayHandle
    }
}

#[cfg(not(windows))]
impl Drop for OverlayHandle {
    fn drop(&mut self) {}
}
