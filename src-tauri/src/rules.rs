//! Small pure rules, kept apart so they can be unit tested.

use tauri::Url;

pub const WHATSAPP_ORIGIN: &str = "https://web.whatsapp.com";

/// What to do with a navigation inside an account webview.
#[derive(Debug, PartialEq, Eq)]
pub enum Nav {
    /// Stay in the webview.
    Allow,
    /// Cancel, and open this URL in the default browser.
    External(String),
    /// Cancel and do nothing.
    Block,
}

/// Account webviews may only show WhatsApp Web (and the blank page WebView2
/// uses internally). Safe http(s) links go to the browser; everything else is dropped.
pub fn navigation(url: &Url) -> Nav {
    if is_whatsapp(url) || url.as_str() == "about:blank" {
        return Nav::Allow;
    }
    match external_link(url) {
        Some(u) => Nav::External(u),
        None => Nav::Block,
    }
}

pub fn is_whatsapp(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("web.whatsapp.com")
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
}

/// A link we are willing to hand to the default browser.
pub fn external_link(url: &Url) -> Option<String> {
    let ok_scheme = matches!(url.scheme(), "http" | "https");
    let has_host = url.host_str().is_some_and(|h| !h.is_empty());
    let no_creds = url.username().is_empty() && url.password().is_none();
    (ok_scheme && has_host && no_creds).then(|| url.to_string())
}

/// WhatsApp puts the unread count in the title: "(12) WhatsApp".
pub fn unread_from_title(title: &str) -> u32 {
    let t = title.trim_start();
    let Some(rest) = t.strip_prefix('(') else { return 0 };
    let Some(end) = rest.find(')') else { return 0 };
    let digits = &rest[..end];
    if digits.is_empty() || digits.len() > 6 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return 0;
    }
    digits.parse::<u32>().map(|n| n.min(100_000)).unwrap_or(0)
}

/// Account ids double as folder names, so they are strict.
pub fn valid_id(id: &str) -> bool {
    id.len() == 12 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The 16-byte key Tauri's `data_store_identifier` takes: the id's bytes,
/// zero-padded. Our patched tauri-runtime-wry turns it into the WebView2
/// profile name (see `profile_name`).
pub fn profile_key(id: &str) -> Option<[u8; 16]> {
    if !valid_id(id) {
        return None;
    }
    let mut key = [0u8; 16];
    key[..id.len()].copy_from_slice(id.as_bytes());
    Some(key)
}

/// The WebView2 profile name the patch makes from `profile_key`: lowercase
/// base32 of the id (20 chars). Lowercase so ids that differ only in case
/// never share a folder; short because Chromium's cache paths come close to
/// Windows' 260-character limit. Must match the patch exactly.
pub fn profile_name(id: &str) -> Option<String> {
    profile_key(id)?;
    Some(base32(id.as_bytes()))
}

/// RFC 4648 base32, lowercase, no padding.
fn base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let (mut name, mut acc, mut bits) = (String::new(), 0u32, 0u32);
    for &b in bytes {
        acc = (acc << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            name.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        name.push(ALPHABET[((acc << (5 - bits)) & 31) as usize] as char);
    }
    name
}

/// The hex profile name used before 0.2 (for moving old profile folders).
pub fn old_profile_name(id: &str) -> Option<String> {
    profile_key(id).map(|k| k.iter().map(|b| format!("{b:02x}")).collect())
}

/// "Mute all" choices from the tray and Settings → the `muted_until` value.
pub fn mute_until(choice: &str, now: u64) -> Option<u64> {
    match choice {
        "off" => Some(0),
        "hour" => Some(now + 3600),
        "forever" => Some(u64::MAX),
        _ => None,
    }
}

pub fn is_muted(muted_until: u64, now: u64) -> bool {
    muted_until > now
}

pub fn new_id() -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut bytes = [0u8; 12];
    getrandom::fill(&mut bytes).expect("system random generator");
    bytes.iter().map(|b| ALPHABET[(b & 63) as usize] as char).collect()
}

/// A rectangle (logical pixels) the UI asks us to place a webview in.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn is_sane(&self) -> bool {
        let fin = [self.x, self.y, self.w, self.h].iter().all(|v| v.is_finite());
        fin && self.x >= 0.0
            && self.y >= 0.0
            && self.w >= 1.0
            && self.h >= 1.0
            && self.x + self.w <= 20_000.0
            && self.y + self.h <= 20_000.0
    }
}

