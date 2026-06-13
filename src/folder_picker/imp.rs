use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::core::{Interface, PCWSTR};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName,
    FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};

use crate::log;

pub fn pick_folder(initial: Option<&Path>) -> Option<PathBuf> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let open_dialog: IFileOpenDialog =
            match CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) {
                Ok(dialog) => dialog,
                Err(err) => {
                    log::error(&format!("Failed to create folder picker dialog: {err}"));
                    return None;
                }
            };

        let dialog: IFileDialog = match open_dialog.cast() {
            Ok(dialog) => dialog,
            Err(err) => {
                log::error(&format!("Failed to configure folder picker dialog: {err}"));
                return None;
            }
        };

        let options = FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM;
        if dialog.SetOptions(options).is_err() {
            log::error("Failed to configure folder picker options");
            return None;
        }

        if let Some(path) = initial {
            if let Ok(item) = shell_item_from_path(path) {
                let _ = dialog.SetFolder(&item);
            }
        }

        if dialog.Show(None).is_err() {
            return None;
        }

        let item = match dialog.GetResult() {
            Ok(item) => item,
            Err(err) => {
                log::error(&format!("Folder picker returned no selection: {err}"));
                return None;
            }
        };

        path_from_shell_item(&item)
    }
}

unsafe fn shell_item_from_path(path: &Path) -> windows::core::Result<IShellItem> {
    let wide = path_to_wide(path);
    SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)
}

unsafe fn path_from_shell_item(item: &IShellItem) -> Option<PathBuf> {
    let wide_path = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
    let path = wide_path.to_string().ok()?;
    if path.is_empty() {
        return None;
    }
    Some(PathBuf::from(path))
}

fn path_to_wide(path: &Path) -> Vec<u16> {
    OsStr::new(path.as_os_str())
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
