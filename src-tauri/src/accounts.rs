//! One WebView2 per WhatsApp account, each with its own data folder.

use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Url, Webview, WebviewUrl, Window,
    webview::{NewWindowResponse, PermissionKind, PermissionResponse, WebviewBuilder},
    window::Color,
};

use crate::{
    App, platform,
    rules::{self, Nav, Perm},
};

/// Where account webviews wait while not shown in a pane. Off the window,
/// but never hidden: a hidden WebView2 may throttle the WhatsApp connection.
pub const PARK: (f64, f64) = (-32_000.0, 0.0);

pub fn label(id: &str) -> String {
    format!("acc-{id}")
}

pub fn get(app: &AppHandle, id: &str) -> Option<Webview> {
    app.get_webview(&label(id))
}

pub fn create(app: &AppHandle, window: &Window, id: &str) -> tauri::Result<Webview> {
    let state = app.state::<App>();
    let dir = state.store.account_dir(id).ok_or_else(|| tauri::Error::AssetNotFound(id.into()))?;
    let bg = state.cfg.lock().unwrap().settings.theme.background;
    let url: Url = format!("{}/", rules::WHATSAPP_ORIGIN).parse().expect("static url");

    let title_app = app.clone();
    let perm_app = app.clone();
    let title_id = id.to_string();
    let perm_id = id.to_string();

    let builder = WebviewBuilder::new(label(id), WebviewUrl::External(url))
        .data_directory(dir)
        .background_color(Color(bg[0], bg[1], bg[2], 255))
        .devtools(cfg!(debug_assertions))
        .zoom_hotkeys_enabled(false)
        .on_navigation(|url| match rules::navigation(url) {
            Nav::Allow => true,
            Nav::External(link) => {
                platform::open_in_browser(&link);
                false
            }
            Nav::Block => false,
        })
        .on_new_window(|url, _features| {
            // Never open a second window. Outside links go to the browser.
            if !rules::is_whatsapp(&url)
                && let Some(link) = rules::external_link(&url)
            {
                platform::open_in_browser(&link);
            }
            NewWindowResponse::Deny
        })
        .on_document_title_changed(move |_wv, title| {
            crate::set_unread(&title_app, &title_id, rules::unread_from_title(&title));
        })
        .on_permission_request(move |_wv, kind| {
            let perm = match kind {
                PermissionKind::Notifications => Perm::Notifications,
                PermissionKind::Microphone => Perm::Microphone,
                PermissionKind::Camera => Perm::Camera,
                PermissionKind::ClipboardRead => Perm::ClipboardRead,
                _ => Perm::Other,
            };
            let st = perm_app.state::<App>();
            let (n, m) = st
                .cfg
                .lock()
                .unwrap()
                .account(&perm_id)
                .map(|a| (a.notifications, a.media))
                .unwrap_or((false, false));
            if rules::permission(perm, n, m) { PermissionResponse::Allow } else { PermissionResponse::Deny }
        })
        .on_download(|_wv, _event| true);

    let webview = window.add_child(builder, LogicalPosition::new(PARK.0, PARK.1), LogicalSize::new(800.0, 600.0))?;
    crate::hooks::attach(app, &webview, id);
    let zoom = state.cfg.lock().unwrap().account(id).map(|a| a.zoom).unwrap_or(1.0);
    if (zoom - 1.0).abs() > f64::EPSILON {
        let _ = webview.set_zoom(zoom);
    }
    // WebView2 start-up is slow; the UI may have laid out before we existed.
    let rect = state.placed.lock().unwrap().iter().find(|(pid, _)| pid == id).map(|(_, r)| *r);
    place(&webview, rect);
    let _ = app.emit_to("ui", "relayout", ());
    Ok(webview)
}

pub fn place(webview: &Webview, rect: Option<rules::Rect>) {
    match rect {
        Some(r) => {
            let _ = webview.set_position(LogicalPosition::new(r.x, r.y));
            let _ = webview.set_size(LogicalSize::new(r.w, r.h));
        }
        None => {
            let _ = webview.set_position(LogicalPosition::new(PARK.0, PARK.1));
        }
    }
}