/// Account display names: trimmed, no control characters, 1–40 chars.
pub fn clean_name(name: &str) -> Option<String> {
    let s: String = name.chars().filter(|c| !c.is_control()).collect();
    let s = s.trim();
    let n = s.chars().count();
    (1..=40).contains(&n).then(|| s.to_string())
}

pub const ACCOUNT_COLORS: [&str; 6] = ["sky", "violet", "mint", "amber", "rose", "teal"];

pub fn valid_color(c: &str) -> bool {
    ACCOUNT_COLORS.contains(&c)
}

/// Saved window rectangle must overlap a monitor by a usable amount,
/// else we centre on the primary one. Rects are (x, y, w, h) in physical px.
pub fn visible_on_any(win: (i32, i32, u32, u32), monitors: &[(i32, i32, u32, u32)]) -> bool {
    const MIN: i64 = 120;
    let (wx, wy, ww, wh) = (win.0 as i64, win.1 as i64, win.2 as i64, win.3 as i64);
    monitors.iter().any(|&(mx, my, mw, mh)| {
        let (mx, my, mw, mh) = (mx as i64, my as i64, mw as i64, mh as i64);
        let ix = (wx + ww).min(mx + mw) - wx.max(mx);
        let iy = (wy + wh).min(my + mh) - wy.max(my);
        // The title bar strip (top 40 px) must be reachable.
        let top_ok = wy >= my - 8 && wy < my + mh - 40;
        ix >= MIN && iy >= 40 && top_ok
    })
}

/// The permission kinds we care about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Perm {
    Notifications,
    Microphone,
    Camera,
    ClipboardRead,
    Other,
}

/// Navigation is locked to WhatsApp Web, and cross-origin frames cannot ask
/// for mic/camera unless WhatsApp delegates it, so the origin is implied.
/// Everything not listed is refused; WebView2's own prompt is never shown.
pub fn permission(kind: Perm, notifications_on: bool, media_on: bool) -> bool {
    match kind {
        Perm::Notifications => notifications_on,
        Perm::Microphone | Perm::Camera => media_on,
        // WhatsApp reads the clipboard to paste images.
        Perm::ClipboardRead => true,
        Perm::Other => false,
    }
}

