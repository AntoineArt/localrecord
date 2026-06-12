use std::path::Path;

use windows::core::HSTRING;
use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

use crate::log;

const APP_ID: &str = "com.localrecord.LocalRecord";

pub fn init() {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
        let app_id = HSTRING::from(APP_ID);
        if let Err(err) = SetCurrentProcessExplicitAppUserModelID(&app_id) {
            log::error(&format!("Failed to set AppUserModelID: {err}"));
        }
    }
}

/// Shows a Windows toast when a recording finishes. Returns true if the toast was shown.
pub fn show_recording_saved(path: &Path, clipboard_ok: bool) -> bool {
    let headline = if clipboard_ok {
        "Recording saved and copied to clipboard"
    } else {
        "Recording saved (clipboard copy failed)"
    };
    let path_text = path.display().to_string();

    match show_toast("LocalRecord", headline, &path_text) {
        Ok(()) => true,
        Err(err) => {
            log::error(&format!("Toast notification failed: {err}"));
            false
        }
    }
}

fn show_toast(title: &str, line1: &str, line2: &str) -> Result<(), String> {
    let xml = format!(
        r#"<toast><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text><text>{}</text></binding></visual></toast>"#,
        escape_xml(title),
        escape_xml(line1),
        escape_xml(line2),
    );

    let document = XmlDocument::new().map_err(|e| e.to_string())?;
    document
        .LoadXml(&HSTRING::from(xml))
        .map_err(|e| e.to_string())?;

    let toast = ToastNotification::CreateToastNotification(&document).map_err(|e| e.to_string())?;
    let notifier =
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))
            .or_else(|_| ToastNotificationManager::CreateToastNotifier())
            .map_err(|e| e.to_string())?;

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
