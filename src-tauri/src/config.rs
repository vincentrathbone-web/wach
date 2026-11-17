//! `config.json` and `closed.json` in %LOCALAPPDATA%\WACH\.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::rules;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Account {
    pub id: String,
    pub name: String,
    /// A key from `rules::ACCOUNT_COLORS`; the UI maps it to a token.
    pub color: String,
    pub notifications: bool,
    pub media: bool,
    pub zoom: f64,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            color: "sky".into(),
            notifications: true,
            media: true,
            zoom: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Layout {
    pub split: bool,
    pub left: Option<String>,
    pub right: Option<String>,
    /// 0 = left pane has focus, 1 = right.
    pub focused: u8,
    /// Width share of the left pane, 0.2–0.8.
    pub ratio: f64,
}

impl Default for Layout {
    fn default() -> Self {
        Self { split: true, left: None, right: None, focused: 0, ratio: 0.5 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub maximized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Theme {
    pub name: String,
    /// Window background before the UI paints (matches `--color-navy`).
    pub background: [u8; 3],
    /// Unread dot on tray and taskbar (matches `--color-alert`).
    pub alert: [u8; 3],
}

impl Default for Theme {
    fn default() -> Self {
        Self { name: "night-signal".into(), background: [0x0B, 0x12, 0x24], alert: [0xFF, 0x6B, 0x81] }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub close_to_tray: bool,
    /// Glow strength in px, 0–48.
    pub glow: u32,
    pub theme: Theme,
}

impl Default for Settings {
    fn default() -> Self {
        Self { close_to_tray: true, glow: 24, theme: Theme::default() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub accounts: Vec<Account>,
    pub layout: Layout,
    pub window: Option<WindowState>,
    pub settings: Settings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            accounts: Vec::new(),
            layout: Layout::default(),
            window: None,
            settings: Settings::default(),
        }
    }
}

impl Config {
    /// Drop anything unsafe or inconsistent that may have been hand-edited.
    pub fn sanitize(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.accounts.retain(|a| rules::valid_id(&a.id) && seen.insert(a.id.clone()));
        for a in &mut self.accounts {
            a.name = rules::clean_name(&a.name).unwrap_or_else(|| "Account".into());
            if !rules::valid_color(&a.color) {
                a.color = "sky".into();
            }
            if !a.zoom.is_finite() {
                a.zoom = 1.0;
            }
            a.zoom = a.zoom.clamp(0.5, 2.5);
        }
        let ids: Vec<String> = self.accounts.iter().map(|a| a.id.clone()).collect();
        let l = &mut self.layout;
        if l.left.as_ref().is_some_and(|id| !ids.contains(id)) {
            l.left = None;
        }
        if l.right.as_ref().is_some_and(|id| !ids.contains(id)) {
            l.right = None;
        }
        if l.left.is_some() && l.left == l.right {
            l.right = None;
        }
        if !l.ratio.is_finite() {
            l.ratio = 0.5;
        }
        l.ratio = l.ratio.clamp(0.2, 0.8);
        l.focused = l.focused.min(1);
        self.settings.glow = self.settings.glow.min(48);
    }

    pub fn account(&self, id: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    pub fn account_mut(&mut self, id: &str) -> Option<&mut Account> {
        self.accounts.iter_mut().find(|a| a.id == id)
    }
}

pub struct Store {
    pub root: PathBuf,
}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn accounts_dir(&self) -> PathBuf {
        self.root.join("accounts")
    }

    pub fn account_dir(&self, id: &str) -> Option<PathBuf> {
        rules::valid_id(id).then(|| self.accounts_dir().join(id))
    }

    pub fn load(&self) -> Config {
        let mut cfg: Config = fs::read_to_string(self.root.join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        cfg.sanitize();
        cfg
    }

    pub fn save(&self, cfg: &Config) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(cfg).map_err(std::io::Error::other)?;
        write_atomic(&self.root.join("config.json"), &json)
    }

    pub fn load_closed(&self) -> Vec<String> {
        fs::read_to_string(self.root.join("closed.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|id| rules::valid_id(id))
            .collect()
    }

    pub fn save_closed(&self, ids: &[String]) -> std::io::Result<()> {
        let json = serde_json::to_vec(ids).map_err(std::io::Error::other)?;
        write_atomic(&self.root.join("closed.json"), &json)
    }

    /// Delete data folders of accounts closed in an earlier run.
    /// Returns the ids that could not be deleted yet (kept for next time).
    pub fn cleanup_closed(&self, cfg: &Config) -> Vec<String> {
        let closed = self.load_closed();
        let plan = cleanup_plan(&closed, cfg);
        let mut left = Vec::new();
        for id in &plan.delete {
            if let Some(dir) = self.account_dir(id)
                && dir.exists()
                && fs::remove_dir_all(&dir).is_err()
            {
                left.push(id.clone());
            }
        }
        left.extend(plan.keep);
        let _ = self.save_closed(&left);
        left
    }
}

#[derive(Debug, PartialEq)]
pub struct CleanupPlan {
    pub delete: Vec<String>,
    pub keep: Vec<String>,
}

/// Safety rule from Altus +: never delete a folder that still belongs to an
/// account, and if many accounts vanished at once, assume something is wrong
/// with the config and delete nothing.
pub fn cleanup_plan(closed: &[String], cfg: &Config) -> CleanupPlan {
    const MAX_AT_ONCE: usize = 3;
    let in_use = |id: &String| cfg.accounts.iter().any(|a| &a.id == id);
    let candidates: Vec<String> = closed.iter().filter(|id| !in_use(id)).cloned().collect();
    if candidates.len() > MAX_AT_ONCE || (cfg.accounts.is_empty() && !candidates.is_empty() && candidates.len() > 1) {
        return CleanupPlan { delete: Vec::new(), keep: candidates };
    }
    CleanupPlan { delete: candidates, keep: Vec::new() }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acc(id: &str) -> Account {
        Account { id: id.into(), name: "A".into(), ..Default::default() }
    }

    #[test]
    fn sanitize_fixes_bad_values() {
        let mut c = Config {
            accounts: vec![acc("AAAAAAAAAAAA"), acc("AAAAAAAAAAAA"), acc("../bad"), Account { color: "#fff".into(), zoom: 9.0, ..acc("BBBBBBBBBBBB") }],
            layout: Layout { split: true, left: Some("AAAAAAAAAAAA".into()), right: Some("AAAAAAAAAAAA".into()), focused: 7, ratio: f64::NAN },
            ..Default::default()
        };
        c.settings.glow = 400;
        c.sanitize();
        assert_eq!(c.accounts.len(), 2);
        assert_eq!(c.accounts[1].color, "sky");
        assert_eq!(c.accounts[1].zoom, 2.5);
        assert_eq!(c.layout.right, None);
        assert_eq!(c.layout.focused, 1);
        assert_eq!(c.layout.ratio, 0.5);
        assert_eq!(c.settings.glow, 48);
    }

    #[test]
    fn cleanup_rules() {
        let cfg = Config { accounts: vec![acc("AAAAAAAAAAAA")], ..Default::default() };
        let p = cleanup_plan(&["AAAAAAAAAAAA".into(), "BBBBBBBBBBBB".into()], &cfg);
        assert_eq!(p.delete, vec!["BBBBBBBBBBBB".to_string()]);
        let many: Vec<String> = ["CCCCCCCCCCCC", "DDDDDDDDDDDD", "EEEEEEEEEEEE", "FFFFFFFFFFFF"].map(String::from).to_vec();
        let p = cleanup_plan(&many, &cfg);
        assert!(p.delete.is_empty());
        assert_eq!(p.keep.len(), 4);
        let empty = Config::default();
        let p = cleanup_plan(&many[..2], &empty);
        assert!(p.delete.is_empty());
        let p = cleanup_plan(&many[..1], &empty);
        assert_eq!(p.delete.len(), 1);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("wach-test-{}", rules::new_id()));
        let store = Store::new(dir.clone());
        let mut c = Config::default();
        c.accounts.push(acc("AAAAAAAAAAAA"));
        store.save(&c).unwrap();
        assert_eq!(store.load(), c);
        // A broken file falls back to defaults instead of failing.
        fs::write(dir.join("config.json"), b"{ not json").unwrap();
        assert_eq!(store.load(), Config::default());
        store.save_closed(&["AAAAAAAAAAAA".into(), "../x".into()]).unwrap();
        assert_eq!(store.load_closed(), vec!["AAAAAAAAAAAA".to_string()]);
        fs::remove_dir_all(dir).unwrap();
    }
}
