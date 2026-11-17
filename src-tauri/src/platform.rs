//! Small Windows helpers.

use windows::{
    Win32::{
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW,
            RegDeleteKeyValueW, RegGetValueW, RegSetValueExW,
        },
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

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "WACH";
/// Command-line flag: start hidden in the tray (used by the Run entry).
pub const HIDDEN_FLAG: &str = "--hidden";

/// The command line Windows runs at sign-in for this exe.
fn autostart_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\" {HIDDEN_FLAG}", exe.display()))
}

/// What the Run entry says now, if there is one.
fn autostart_value() -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_KEY),
            &HSTRING::from(RUN_VALUE),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if ok.is_err() {
        return None;
    }
    let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

/// On only when the Run entry points at this very exe (a debug build or an
/// old install folder does not count).
pub fn autostart_enabled() -> bool {
    autostart_value().is_some_and(|v| Some(v) == autostart_command())
}

pub fn set_autostart(on: bool) -> Result<(), String> {
    if on {
        let cmd = autostart_command().ok_or("Can't find WACH's own path.")?;
        let mut key = HKEY::default();
        unsafe { RegCreateKeyExW(HKEY_CURRENT_USER, &HSTRING::from(RUN_KEY), None, None, REG_OPTION_NON_VOLATILE, KEY_WRITE | KEY_READ, None, &mut key, None) }
            .ok()
            .map_err(|e| e.to_string())?;
        set_string(key, RUN_VALUE, &cmd);
        unsafe {
            let _ = RegCloseKey(key);
        }
        if !autostart_enabled() {
            return Err("Windows did not save the start-up entry.".into());
        }
    } else if autostart_enabled() {
        // Only remove our own entry, never one another WACH copy wrote.
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(RUN_KEY), &HSTRING::from(RUN_VALUE)) }
            .ok()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn set_string(key: HKEY, name: &str, value: &str) {
    let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };
    unsafe {
        let _ = RegSetValueExW(key, &HSTRING::from(name), None, REG_SZ, Some(bytes));
    }
}
