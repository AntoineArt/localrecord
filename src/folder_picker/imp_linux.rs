use std::path::{Path, PathBuf};

pub fn pick_folder(initial: Option<&Path>) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title("Choose recordings folder");

    if let Some(path) = initial {
        dialog = dialog.set_directory(path);
    }

    dialog.pick_folder()
}
