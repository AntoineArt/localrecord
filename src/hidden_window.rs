#[cfg(windows)]
mod imp;

#[cfg(windows)]
pub fn create() -> ClipboardOwner {
    imp::create().0 as isize
}

#[cfg(not(windows))]
pub fn create() {}

#[cfg(windows)]
pub type ClipboardOwner = isize;

#[cfg(not(windows))]
pub type ClipboardOwner = ();
