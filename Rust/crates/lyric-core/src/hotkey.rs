//! One canonical spelling for a global shortcut.
//!
//! The settings window records a combination and the native side hands it to `RegisterHotKey`.
//! If the two disagreed about what a string means, a binding the recorder accepted could be one
//! the backend silently dropped — the UI would show a shortcut that never fires. So both sides
//! call this module instead of each keeping its own match arm.
//!
//! The shape is deliberately narrow: `RegisterHotKey` takes a set of modifier flags plus exactly
//! one virtual key, so anything else (a bare key, two keys, a sequence) is rejected here rather
//! than being accepted by the UI and refused later.

/// A modifier set plus one key — the only shape `RegisterHotKey` supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    /// Windows virtual-key code. Letters, digits and F1–F24 only.
    pub vk: u32,
}

/// Longest spelling accepted, so a hand-edited settings file cannot make us allocate freely.
pub const MAX_LEN: usize = 64;

impl Hotkey {
    /// Parse `Ctrl+Alt+R` and its many spellings. Returns `None` for anything the recorder
    /// should refuse — including an empty string, which callers treat as "no shortcut".
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() > MAX_LEN {
            return None;
        }
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut win = false;
        let mut key: Option<u32> = None;
        let mut tokens = 0usize;
        for raw in text.split('+') {
            let token = raw.trim();
            // An empty token means a doubled `+` or a trailing one; both are typos, not a key.
            if token.is_empty() {
                return None;
            }
            tokens += 1;
            let lower = token.to_ascii_lowercase();
            match lower.as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                "win" | "windows" | "super" | "meta" => win = true,
                _ => {
                    // A second non-modifier token would be a sequence, which is not a hotkey.
                    if key.is_some() {
                        return None;
                    }
                    key = Some(virtual_key(token)?);
                }
            }
        }
        if tokens < 2 {
            return None;
        }
        // At least one modifier: a bare key would be grabbed globally for the whole desktop,
        // which is never what someone means when they open this recorder.
        if !(ctrl || alt || shift || win) {
            return None;
        }
        Some(Self { ctrl, alt, shift, win, vk: key? })
    }

    /// The canonical spelling: modifiers in a fixed order, then the key.
    pub fn label(self) -> String {
        let mut parts: Vec<&str> = Vec::with_capacity(5);
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.win {
            parts.push("Win");
        }
        let key = key_label(self.vk);
        parts.push(key.as_str());
        parts.join("+")
    }
}

/// Canonical spelling, or `None` if this is not a shortcut we can register.
pub fn normalize(text: &str) -> Option<String> {
    Hotkey::parse(text).map(Hotkey::label)
}

fn virtual_key(token: &str) -> Option<u32> {
    let upper = token.to_ascii_uppercase();
    let mut chars = upper.chars();
    let first = chars.next()?;
    let rest: String = chars.collect();
    if rest.is_empty() {
        if first.is_ascii_uppercase() {
            return Some(first as u32); // 0x41..=0x5A
        }
        if let Some(digit) = first.to_digit(10) {
            return Some(0x30 + digit); // 0x30..=0x39
        }
        return None;
    }
    // F1–F24 only; `RegisterHotKey` also accepts F25 but no keyboard has one.
    if first == 'F' {
        let number = rest.parse::<u32>().ok()?;
        if (1..=24).contains(&number) {
            return Some(0x70 + number - 1);
        }
    }
    None
}

fn key_label(vk: u32) -> String {
    if (0x41..=0x5A).contains(&vk) {
        return char::from_u32(vk).map(|c| c.to_string()).unwrap_or_default();
    }
    if (0x30..=0x39).contains(&vk) {
        return char::from_u32(vk).map(|c| c.to_string()).unwrap_or_default();
    }
    if (0x70..=0x87).contains(&vk) {
        return format!("F{}", vk - 0x70 + 1);
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_presets_survive_a_round_trip() {
        for preset in ["Ctrl+Alt+R", "Ctrl+Shift+R", "Ctrl+Alt+N"] {
            assert_eq!(normalize(preset).as_deref(), Some(preset), "{preset}");
        }
    }

    #[test]
    fn spelling_is_case_insensitive_and_order_is_canonical() {
        assert_eq!(normalize("ctrl+alt+r").as_deref(), Some("Ctrl+Alt+R"));
        assert_eq!(normalize("Alt+Ctrl+R").as_deref(), Some("Ctrl+Alt+R"));
        assert_eq!(normalize("SHIFT+CTRL+F12").as_deref(), Some("Ctrl+Shift+F12"));
        assert_eq!(normalize("control + alt + n").as_deref(), Some("Ctrl+Alt+N"));
    }

    #[test]
    fn digits_and_function_keys_are_accepted() {
        assert_eq!(normalize("Ctrl+Alt+7").as_deref(), Some("Ctrl+Alt+7"));
        assert_eq!(normalize("Win+F24").as_deref(), Some("Win+F24"));
    }

    #[test]
    fn shapes_register_hot_key_cannot_take_are_refused() {
        assert_eq!(normalize(""), None, "empty is 'no shortcut', not a binding");
        assert_eq!(normalize("Ctrl+Alt"), None, "modifiers alone are not a key");
        assert_eq!(normalize("R"), None, "a bare key would be grabbed globally");
        assert_eq!(normalize("Ctrl+R+T"), None, "a sequence is not a hotkey");
        assert_eq!(normalize("Ctrl+Foo"), None);
        assert_eq!(normalize("Ctrl+F25"), None);
        assert_eq!(normalize("Ctrl++R"), None);
        assert_eq!(normalize("Ctrl+R+"), None);
        assert_eq!(normalize(&"Ctrl+".repeat(40)), None, "over-long input is refused");
    }

    #[test]
    fn the_virtual_key_codes_match_windows() {
        assert_eq!(Hotkey::parse("Ctrl+A").unwrap().vk, 0x41);
        assert_eq!(Hotkey::parse("Ctrl+0").unwrap().vk, 0x30);
        assert_eq!(Hotkey::parse("Ctrl+F1").unwrap().vk, 0x70);
        assert_eq!(Hotkey::parse("Ctrl+F12").unwrap().vk, 0x7B);
    }
}
