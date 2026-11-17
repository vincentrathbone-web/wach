//! WACH: several WhatsApp Web accounts side by side, each fully separate.

mod accounts;
mod config;
#[cfg(windows)]
mod hooks;
#[cfg(windows)]
mod platform;
pub mod rules;
mod shortcuts;
#[cfg(windows)]
mod toast;
mod tray;
mod update;

use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition, PhysicalSize, RunEvent, State,
    Webview, WebviewUrl, WindowEvent,
    webview::{Color, WebviewBuilder},
    window::WindowBuilder,
};

use tauri_runtime::ResizeDirection;
use config::{Account, Config, Layout, Store, WindowState};
use shortcuts::Action;

pub struct App {
    store: Store,
    cfg: Mutex<Config>,
    unread: Mutex<HashMap<String, u32>>,
    reloads: Mutex<HashMap<String, Vec<u64>>>,
    /// Last pane rectangles from the UI, so late-starting webviews can be placed.
    placed: Mutex<Vec<(String, rules::Rect)>>,
    quitting: AtomicBool,
}

impl App {
    fn save(&self) {
        let cfg = self.cfg.lock().unwrap().clone();
        if let Err(e) = self.store.save(&cfg) {
            eprintln!("[wach] saving config failed: {e}");
        }
    }
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[derive(Serialize, Clone)]
struct SettingsDto {
    autostart: bool,
    muted_until: u64,
}

fn settings_dto(app: &AppHandle) -> SettingsDto {
    SettingsDto {
        autostart: platform::autostart_enabled(),
        muted_until: app.state::<App>().cfg.lock().unwrap().settings.muted_until,
    }
}

/// Tell the UI and the tray that a setting changed elsewhere.
fn settings_changed(app: &AppHandle) {
    let _ = app.emit_to("ui", "settings", settings_dto(app));
    tray::update(app);
}

/// "off" | "hour" | "forever" (tray menu and Settings).
pub(crate) fn set_mute(app: &AppHandle, choice: &str) {
    let now = now_secs();
    let Some(until) = rules::mute_until(choice, now) else { return };
    let state = app.state::<App>();
    state.cfg.lock().unwrap().settings.muted_until = until;
    state.save();
    settings_changed(app);
    if until != 0 && until != u64::MAX {
        // Refresh tray and UI when the hour is over.
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(until - now + 1));
            if app.state::<App>().cfg.lock().unwrap().settings.muted_until == until {
                let a2 = app.clone();
                let _ = app.run_on_main_thread(move || settings_changed(&a2));
            }
        });
    }
}

pub(crate) fn set_autostart(app: &AppHandle, on: bool) -> Result<(), String> {
    let r = platform::set_autostart(on);
    settings_changed(app);
    r
}

// ---------------------------------------------------------------- events from webviews

pub(crate) fn set_unread(app: &AppHandle, id: &str, count: u32) {
    let state = app.state::<App>();
    {
        let mut map = state.unread.lock().unwrap();
        if map.get(id) == Some(&count) {
            return;
        }
        map.insert(id.to_string(), count);
    }
    let _ = app.emit_to("ui", "unread", serde_json::json!({ "id": id, "count": count }));
    tray::update(app);
}

/// Bring the window up and show this account in the focused pane.
pub(crate) fn show_account(app: &AppHandle, id: &str) {
    show_window(app);
    let _ = app.emit_to("ui", "show-account", id);
}

pub(crate) fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub(crate) fn on_shortcut(app: &AppHandle, from: Option<&str>, action: Action) {
    match (action, from) {
        (Action::ZoomIn | Action::ZoomOut | Action::ZoomReset, Some(id)) => zoom(app, id, action),
        _ => {
            let _ = app.emit_to("ui", "shortcut", action);
        }
    }
}

