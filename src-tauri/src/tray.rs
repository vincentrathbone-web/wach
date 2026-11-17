//! Tray icon, its tooltip, and the unread dot on tray and taskbar.

use tauri::{
    AppHandle, Manager,
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use crate::App;

const TRAY_ID: &str = "main";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show WACH", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit WACH", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("WACH")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => crate::show_window(app),
            "quit" => crate::quit(app),
            _ => {}
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

/// Recompute totals and refresh tooltip, tray icon and taskbar overlay.
pub fn update(app: &AppHandle) {
    let state = app.state::<App>();
    let (lines, total) = {
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
        (lines, total)
    };
    let tooltip = if total == 0 { "WACH".to_string() } else { format!("WACH · {total} unread\n{}", lines.join("\n")) };
    // Windows limits tray tooltips to 127 characters.
    let tooltip: String = tooltip.chars().take(127).collect();
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(&tooltip));
        let _ = tray.set_icon(tray_icon(app, total > 0));
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