/// Crash reload limit: at most `max` reloads within `window_secs`.
pub fn may_reload(history: &mut Vec<u64>, now: u64, max: usize, window_secs: u64) -> bool {
    history.retain(|&t| now.saturating_sub(t) < window_secs);
    if history.len() >= max {
        return false;
    }
    history.push(now);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn muting() {
        let now = 1_000_000;
        assert!(!is_muted(0, now));
        assert!(is_muted(mute_until("hour", now).unwrap(), now + 3599));
        assert!(!is_muted(mute_until("hour", now).unwrap(), now + 3600));
        assert!(is_muted(mute_until("forever", now).unwrap(), u64::MAX - 1));
        assert_eq!(mute_until("off", now), Some(0));
        assert_eq!(mute_until("bogus", now), None);
    }

    #[test]
    fn profile_keys() {
        let k = profile_key("Ab3_-xyz0123").unwrap();
        assert_eq!(&k[..12], b"Ab3_-xyz0123");
        assert_eq!(&k[12..], &[0, 0, 0, 0]);
        assert!(profile_key("../../evil00").is_none());
        assert!(profile_key("short").is_none());
        assert_ne!(profile_name("abcdefghijkl"), profile_name("ABCDEFGHIJKL"));
        // RFC 4648 test vectors (lowercase, unpadded).
        assert_eq!(base32(b"f"), "my");
        assert_eq!(base32(b"foob"), "mzxw6yq");
        assert_eq!(base32(b"foobar"), "mzxw6ytboi");
        assert_eq!(profile_name("Ab3_-xyz0123").unwrap().len(), 20);
        assert_eq!(old_profile_name("AAAAAAAAAAAA").unwrap(), format!("{}{}", "41".repeat(12), "00".repeat(4)));
    }

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn navigation_rules() {
        assert_eq!(navigation(&u("https://web.whatsapp.com/")), Nav::Allow);
        assert_eq!(navigation(&u("https://web.whatsapp.com:443/x?y")), Nav::Allow);
        assert_eq!(navigation(&u("about:blank")), Nav::Allow);
        assert_eq!(navigation(&u("https://web.whatsapp.com:8443/")), Nav::External("https://web.whatsapp.com:8443/".into()));
        assert_eq!(navigation(&u("http://web.whatsapp.com/")), Nav::External("http://web.whatsapp.com/".into()));
        assert_eq!(navigation(&u("https://evil.com/")), Nav::External("https://evil.com/".into()));
        assert_eq!(navigation(&u("https://web.whatsapp.com.evil.com/")), Nav::External("https://web.whatsapp.com.evil.com/".into()));
        assert_eq!(navigation(&u("https://user:pw@web.whatsapp.com/")), Nav::Block);
        assert_eq!(navigation(&u("https://user@example.com/")), Nav::Block);
        assert_eq!(navigation(&u("file:///C:/Windows/")), Nav::Block);
        assert_eq!(navigation(&u("javascript:alert(1)")), Nav::Block);
        assert_eq!(navigation(&u("ms-settings:privacy")), Nav::Block);
        assert_eq!(navigation(&u("data:text/html,hi")), Nav::Block);
    }

    #[test]
    fn unread_parsing() {
        assert_eq!(unread_from_title("WhatsApp"), 0);
        assert_eq!(unread_from_title("(3) WhatsApp"), 3);
        assert_eq!(unread_from_title("(120) WhatsApp"), 120);
        assert_eq!(unread_from_title("(999999) WhatsApp"), 100_000);
        assert_eq!(unread_from_title("(1234567) WhatsApp"), 0);
        assert_eq!(unread_from_title("(abc) WhatsApp"), 0);
        assert_eq!(unread_from_title("() WhatsApp"), 0);
        assert_eq!(unread_from_title("(-1) WhatsApp"), 0);
        assert_eq!(unread_from_title("WhatsApp (3)"), 0);
    }

    #[test]
    fn ids() {
        let id = new_id();
        assert!(valid_id(&id), "{id}");
        assert_ne!(new_id(), new_id());
        assert!(!valid_id("short"));
        assert!(!valid_id("../../etc/pw"));
        assert!(!valid_id("abcdefghij.."));
        assert!(!valid_id("abcdefghijk\\"));
        assert!(valid_id("Ab3_-xYz0123"));
    }

    #[test]
    fn rects() {
        assert!(Rect { x: 80.0, y: 92.0, w: 600.0, h: 700.0 }.is_sane());
        assert!(!Rect { x: -1.0, y: 0.0, w: 10.0, h: 10.0 }.is_sane());
        assert!(!Rect { x: 0.0, y: 0.0, w: 0.0, h: 10.0 }.is_sane());
        assert!(!Rect { x: f64::NAN, y: 0.0, w: 10.0, h: 10.0 }.is_sane());
        assert!(!Rect { x: 0.0, y: 0.0, w: 30_000.0, h: 10.0 }.is_sane());
    }

    #[test]
    fn names_and_colors() {
        assert_eq!(clean_name("  Work  ").as_deref(), Some("Work"));
        assert_eq!(clean_name("Wo\u{0}rk\n").as_deref(), Some("Work"));
        assert_eq!(clean_name("   "), None);
        assert_eq!(clean_name(&"x".repeat(41)), None);
        assert!(valid_color("sky"));
        assert!(!valid_color("#ff0000"));
    }

    #[test]
    fn monitor_visibility() {
        let mons = [(0, 0, 1920, 1080), (1920, 0, 2560, 1440)];
        assert!(visible_on_any((100, 100, 1100, 760), &mons));
        assert!(visible_on_any((2000, 50, 1100, 760), &mons));
        assert!(!visible_on_any((5000, 100, 1100, 760), &mons));
        assert!(!visible_on_any((100, -700, 1100, 760), &mons));
        assert!(!visible_on_any((-1050, 100, 1100, 760), &mons));
        assert!(!visible_on_any((100, 100, 1100, 760), &[]));
    }

    #[test]
    fn permissions() {
        assert!(permission(Perm::Notifications, true, false));
        assert!(!permission(Perm::Notifications, false, true));
        assert!(permission(Perm::Microphone, false, true));
        assert!(!permission(Perm::Camera, true, false));
        assert!(!permission(Perm::Other, true, true));
    }

    #[test]
    fn reload_limit() {
        let mut h = Vec::new();
        assert!(may_reload(&mut h, 100, 3, 60));
        assert!(may_reload(&mut h, 101, 3, 60));
        assert!(may_reload(&mut h, 102, 3, 60));
        assert!(!may_reload(&mut h, 103, 3, 60));
        assert!(may_reload(&mut h, 170, 3, 60));
    }
}
