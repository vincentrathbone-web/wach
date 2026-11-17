//! In-app updates from GitHub Releases (tauri-plugin-updater). Every update is
//! signed; the plugin refuses files whose signature does not match the public
//! key in tauri.conf.json. WACH checks at start and every 6 hours, but only
//! installs when the user clicks.

use std::{sync::Mutex, time::Duration};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Clone, Serialize, PartialEq, Debug)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum Status {
    Idle,
    Checking,
    UpToDate,
    Available { version: String },
    Installing,
    Error { message: String },
}

pub struct Updates {
    pub status: Mutex<Status>,
    pending: Mutex<Option<Update>>,
}

impl Default for Updates {
    fn default() -> Self {
        Self { status: Mutex::new(Status::Idle), pending: Mutex::new(None) }
    }
}

const FIRST_CHECK: Duration = Duration::from_secs(30);
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// Background checks. Release builds only: a debug build is not installed
/// where the updater would put the new version.
pub fn start(app: &AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            tauri::async_runtime::block_on(check(&app, false));
            std::thread::sleep(EVERY);
        }
    });
}

fn set(app: &AppHandle, status: Status) {
    *app.state::<Updates>().status.lock().unwrap() = status.clone();
    let _ = app.emit_to("ui", "update", status);
    crate::tray::update(app);
}

/// `loud`: the user asked (tray or Settings), so also say "up to date" or
/// show errors as a notification when the window is hidden.
pub async fn check(app: &AppHandle, loud: bool) {
    let st = app.state::<Updates>();
    if matches!(*st.status.lock().unwrap(), Status::Checking | Status::Installing) {
        return;
    }
    set(app, Status::Checking);
    let result = match app.updater() {
        Ok(u) => u.check().await.map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    let status = match result {
        Ok(Some(update)) => {
            let version = update.version.clone();
            *st.pending.lock().unwrap() = Some(update);
            Status::Available { version }
        }
        Ok(None) => Status::UpToDate,
        // A failed background check is not worth bothering anyone about.
        Err(e) if !loud => {
            eprintln!("[wach] update check failed: {e}");
            Status::Idle
        }
        Err(e) => Status::Error { message: friendly(&e) },
    };
    if loud && !window_visible(app) {
        notify(app, &status);
    }
    set(app, status);
}

/// Download, verify and run the installer. The app quits while the installer
/// runs; the installer starts WACH again.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let update = app.state::<Updates>().pending.lock().unwrap().take().ok_or("No update waiting.")?;
    set(app, Status::Installing);
    if let Err(e) = update.download_and_install(|_, _| {}, || {}).await {
        let message = friendly(&e.to_string());
        *app.state::<Updates>().pending.lock().unwrap() = Some(update);
        set(app, Status::Error { message: message.clone() });
        return Err(message);
    }
    crate::quit(app);
    Ok(())
}

fn friendly(e: &str) -> String {
    let lower = e.to_lowercase();
    if lower.contains("signature") {
        "The update's signature did not match, so it was not installed.".into()
    } else if lower.contains("dns") || lower.contains("connect") || lower.contains("timed out") || lower.contains("network") {
        "Can't reach the update server. Check your internet connection.".into()
    } else if lower.contains("404") || lower.contains("not found") || lower.contains("release json") {
        "No published version found yet.".into()
    } else {
        format!("Update check failed: {}", e.chars().take(120).collect::<String>())
    }
}

fn window_visible(app: &AppHandle) -> bool {
    app.get_window("main").is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
}

#[cfg(windows)]
fn notify(app: &AppHandle, status: &Status) {
    let body = match status {
        Status::UpToDate => format!("You have the latest version ({}).", app.package_info().version),
        Status::Available { version } => format!("Version {version} is ready. Open WACH to install it."),
        Status::Error { message } => message.clone(),
        _ => return,
    };
    let app2 = app.clone();
    crate::toast::show("wach-update", "WACH updates", &body, "update", move || {
        let app3 = app2.clone();
        let _ = app2.run_on_main_thread(move || crate::show_window(&app3));
    });
}

#[cfg(not(windows))]
fn notify(_app: &AppHandle, _status: &Status) {}
