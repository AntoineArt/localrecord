use std::path::Path;

use windows::core::HSTRING;
use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

use crate::balloon;
use crate::log;

pub const APP_ID: &str = "com.localrecord.LocalRecord";

pub fn init() {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
        let app_id = HSTRING::from(APP_ID);
        if let Err(err) = SetCurrentProcessExplicitAppUserModelID(&app_id) {
            log::error(&format!("Failed to set AppUserModelID: {err}"));
        }
    }

    if let Err(err) = ensure_start_menu_shortcut() {
        log::error(&format!("Start Menu shortcut for notifications: {err}"));
    }
}

/// Shows a visible notification when a recording finishes.
pub fn show_recording_saved(path: &Path, clipboard_ok: bool) -> bool {
    let headline = if clipboard_ok {
        "Recording saved and copied to clipboard"
    } else {
        "Recording saved (clipboard copy failed)"
    };
    let filename = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let folder = path
        .parent()
        .map(|parent| parent.display().to_string())
        .unwrap_or_default();

    if show_toast("LocalRecord", headline, &filename, &folder).is_ok() {
        return true;
    }

    let balloon_body = if folder.is_empty() {
        format!("{headline}\n{filename}")
    } else {
        format!("{headline}\n{filename}\n{folder}")
    };

    if balloon::show("LocalRecord", &balloon_body).is_ok() {
        return true;
    }

    log::error("All notification methods failed for recording saved");
    false
}

/// Short status notification (errors, settings changes, etc.).
pub fn show_message(headline: &str, detail: &str) -> bool {
    if show_toast("LocalRecord", headline, detail, "").is_ok() {
        return true;
    }

    let body = if detail.is_empty() {
        headline.to_string()
    } else {
        format!("{headline}\n{detail}")
    };

    balloon::show("LocalRecord", &body).is_ok()
}

fn show_toast(title: &str, headline: &str, detail: &str, subdetail: &str) -> Result<(), String> {
    let xml = if subdetail.is_empty() {
        format!(
            r#"<toast><visual><binding template="ToastGeneric"><text hint-maxLines="1">{}</text><text hint-style="subtitle">{}</text><text hint-style="body" hint-wrap="true">{}</text></binding></visual></toast>"#,
            escape_xml(title),
            escape_xml(headline),
            escape_xml(detail),
        )
    } else {
        format!(
            r#"<toast><visual><binding template="ToastGeneric"><text hint-maxLines="1">{}</text><text hint-style="subtitle">{}</text><text hint-style="body" hint-wrap="true">{}</text><text hint-style="captionSubtle" hint-wrap="true">{}</text></binding></visual></toast>"#,
            escape_xml(title),
            escape_xml(headline),
            escape_xml(detail),
            escape_xml(subdetail),
        )
    };

    let document = XmlDocument::new().map_err(|e| e.to_string())?;
    document
        .LoadXml(&HSTRING::from(xml))
        .map_err(|e| e.to_string())?;

    let toast = ToastNotification::CreateToastNotification(&document).map_err(|e| e.to_string())?;
    let notifier =
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))
            .or_else(|_| ToastNotificationManager::CreateToastNotifier())
            .map_err(|e| e.to_string())?;

    match notifier.Setting() {
        Ok(setting) if setting.0 != 1 => {
            return Err(format!("Toast notifications disabled (setting={})", setting.0));
        }
        Err(err) => log::error(&format!("Could not read toast setting: {err}")),
        _ => {}
    }

    notifier
        .Show(&toast)
        .map_err(|e| e.to_string())
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn ensure_start_menu_shortcut() -> Result<(), String> {
    use std::env;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    let appdata = env::var("APPDATA").map_err(|e| e.to_string())?;
    let shortcut_dir = Path::new(&appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs");
    std::fs::create_dir_all(&shortcut_dir).map_err(|e| e.to_string())?;

    let shortcut_path = shortcut_dir.join("LocalRecord.lnk");
    if shortcut_path.exists() {
        return Ok(());
    }

    let exe = env::current_exe().map_err(|e| e.to_string())?;
    let work_dir = exe
        .parent()
        .ok_or("Could not resolve executable directory")?
        .to_path_buf();

    let create_only = format!(
        r#"
$WshShell = New-Object -ComObject WScript.Shell
$Shortcut = $WshShell.CreateShortcut('{shortcut}')
$Shortcut.TargetPath = '{target}'
$Shortcut.WorkingDirectory = '{workdir}'
$Shortcut.Description = 'LocalRecord'
$Shortcut.Save()
"#,
        shortcut = shortcut_path.display().to_string().replace('\'', "''"),
        target = exe.display().to_string().replace('\'', "''"),
        workdir = work_dir.display().to_string().replace('\'', "''"),
    );

    let status = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &create_only,
        ])
        .creation_flags(0x08000000)
        .status()
        .map_err(|e| e.to_string())?;

    if !status.success() {
        return Err(format!("PowerShell shortcut creation failed: {status}"));
    }

    log::info(&format!(
        "Created Start Menu shortcut: {}",
        shortcut_path.display()
    ));
    Ok(())
}
