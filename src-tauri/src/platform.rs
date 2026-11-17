//! Small Windows helpers.

use windows::{
    Win32::{
        System::Registry::{HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey, RegCreateKeyExW, RegSetValueExW},
        UI::{Shell::{SetCurrentProcessExplicitAppUserModelID, ShellExecuteW}, WindowsAndMessaging::SW_SHOWNORMAL},
    },
    core::{HSTRING, PCWSTR, w},
};

use crate::rules;

pub const AUMID: &str = "app.wach.desktop";

/// Open a link in the default browser. Only plain http(s) links get here.
pub fn open_in_browser(url: &str) {
    let Ok(parsed) = tauri::Url::parse(url) else { return };
    let Some(safe) = rules::external_link(&parsed) else { return };
    let target = HSTRING::from(safe);
    unsafe {
        ShellExecuteW(None, w!("open"), &target, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

/// Give the process our AppUserModelID and register it under HKCU so Windows
/// shows our toasts (name + icon) even before the installer adds a shortcut.
pub fn register_app_id(icon_png: Option<&std::path::Path>) {
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(AUMID));
    }
    let key_path = HSTRING::from(format!("Software\\Classes\\AppUserModelId\\{AUMID}"));
    let mut key = HKEY::default();
    let created = unsafe {
        RegCreateKeyExW(HKEY_CURRENT_USER, &key_path, None, None, REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut key, None)
    };
    if created.is_err() {
        return;
    }
    set_string(key, "DisplayName", "WACH");
    if let Some(icon) = icon_png {
        set_string(key, "IconUri", &icon.to_string_lossy());
    }
    unsafe {
        let _ = RegCloseKey(key);
    }
}

fn set_string(key: HKEY, name: &str, value: &str) {
    let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };
    unsafe {
        let _ = RegSetValueExW(key, &HSTRING::from(name), None, REG_SZ, Some(bytes));
    }
}
