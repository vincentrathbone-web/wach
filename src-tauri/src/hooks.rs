//! WebView2 features Tauri does not expose: native notifications, the
//! context menu, crash recovery, offline page, and our keyboard shortcuts.
//! Nothing here injects code into WhatsApp's page.

use std::{cell::RefCell, collections::HashMap};

use tauri::{AppHandle, Emitter, Manager, Webview};
use webview2_com::{
    AcceleratorKeyPressedEventHandler, ContextMenuRequestedEventHandler, FocusChangedEventHandler,
    Microsoft::Web::WebView2::Win32::*, NavigationCompletedEventHandler, NotificationReceivedEventHandler,
    ProcessFailedEventHandler, take_pwstr,
};
use windows::{
    Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT},
    core::{HSTRING, Interface, PWSTR},
};

use crate::{App, toast};

thread_local! {
    /// WebView2 notification objects waiting for a click, by our own id.
    /// They are COM objects bound to the UI thread, so they live here.
    static PENDING: RefCell<(u64, HashMap<u64, ICoreWebView2Notification>)> = RefCell::new((0, HashMap::new()));
}

pub fn attach(app: &AppHandle, webview: &Webview, id: &str) {
    let app = app.clone();
    let id = id.to_string();
    let _ = webview.with_webview(move |pw| unsafe {
        let controller = pw.controller();
        if let Err(e) = setup(&app, &id, &controller) {
            eprintln!("[wach] WebView2 hooks for {id}: {e}");
        }
    });
}

unsafe fn setup(app: &AppHandle, id: &str, controller: &ICoreWebView2Controller) -> windows::core::Result<()> {
    unsafe {
        let core = controller.CoreWebView2()?;
        let mut token = 0i64;

        // Settings: no status bar, no password/autofill prompts, our own zoom.
        let settings = core.Settings()?;
        settings.SetIsStatusBarEnabled(false)?;
        settings.SetIsZoomControlEnabled(false)?;
        if let Ok(s4) = settings.cast::<ICoreWebView2Settings4>() {
            s4.SetIsPasswordAutosaveEnabled(false)?;
            s4.SetIsGeneralAutofillEnabled(false)?;
        }

        // Native notifications → our own toast that names the account.
        if let Ok(core24) = core.cast::<ICoreWebView2_24>() {
            let (app2, id2) = (app.clone(), id.to_string());
            core24.add_NotificationReceived(
                &NotificationReceivedEventHandler::create(Box::new(move |_sender, args| {
                    let Some(args) = args else { return Ok(()) };
                    args.SetHandled(true)?;
                    let n = args.Notification()?;
                    let title = read(|p| n.Title(p));
                    let body = read(|p| n.Body(p));
                    let tag = read(|p| n.Tag(p));
                    on_notification(&app2, &id2, n, title, body, tag);
                    Ok(())
                })),
                &mut token,
            )?;
        }

        // Context menu: drop browser items that make no sense in an app.
        if let Ok(core11) = core.cast::<ICoreWebView2_11>() {
            core11.add_ContextMenuRequested(
                &ContextMenuRequestedEventHandler::create(Box::new(|_sender, args| {
                    let Some(args) = args else { return Ok(()) };
                    let items = args.MenuItems()?;
                    let mut count = 0u32;
                    items.Count(&mut count)?;
                    for i in (0..count).rev() {
                        let item = items.GetValueAtIndex(i)?;
                        let name = read(|p| item.Name(p));
                        if BLOCKED_MENU_ITEMS.contains(&name.as_str()) {
                            items.RemoveValueAtIndex(i)?;
                        }
                    }
                    Ok(())
                })),
                &mut token,
            )?;
        }

        // Crash recovery: reload just this account, a few times a minute at most.
        let (app2, id2) = (app.clone(), id.to_string());
        core.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_sender, args| {
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                args.ProcessFailedKind(&mut kind)?;
                let browser_gone = kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED;
                crate::on_account_crashed(&app2, &id2, browser_gone);
                Ok(())
            })),
            &mut token,
        )?;

        // Offline / failed load: our own small page with a retry link.
        core.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(|sender, args| {
                let (Some(core), Some(args)) = (sender, args) else { return Ok(()) };
                let mut ok = false.into();
                args.IsSuccess(&mut ok)?;
                if ok.as_bool() {
                    return Ok(());
                }
                let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                args.WebErrorStatus(&mut status)?;
                if status == COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED {
                    return Ok(());
                }
                core.NavigateToString(&HSTRING::from(OFFLINE_PAGE))?;
                Ok(())
            })),
            &mut token,
        )?;

        // Our shortcuts work even while a WhatsApp pane has the keyboard.
        let (app2, id2) = (app.clone(), id.to_string());
        controller.add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_sender, args| {
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
                args.KeyEventKind(&mut kind)?;
                if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN {
                    return Ok(());
                }
                let mut vk = 0u32;
                args.VirtualKey(&mut vk)?;
                let down = |k: i32| GetKeyState(k) < 0;
                let ctrl = down(VK_CONTROL.0 as i32);
                let shift = down(VK_SHIFT.0 as i32);
                let alt = down(VK_MENU.0 as i32);
                if let Some(action) = crate::shortcuts::from_key(vk, ctrl, shift, alt) {
                    args.SetHandled(true)?;
                    crate::on_shortcut(&app2, Some(&id2), action);
                }
                Ok(())
            })),
            &mut token,
        )?;

        // Tell the UI which pane the user clicked into.
        let (app2, id2) = (app.clone(), id.to_string());
        controller.add_GotFocus(
            &FocusChangedEventHandler::create(Box::new(move |_sender, _args| {
                let _ = app2.emit_to("ui", "account-focused", &id2);
                Ok(())
            })),
            &mut token,
        )?;

        Ok(())
    }
}

