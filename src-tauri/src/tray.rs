//! Tray icon, its tooltip, and the unread dot on tray and taskbar.

use tauri::{
    AppHandle, Manager,
    image::Image,
    menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use crate::{App, rules, update};

const TRAY_ID: &str = "main";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("WACH")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "show" => crate::show_window(app),
                "quit" => crate::quit(app),
                "autostart" => {
                    let on = !crate::platform::autostart_enabled();
                    if let Err(e) = crate::set_autostart(app, on) {
                        eprintln!("[wach] autostart: {e}");
                        update(app);
                    }
                }
                "update-check" => {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move { update::check(&app, true).await });
                }
                "update-install" => {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = update::install(&app).await;
                    });
                }
                _ => {
                    if let Some(choice) = id.strip_prefix("mute:") {
                        crate::set_mute(app, choice);
                    } else if let Some(acc) = id.strip_prefix("acc:") {
                        crate::show_account(app, acc);
                    }
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                let app = tray.app_handle();
                match app.get_window("main") {
                    Some(w) if w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false) => {
                        let _ = w.hide();
                    }
                    _ => crate::show_window(app),
                }
            }
        });
    if let Some(icon) = tray_icon(app, false) {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

/// Accounts (with unread), mute, start with Windows, updates, show, quit.
fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let state = app.state::<App>();
    let (accounts, muted_until) = {
        let cfg = state.cfg.lock().unwrap();
        let unread = state.unread.lock().unwrap();
        let list: Vec<(String, String)> = cfg
            .accounts
            .iter()
            .map(|a| {
                let n = unread.get(&a.id).copied().unwrap_or(0);
                // "&" would become a keyboard accelerator in Windows menus.
                let name = a.name.replace('&', "&&");
                (a.id.clone(), if n > 0 { format!("{name}  ({n})") } else { name })
            })
            .collect();
        (list, cfg.settings.muted_until)
    };
    let muted = rules::is_muted(muted_until, crate::now_secs());

    let mut items: Vec<Box<dyn IsMenuItem<tauri::Wry>>> = Vec::new();
    for (id, label) in accounts {
        items.push(Box::new(MenuItem::with_id(app, format!("acc:{id}"), label, true, None::<&str>)?));
    }
    if !items.is_empty() {
        items.push(Box::new(PredefinedMenuItem::separator(app)?));
    }
    let mute_label = match (muted, muted_until) {
        (false, _) => "Mute notifications",
        (true, u64::MAX) => "Notifications muted",
        (true, _) => "Notifications muted for now",
    };
    let mute = Submenu::with_items(
        app,
        mute_label,
        true,
        &[
            &MenuItem::with_id(app, "mute:hour", "For 1 hour", true, None::<&str>)?,
            &MenuItem::with_id(app, "mute:forever", "Until I turn them back on", true, None::<&str>)?,
            &MenuItem::with_id(app, "mute:off", "Unmute", muted, None::<&str>)?,
        ],
    )?;
    items.push(Box::new(mute));
    items.push(Box::new(CheckMenuItem::with_id(
        app,
        "autostart",
        "Start with Windows",
        true,
        crate::platform::autostart_enabled(),
        None::<&str>,
    )?));
    let status = app.state::<update::Updates>().status.lock().unwrap().clone();
    let update_item = match status {
        update::Status::Available { version } => MenuItem::with_id(app, "update-install", format!("Install update {version}"), true, None::<&str>)?,
        update::Status::Checking => MenuItem::with_id(app, "update-check", "Checking for updates…", false, None::<&str>)?,
        update::Status::Installing => MenuItem::with_id(app, "update-check", "Installing update…", false, None::<&str>)?,
        _ => MenuItem::with_id(app, "update-check", "Check for updates", true, None::<&str>)?,
    };
    items.push(Box::new(update_item));
    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(MenuItem::with_id(app, "show", "Show WACH", true, None::<&str>)?));
    items.push(Box::new(MenuItem::with_id(app, "quit", "Quit WACH", true, None::<&str>)?));
    let refs: Vec<&dyn IsMenuItem<tauri::Wry>> = items.iter().map(|b| b.as_ref()).collect();
    Menu::with_items(app, &refs)
}

/// Recompute totals and refresh menu, tooltip, tray icon and taskbar overlay.
pub fn update(app: &AppHandle) {
    let state = app.state::<App>();
    let (lines, total, muted) = {
        let cfg = state.cfg.lock().unwrap();
        let unread = state.unread.lock().unwrap();
        let mut total = 0u32;
        let mut lines = Vec::new();
        for a in &cfg.accounts {
            let n = unread.get(&a.id).copied().unwrap_or(0);
            total = total.saturating_add(n);
            if n > 0 {
                lines.push(format!("{}: {n}", a.name));
            }
        }
        (lines, total, rules::is_muted(cfg.settings.muted_until, crate::now_secs()))
    };
    let mut tooltip = if total == 0 { "WACH".to_string() } else { format!("WACH · {total} unread\n{}", lines.join("\n")) };
    if muted {
        tooltip.push_str("\nNotifications muted");
    }
    // Windows limits tray tooltips to 127 characters.
    let tooltip: String = tooltip.chars().take(127).collect();
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(&tooltip));
        let _ = tray.set_icon(tray_icon(app, total > 0));
        if let Ok(menu) = build_menu(app) {
            let _ = tray.set_menu(Some(menu));
        }
    }
    if let Some(w) = app.get_window("main") {
        let overlay = (total > 0).then(|| dot_image(16, alert(app)));
        let _ = w.set_overlay_icon(overlay);
    }
}

fn alert(app: &AppHandle) -> [u8; 3] {
    app.state::<App>().cfg.lock().unwrap().settings.theme.alert
}

fn tray_icon(app: &AppHandle, unread: bool) -> Option<Image<'static>> {
    let base = Image::from_bytes(include_bytes!("../icons/tray.png")).ok()?.to_owned();
    if !unread {
        return Some(base);
    }
    let (w, h) = (base.width(), base.height());
    let mut rgba = base.rgba().to_vec();
    let c = alert(app);
    let r = w as f32 * 0.22;
    let (cx, cy) = (w as f32 - r - 0.5, r + 0.5);
    paint_circle(&mut rgba, w, h, cx, cy, r + w as f32 * 0.06, [0x0B, 0x12, 0x24]);
    paint_circle(&mut rgba, w, h, cx, cy, r, c);
    Some(Image::new_owned(rgba, w, h))
}

fn dot_image(size: u32, color: [u8; 3]) -> Image<'static> {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let r = size as f32 / 2.0 - 0.5;
    paint_circle(&mut rgba, size, size, size as f32 / 2.0, size as f32 / 2.0, r, color);
    Image::new_owned(rgba, size, size)
}

/// Anti-aliased filled circle, blended over what is there.
fn paint_circle(rgba: &mut [u8], w: u32, h: u32, cx: f32, cy: f32, r: f32, c: [u8; 3]) {
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let cover = (r - d + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let i = ((y * w + x) * 4) as usize;
            for k in 0..3 {
                rgba[i + k] = (c[k] as f32 * cover + rgba[i + k] as f32 * (1.0 - cover)) as u8;
            }
            rgba[i + 3] = (255.0 * cover + rgba[i + 3] as f32 * (1.0 - cover)) as u8;
        }
    }
}