fn zoom(app: &AppHandle, id: &str, action: Action) {
    let state = app.state::<App>();
    let z = {
        let mut cfg = state.cfg.lock().unwrap();
        let Some(a) = cfg.account_mut(id) else { return };
        a.zoom = match action {
            Action::ZoomIn => (a.zoom + 0.1).min(2.5),
            Action::ZoomOut => (a.zoom - 0.1).max(0.5),
            _ => 1.0,
        };
        a.zoom = (a.zoom * 10.0).round() / 10.0;
        a.zoom
    };
    if let Some(wv) = accounts::get(app, id) {
        let _ = wv.set_zoom(z);
    }
    state.save();
}

pub(crate) fn on_account_crashed(app: &AppHandle, id: &str, browser_gone: bool) {
    let now = now_secs();
    let allowed = {
        let state = app.state::<App>();
        let mut map = state.reloads.lock().unwrap();
        rules::may_reload(map.entry(id.to_string()).or_default(), now, 3, 60)
    };
    let _ = app.emit_to("ui", "account-crashed", serde_json::json!({ "id": id, "reloading": allowed }));
    if !allowed {
        return;
    }
    if browser_gone {
        recreate_webview(app, id);
    } else if let Some(wv) = accounts::get(app, id) {
        let _ = wv.reload();
    }
}

