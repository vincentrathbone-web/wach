//! Keyboard shortcuts that must work while a WhatsApp pane has focus.

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case", tag = "action", content = "n")]
pub enum Action {
    ToggleSplit,
    OtherPane,
    Swap,
    Account(u8),
    ZoomIn,
    ZoomOut,
    ZoomReset,
}

/// Windows virtual-key codes → action. Alt combos are left to WhatsApp.
pub fn from_key(vk: u32, ctrl: bool, shift: bool, alt: bool) -> Option<Action> {
    if !ctrl || alt {
        return None;
    }
    Some(match (vk, shift) {
        (0xDC, false) => Action::ToggleSplit,                // Ctrl+\
        (0x09, _) => Action::OtherPane,                       // Ctrl+Tab / Ctrl+Shift+Tab
        (0x55, false) => Action::Swap,                        // Ctrl+U
        (0x31..=0x39, false) => Action::Account((vk - 0x30) as u8), // Ctrl+1…9
        (0xBB | 0x6B, _) => Action::ZoomIn,                   // Ctrl+= / Ctrl+numpad +
        (0xBD | 0x6D, false) => Action::ZoomOut,              // Ctrl+- / Ctrl+numpad -
        (0x30 | 0x60, false) => Action::ZoomReset,            // Ctrl+0
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping() {
        assert_eq!(from_key(0xDC, true, false, false), Some(Action::ToggleSplit));
        assert_eq!(from_key(0x09, true, true, false), Some(Action::OtherPane));
        assert_eq!(from_key(0x33, true, false, false), Some(Action::Account(3)));
        assert_eq!(from_key(0x33, false, false, false), None);
        assert_eq!(from_key(0x33, true, false, true), None);
        assert_eq!(from_key(0x41, true, false, false), None); // Ctrl+A stays with WhatsApp
        assert_eq!(serde_json::to_string(&Action::Account(2)).unwrap(), r#"{"action":"account","n":2}"#);
        assert_eq!(serde_json::to_string(&Action::Swap).unwrap(), r#"{"action":"swap"}"#);
    }
}
