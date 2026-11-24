//! Windows toast notifications under our own AppUserModelID.

use std::sync::Mutex;

use windows::{
    Data::Xml::Dom::XmlDocument,
    Foundation::TypedEventHandler,
    UI::Notifications::{ToastActivatedEventArgs, ToastNotification, ToastNotificationManager},
    core::{HSTRING, IInspectable, Interface},
};

use crate::platform::AUMID;

/// Toasts must stay alive for their click handler to fire.
static LIVE: Mutex<Vec<ToastNotification>> = Mutex::new(Vec::new());

pub fn show(group: &str, title: &str, body: &str, tag: &str, on_click: impl FnOnce() + Send + 'static) {
    if let Err(e) = try_show(group, title, body, tag, on_click) {
        eprintln!("[wach] toast failed: {e}");
    }
}

fn try_show(group: &str, title: &str, body: &str, tag: &str, on_click: impl FnOnce() + Send + 'static) -> windows::core::Result<()> {
    let xml = format!(
        r#"<toast><visual><binding template="ToastGeneric"><text hint-maxLines="1">{}</text><text>{}</text></binding></visual></toast>"#,
        escape(&cap(title, 120)),
        escape(&cap(body, 400)),
    );
    present(&xml, group, tag, move |_| on_click())
}

/// "Saved …" toast for a finished download. Clicking it calls `on_click`
/// with "open"; the "Show in folder" button calls it with "folder".
pub fn show_saved(title: &str, body: &str, on_click: impl FnOnce(String) + Send + 'static) {
    let xml = format!(
        r#"<toast launch="open"><visual><binding template="ToastGeneric"><text hint-maxLines="1">{}</text><text>{}</text></binding></visual><actions><action content="Open" arguments="open"/><action content="Show in folder" arguments="folder"/></actions></toast>"#,
        escape(&cap(title, 120)),
        escape(&cap(body, 400)),
    );
    if let Err(e) = present(&xml, "download", "", on_click) {
        eprintln!("[wach] toast failed: {e}");
    }
}

fn present(xml: &str, group: &str, tag: &str, on_click: impl FnOnce(String) + Send + 'static) -> windows::core::Result<()> {
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    // Same chat → the new toast replaces the old one. Tag ≤ 64 chars.
    let tag = toast_tag(tag);
    if !tag.is_empty() {
        toast.SetTag(&HSTRING::from(tag))?;
        toast.SetGroup(&HSTRING::from(cap(group, 64)))?;
    }
    let once = Mutex::new(Some(on_click));
    toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(move |_, args| {
        // Which button (or the toast itself, via `launch`) was clicked.
        let arg = args
            .as_ref()
            .and_then(|a| a.cast::<ToastActivatedEventArgs>().ok())
            .and_then(|a| a.Arguments().ok())
            .map(|s| s.to_string())
            .unwrap_or_default();
        if let Some(f) = once.lock().unwrap().take() {
            f(arg);
        }
        Ok(())
    }))?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?.Show(&toast)?;
    let mut live = LIVE.lock().unwrap();
    live.push(toast);
    if live.len() > 30 {
        live.remove(0);
    }
    Ok(())
}

fn cap(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control() || *c == '\n').take(max).collect()
}

fn toast_tag(tag: &str) -> String {
    tag.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@')).take(64).collect()
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_and_limits() {
        assert_eq!(escape(r#"<b>"A" & 'B'</b>"#), "&lt;b&gt;&quot;A&quot; &amp; &apos;B&apos;&lt;/b&gt;");
        assert_eq!(cap("a\u{7}b", 10), "ab");
        assert_eq!(cap(&"x".repeat(500), 400).len(), 400);
        assert_eq!(toast_tag("chat:123@c.us<x>"), "chat123@c.usx");
        assert_eq!(toast_tag(&"a".repeat(100)).len(), 64);
    }
}