/// Close (if needed) and start an account webview again. Runs off the main
/// thread: creating a WebView2 from the main thread waits on itself.
fn recreate_webview(app: &AppHandle, id: &str) {
    let (app, id) = (app.clone(), id.to_string());
    std::thread::spawn(move || {
        if let Some(wv) = accounts::get(&app, &id) {
            let _ = wv.close();
            for _ in 0..40 {
                if accounts::get(&app, &id).is_none() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        start_webview(&app, &id);
    });
}

fn start_webview(app: &AppHandle, id: &str) {
    if let Some(w) = app.get_window("main")
        && let Err(e) = accounts::create(app, &w, id)
    {
        eprintln!("[wach] account {id} failed to start: {e}");
    }
}

// ---------------------------------------------------------------- commands (UI webview only)

fn ui_only(webview: &Webview) -> Result<(), String> {
    if webview.label() == "ui" { Ok(()) } else { Err("not allowed".into()) }
}

#[derive(Serialize)]
struct StateDto {
    config: Config,
    unread: HashMap<String, u32>,
    maximized: bool,
    settings: SettingsDto,
    version: String,
    update: update::Status,
    /// Debug builds only: run ui/selftest.js (WACH_SELFTEST=1).
    selftest: bool,
}

/// Debug builds only: where each account webview is (physical px; x < -1000 = parked).
#[cfg(debug_assertions)]
#[tauri::command]
fn debug_bounds(webview: Webview, app: AppHandle, state: State<App>) -> Result<HashMap<String, (i32, i32, u32, u32)>, String> {
    ui_only(&webview)?;
    let ids: Vec<String> = state.cfg.lock().unwrap().accounts.iter().map(|a| a.id.clone()).collect();
    Ok(ids
        .into_iter()
        .filter_map(|id| {
            let wv = accounts::get(&app, &id)?;
            let (p, s) = (wv.position().ok()?, wv.size().ok()?);
            Some((id, (p.x, p.y, s.width, s.height)))
        })
        .collect())
}

/// Builds before 0.1 kept data in %LOCALAPPDATA%\WACH, which is also where the
/// installer puts the program. Move it once; never overwrite newer data.
fn move_old_data(old: &std::path::Path, new: &std::path::Path) {
    if !old.join("config.json").is_file() || new.join("config.json").exists() {
        return;
    }
    let _ = std::fs::create_dir_all(new);
    for name in ["config.json", "closed.json", "accounts"] {
        let (from, to) = (old.join(name), new.join(name));
        if from.exists() && !to.exists() {
            let _ = std::fs::rename(&from, &to);
        }
    }
}

/// Debug builds only: fake the events WhatsApp would cause, for the self-test.
#[cfg(debug_assertions)]
#[tauri::command]
fn debug_simulate(webview: Webview, app: AppHandle, state: State<App>, kind: String, id: String, n: Option<u32>) -> Result<serde_json::Value, String> {
    ui_only(&webview)?;
    match kind.as_str() {
        "unread" => set_unread(&app, &id, n.unwrap_or(0)),
        "title" => set_unread(&app, &id, rules::unread_from_title(&format!("({}) WhatsApp", n.unwrap_or(0)))),
        "toast" => {
            let name = state.cfg.lock().unwrap().account(&id).map(|a| a.name.clone()).unwrap_or_default();
            let (app2, id2) = (app.clone(), id.clone());
            toast::show(&id, &format!("{name} · WACH self-test"), "This is a test notification. Click it to open the account.", "selftest", move || {
                let app3 = app2.clone();
                let _ = app2.run_on_main_thread(move || show_account(&app3, &id2));
            });
        }
        "zoom-in" => on_shortcut(&app, Some(&id), Action::ZoomIn),
        "zoom-reset" => on_shortcut(&app, Some(&id), Action::ZoomReset),
        "crash" => on_account_crashed(&app, &id, false),
        "crash-browser" => on_account_crashed(&app, &id, true),
        "hide" => {
            let _ = webview.window().close();
        }
        _ => return Err("unknown".into()),
    }
    let cfg = state.cfg.lock().unwrap();
    Ok(serde_json::json!({ "zoom": cfg.account(&id).map(|a| a.zoom) }))
}

/// Debug builds only: print self-test lines to stderr.
#[cfg(debug_assertions)]
#[tauri::command]
fn debug_log(webview: Webview, line: String) -> Result<(), String> {
    ui_only(&webview)?;
    eprintln!("[selftest] {line}");
    Ok(())
}

#[tauri::command]
fn get_state(webview: Webview, app: AppHandle, state: State<App>, updates: State<update::Updates>) -> Result<StateDto, String> {
    ui_only(&webview)?;
    let maximized = webview.window().is_maximized().unwrap_or(false);
    // Before the struct below: a lock taken inside a struct literal is held
    // until the whole literal is built, and settings_dto locks cfg too.
    let settings = settings_dto(&app);
    let config = state.cfg.lock().unwrap().clone();
    Ok(StateDto {
        config,
        unread: state.unread.lock().unwrap().clone(),
        maximized,
        settings,
        version: app.package_info().version.to_string(),
        update: updates.status.lock().unwrap().clone(),
        selftest: cfg!(debug_assertions) && std::env::var("WACH_SELFTEST").is_ok_and(|v| v == "1"),
    })
}

#[tauri::command]
fn add_account(webview: Webview, app: AppHandle, state: State<App>, name: String, color: String) -> Result<Account, String> {
    ui_only(&webview)?;
    let name = rules::clean_name(&name).ok_or("Name must be 1–40 characters.")?;
    if !rules::valid_color(&color) {
        return Err("Unknown colour.".into());
    }
    let account = Account { id: rules::new_id(), name, color, ..Default::default() };
    {
        let mut cfg = state.cfg.lock().unwrap();
        if cfg.accounts.len() >= 20 {
            return Err("20 accounts is the limit.".into());
        }
        cfg.accounts.push(account.clone());
    }
    state.save();
    // Off the main thread (see recreate_webview); the UI places it via "relayout".
    let (app2, id) = (app.clone(), account.id.clone());
    std::thread::spawn(move || start_webview(&app2, &id));
    Ok(account)
}

#[derive(Deserialize)]
struct AccountPatch {
    name: Option<String>,
    color: Option<String>,
    notifications: Option<bool>,
    media: Option<bool>,
}

#[tauri::command]
fn update_account(webview: Webview, state: State<App>, id: String, patch: AccountPatch) -> Result<Account, String> {
    ui_only(&webview)?;
    let updated = {
        let mut cfg = state.cfg.lock().unwrap();
        let a = cfg.account_mut(&id).ok_or("No such account.")?;
        if let Some(n) = patch.name {
            a.name = rules::clean_name(&n).ok_or("Name must be 1–40 characters.")?;
        }
        if let Some(c) = patch.color {
            if !rules::valid_color(&c) {
                return Err("Unknown colour.".into());
            }
            a.color = c;
        }
        if let Some(v) = patch.notifications {
            a.notifications = v;
        }
        if let Some(v) = patch.media {
            a.media = v;
        }
        a.clone()
    };
    state.save();
    Ok(updated)
}

#[tauri::command]
fn remove_account(webview: Webview, app: AppHandle, state: State<App>, id: String) -> Result<(), String> {
    ui_only(&webview)?;
    if !rules::valid_id(&id) {
        return Err("Bad id.".into());
    }
    {
        let mut cfg = state.cfg.lock().unwrap();
        let before = cfg.accounts.len();
        cfg.accounts.retain(|a| a.id != id);
        if cfg.accounts.len() == before {
            return Err("No such account.".into());
        }
        cfg.sanitize();
    }
    state.save();
    state.unread.lock().unwrap().remove(&id);
    if let Some(wv) = accounts::get(&app, &id) {
        let _ = wv.close();
    }
    // Its folder is locked while WebView2 runs; delete it at the next start.
    let mut closed = state.store.load_closed();
    if !closed.contains(&id) {
        closed.push(id);
    }
    state.store.save_closed(&closed).map_err(|e| e.to_string())?;
    tray::update(&app);
    Ok(())
}

#[tauri::command]
fn move_account(webview: Webview, state: State<App>, id: String, to: usize) -> Result<(), String> {
    ui_only(&webview)?;
    {
        let mut cfg = state.cfg.lock().unwrap();
        let from = cfg.accounts.iter().position(|a| a.id == id).ok_or("No such account.")?;
        let a = cfg.accounts.remove(from);
        let to = to.min(cfg.accounts.len());
        cfg.accounts.insert(to, a);
    }
    state.save();
    Ok(())
}

#[tauri::command]
fn set_layout(webview: Webview, state: State<App>, layout: Layout) -> Result<Layout, String> {
    ui_only(&webview)?;
    let clean = {
        let mut cfg = state.cfg.lock().unwrap();
        cfg.layout = layout;
        cfg.sanitize();
        cfg.layout.clone()
    };
    state.save();
    Ok(clean)
}

#[derive(Deserialize)]
struct Placement {
    id: String,
    rect: rules::Rect,
}

/// The UI tells us where its pane bodies are; every other account is parked.
#[tauri::command]
fn place_panes(webview: Webview, app: AppHandle, state: State<App>, panes: Vec<Placement>) -> Result<(), String> {
    ui_only(&webview)?;
    if panes.len() > 2 || panes.iter().any(|p| !p.rect.is_sane()) {
        return Err("Bad layout.".into());
    }
    *state.placed.lock().unwrap() = panes.iter().map(|p| (p.id.clone(), p.rect)).collect();
    let ids: Vec<String> = state.cfg.lock().unwrap().accounts.iter().map(|a| a.id.clone()).collect();
    for id in &ids {
        if let Some(wv) = accounts::get(&app, id) {
            accounts::place(&wv, panes.iter().find(|p| &p.id == id).map(|p| p.rect));
        }
    }
    Ok(())
}

#[tauri::command]
fn focus_account(webview: Webview, app: AppHandle, id: String) -> Result<(), String> {
    ui_only(&webview)?;
    if let Some(wv) = accounts::get(&app, &id) {
        wv.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn reload_account(webview: Webview, app: AppHandle, id: String) -> Result<(), String> {
    ui_only(&webview)?;
    match accounts::get(&app, &id) {
        Some(wv) => wv.reload().map_err(|e| e.to_string()),
        None => {
            recreate_webview(&app, &id);
            Ok(())
        }
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn set_settings(
    webview: Webview,
    app: AppHandle,
    state: State<App>,
    close_to_tray: Option<bool>,
    glow: Option<u32>,
    autostart: Option<bool>,
    mute: Option<String>,
) -> Result<SettingsDto, String> {
    ui_only(&webview)?;
    {
        let mut cfg = state.cfg.lock().unwrap();
        if let Some(v) = close_to_tray {
            cfg.settings.close_to_tray = v;
        }
        if let Some(g) = glow {
            cfg.settings.glow = g.min(48);
        }
    }
    state.save();
    if let Some(choice) = mute {
        if rules::mute_until(&choice, 0).is_none() {
            return Err("Unknown mute choice.".into());
        }
        set_mute(&app, &choice);
    }
    if let Some(on) = autostart {
        set_autostart(&app, on)?;
    }
    Ok(settings_dto(&app))
}

#[tauri::command]
async fn check_update(webview: Webview, app: AppHandle) -> Result<update::Status, String> {
    ui_only(&webview)?;
    update::check(&app, true).await;
    Ok(app.state::<update::Updates>().status.lock().unwrap().clone())
}

#[tauri::command]
async fn install_update(webview: Webview, app: AppHandle) -> Result<(), String> {
    ui_only(&webview)?;
    update::install(&app).await
}

#[tauri::command]
fn window_cmd(webview: Webview, app: AppHandle, action: String) -> Result<(), String> {
    ui_only(&webview)?;
    let w = webview.window();
    let r = match action.as_str() {
        "minimize" => w.minimize(),
        "toggle-maximize" => {
            if w.is_maximized().unwrap_or(false) { w.unmaximize() } else { w.maximize() }
        }
        "drag" => w.start_dragging(),
        "close" => w.close(),
        "quit" => {
            quit(&app);
            Ok(())
        }
        _ => return Err("Unknown action.".into()),
    };
    r.map_err(|e| e.to_string())
}

#[tauri::command]
fn window_resize(webview: Webview, edge: String) -> Result<(), String> {
    ui_only(&webview)?;
    let dir = match edge.as_str() {
        "n" => ResizeDirection::North,
        "s" => ResizeDirection::South,
        "e" => ResizeDirection::East,
        "w" => ResizeDirection::West,
        "ne" => ResizeDirection::NorthEast,
        "nw" => ResizeDirection::NorthWest,
        "se" => ResizeDirection::SouthEast,
        "sw" => ResizeDirection::SouthWest,
        _ => return Err("Unknown edge.".into()),
    };
    webview.window().start_resize_dragging(dir).map_err(|e| e.to_string())
}

pub(crate) fn quit(app: &AppHandle) {
    app.state::<App>().quitting.store(true, Ordering::SeqCst);
    save_window_state(app);
    app.exit(0);
}

fn save_window_state(app: &AppHandle) {
    let Some(w) = app.get_window("main") else { return };
    let maximized = w.is_maximized().unwrap_or(false);
    let state = app.state::<App>();
    let mut cfg = state.cfg.lock().unwrap();
    if maximized || w.is_minimized().unwrap_or(false) {
        if let Some(ws) = cfg.window.as_mut() {
            ws.maximized = maximized;
        }
    } else if let (Ok(p), Ok(s)) = (w.outer_position(), w.inner_size()) {
        cfg.window = Some(WindowState { x: p.x, y: p.y, w: s.width, h: s.height, maximized: false });
    }
    drop(cfg);
    state.save();
}

// ---------------------------------------------------------------- start-up

macro_rules! commands {
    ($($extra:ident),*) => {
        tauri::generate_handler![
            get_state, add_account, update_account, remove_account, move_account, set_layout,
            place_panes, focus_account, reload_account, set_settings, window_cmd, window_resize,
            check_update, install_update
            $(, $extra)*
        ]
    };
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // A sign-in start (--hidden) must not pop up a window that is already running.
            if !args.iter().any(|a| a == platform::HIDDEN_FLAG) {
                show_window(app);
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build());
    #[cfg(debug_assertions)]
    let builder = builder.invoke_handler(commands!(debug_bounds, debug_log, debug_simulate));
    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(commands!());
    builder
        .setup(|app| {
            // %LOCALAPPDATA%\app.wach.desktop — kept apart from the install
            // folder (%LOCALAPPDATA%\WACH), and the folder the uninstaller's
            // "delete app data" box removes.
            let mut root = app.path().app_local_data_dir()?;
            // Debug builds only: WACH_DATA_DIR keeps tests away from real logins.
            if cfg!(debug_assertions)
                && let Some(dir) = std::env::var_os("WACH_DATA_DIR")
            {
                root = dir.into();
            } else {
                move_old_data(&app.path().local_data_dir()?.join("WACH"), &root);
            }
            std::fs::create_dir_all(root.join("accounts"))?;
            let store = Store::new(root.clone());
            let cfg = store.load();
            store.cleanup_closed(&cfg);
            store.migrate_profiles(&cfg);

            #[cfg(windows)]
            {
                let icon = root.join("wach.png");
                let _ = std::fs::write(&icon, include_bytes!("../icons/128x128.png"));
                platform::register_app_id(Some(&icon));
            }

            let bg = cfg.settings.theme.background;
            let saved = cfg.window.clone();
            let account_ids: Vec<String> = cfg.accounts.iter().map(|a| a.id.clone()).collect();
            app.manage(App {
                store,
                cfg: Mutex::new(cfg),
                unread: Mutex::new(HashMap::new()),
                reloads: Mutex::new(HashMap::new()),
                placed: Mutex::new(Vec::new()),
                quitting: AtomicBool::new(false),
            });
            app.manage(update::Updates::default());

            let window = WindowBuilder::new(app, "main")
                .title("WACH")
                .inner_size(1100.0, 760.0)
                .min_inner_size(520.0, 400.0)
                .decorations(false)
                .background_color(Color(bg[0], bg[1], bg[2], 255))
                .visible(false)
                .build()?;
            if let Some(icon) = app.default_window_icon() {
                let _ = window.set_icon(icon.clone());
            }

            // Restore size and place, but only if that spot is on a screen now.
            if let Some(ws) = &saved {
                let monitors: Vec<(i32, i32, u32, u32)> = window
                    .available_monitors()
                    .unwrap_or_default()
                    .iter()
                    .map(|m| (m.position().x, m.position().y, m.size().width, m.size().height))
                    .collect();
                if rules::visible_on_any((ws.x, ws.y, ws.w, ws.h), &monitors) {
                    let _ = window.set_size(PhysicalSize::new(ws.w.max(520), ws.h.max(400)));
                    let _ = window.set_position(PhysicalPosition::new(ws.x, ws.y));
                } else {
                    let _ = window.set_size(PhysicalSize::new(ws.w.max(520), ws.h.max(400)));
                    let _ = window.center();
                }
                if ws.maximized {
                    let _ = window.maximize();
                }
            } else {
                let _ = window.center();
            }

            // Our own chrome fills the window; account webviews sit on top.
            let size = window.inner_size()?;
            let scale = window.scale_factor()?;
            let ui = WebviewBuilder::new("ui", WebviewUrl::App("index.html".into()))
                .data_directory(root.join("ui"))
                .background_color(Color(bg[0], bg[1], bg[2], 255))
                .devtools(cfg!(debug_assertions))
                .zoom_hotkeys_enabled(false)
                .auto_resize();
            window.add_child(ui, LogicalPosition::new(0.0, 0.0), LogicalSize::new(size.width as f64 / scale, size.height as f64 / scale))?;

            for id in &account_ids {
                if let Err(e) = accounts::create(app.handle(), &window, id) {
                    eprintln!("[wach] account {id} failed to start: {e}");
                }
            }

            tray::create(app.handle())?;

            let app2 = app.handle().clone();
            window.on_window_event(move |event| match event {
                WindowEvent::CloseRequested { api, .. } => {
                    let state = app2.state::<App>();
                    let to_tray = state.cfg.lock().unwrap().settings.close_to_tray;
                    save_window_state(&app2);
                    if to_tray && !state.quitting.load(Ordering::SeqCst) {
                        api.prevent_close();
                        if let Some(w) = app2.get_window("main") {
                            let _ = w.hide();
                        }
                    }
                }
                WindowEvent::Resized(_) => {
                    let max = app2.get_window("main").and_then(|w| w.is_maximized().ok()).unwrap_or(false);
                    let _ = app2.emit_to("ui", "window-state", serde_json::json!({ "maximized": max }));
                }
                _ => {}
            });

            update::start(app.handle());

            // Started by Windows at sign-in: stay in the tray. The accounts
            // still load, so notifications arrive.
            if !std::env::args().any(|a| a == platform::HIDDEN_FLAG) {
                window.show()?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building WACH")
        .run(|app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                save_window_state(app);
            }
        });
}
