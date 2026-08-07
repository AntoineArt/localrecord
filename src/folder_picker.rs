#[cfg(windows)]
mod imp;

#[cfg(target_os = "linux")]
mod imp_linux;

#[cfg(windows)]
pub use imp::pick_folder;

#[cfg(target_os = "linux")]
pub use imp_linux::pick_folder;

#[cfg(not(any(windows, target_os = "linux")))]
pub fn pick_folder(_initial: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    None
}