const BLOCKED_MENU_ITEMS: &[&str] = &[
    "back", "forward", "reload", "saveAs", "print", "share", "webCapture", "inspectElement",
    "createQrCode", "openLinkInNewWindow", "readAloud", "translate", "other",
];

const OFFLINE_PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>WACH</title>
<style>html,body{height:100%;margin:0}body{display:grid;place-items:center;background:#0b1224;color:#e7edfb;font:15px "Segoe UI",system-ui,sans-serif}
div{text-align:center}p{color:#93a1c4;margin:6px 0 18px}a{display:inline-block;padding:9px 18px;border-radius:10px;background:#2a64f6;color:#fff;text-decoration:none;font-weight:600}</style>
</head><body><div><h1 style="font-size:20px;margin:0">Can't reach WhatsApp</h1><p>Check your internet connection.</p><a href="https://web.whatsapp.com/">Retry</a></div></body></html>"#;

unsafe fn read(f: impl FnOnce(*mut PWSTR) -> windows::core::Result<()>) -> String {
    let mut p = PWSTR::null();
    match f(&mut p) {
        Ok(()) if !p.is_null() => take_pwstr(p),
        _ => String::new(),
    }
}

fn on_notification(app: &AppHandle, id: &str, n: ICoreWebView2Notification, title: String, body: String, tag: String) {
    let state = app.state::<App>();
    let (name, enabled) = {
        let cfg = state.cfg.lock().unwrap();
        let muted = crate::rules::is_muted(cfg.settings.muted_until, crate::now_secs());
        match cfg.account(id) {
            Some(a) => (a.name.clone(), a.notifications && !muted),
            None => return,
        }
    };
    if !enabled {
        unsafe {
            let _ = n.ReportClosed();
        }
        return;
    }
    let key = PENDING.with(|p| {
        let mut p = p.borrow_mut();
        p.0 += 1;
        let key = p.0;
        p.1.insert(key, n.clone());
        // Keep the newest 50 only.
        if p.1.len() > 50 {
            let oldest = *p.1.keys().min().unwrap();
            p.1.remove(&oldest);
        }
        key
    });
    unsafe {
        let _ = n.ReportShown();
    }
    let (app2, id2) = (app.clone(), id.to_string());
    toast::show(id, &format!("{name} · {title}"), &body, &tag, move || {
        let app3 = app2.clone();
        let _ = app2.run_on_main_thread(move || {
            crate::show_account(&app3, &id2);
            PENDING.with(|p| {
                if let Some(n) = p.borrow_mut().1.remove(&key) {
                    unsafe {
                        let _ = n.ReportClicked();
                    }
                }
            });
        });
    });
}
