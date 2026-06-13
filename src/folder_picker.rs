#[cfg(windows)]
mod imp;

#[cfg(windows)]
pub use imp::pick_folder;

#[cfg(not(windows))]
pub fn pick_folder(_initial: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    None
}
