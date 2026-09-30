// SPDX-License-Identifier: GPL-3.0-only
use crate::keys::Modifier;
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Word,
    Phrase,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub sup: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub mods: Mods,
    pub key: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkeys {
    pub word: Hotkey,
    pub phrase: Hotkey,
}

/// Keys that make sense as a hotkey trigger (they type nothing).
const KEY_NAMES: &[(&str, u16)] = &[
    ("Insert", 110),
    ("Pause", 119),
    ("ScrollLock", 70),
    ("Menu", 127),
    ("F1", 59),
    ("F2", 60),
    ("F3", 61),
    ("F4", 62),
    ("F5", 63),
    ("F6", 64),
    ("F7", 65),
    ("F8", 66),
    ("F9", 67),
    ("F10", 68),
    ("F11", 87),
    ("F12", 88),
];

impl Mods {
    #[must_use]
    pub fn any(self) -> bool {
        self.shift || self.ctrl || self.alt || self.sup
    }

    // ponytail: one flag per modifier, so releasing one of two held Shifts
    // clears it. Count per side if anyone holds both.
    pub fn set(&mut self, m: Modifier, down: bool) {
        match m {
            Modifier::Shift => self.shift = down,
            Modifier::Ctrl => self.ctrl = down,
            Modifier::Alt => self.alt = down,
            Modifier::Super => self.sup = down,
        }
    }
}

impl FromStr for Hotkey {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts: Vec<&str> = s.split('+').map(str::trim).collect();
        let key = parts
            .pop()
            .filter(|k| !k.is_empty())
            .ok_or("empty hotkey")?;
        let mut mods = Mods::default();
        for p in parts {
            match p.to_ascii_lowercase().as_str() {
                "shift" => mods.shift = true,
                "ctrl" | "control" => mods.ctrl = true,
                "alt" => mods.alt = true,
                "super" | "win" | "meta" => mods.sup = true,
                _ => return Err(format!("unknown modifier: {p}")),
            }
        }
        let key = KEY_NAMES
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(key))
            .map(|&(_, c)| c)
            .ok_or_else(|| format!("unsupported key: {key}"))?;
        Ok(Self { mods, key })
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.mods.sup, "Super"),
            (self.mods.ctrl, "Ctrl"),
            (self.mods.alt, "Alt"),
            (self.mods.shift, "Shift"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        let name = KEY_NAMES
            .iter()
            .find(|&&(_, c)| c == self.key)
            .map_or("?", |&(n, _)| n);
        f.write_str(name)
    }
}

pub const DEFAULT_WORD: &str = "Insert";
pub const DEFAULT_PHRASE: &str = "Super+Insert";

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            word: DEFAULT_WORD.parse().expect("valid default"),
            phrase: DEFAULT_PHRASE.parse().expect("valid default"),
        }
    }
}

impl Hotkeys {
    #[must_use]
    pub fn matches(&self, key: u16, mods: Mods) -> Option<Scope> {
        let hit = |h: Hotkey| h.key == key && h.mods == mods;
        if hit(self.word) {
            Some(Scope::Word)
        } else if hit(self.phrase) {
            Some(Scope::Phrase)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_key_case_insensitive() {
        let h: Hotkey = "insert".parse().unwrap();
        assert_eq!(
            h,
            Hotkey {
                mods: Mods::default(),
                key: 110
            }
        );
    }

    #[test]
    fn parses_modifiers() {
        let h: Hotkey = "Ctrl + Alt+F12".parse().unwrap();
        assert!(h.mods.ctrl && h.mods.alt && !h.mods.shift && !h.mods.sup);
        assert_eq!(h.key, 88);
    }

    #[test]
    fn rejects_unknown_key_and_modifier() {
        assert!("Foo".parse::<Hotkey>().is_err());
        assert!("Hyper+Insert".parse::<Hotkey>().is_err());
        assert!("".parse::<Hotkey>().is_err());
    }

    #[test]
    fn display_round_trips() {
        for s in ["Insert", "Super+Insert", "Super+Ctrl+Alt+Shift+F1"] {
            assert_eq!(s.parse::<Hotkey>().unwrap().to_string(), s);
        }
    }

    #[test]
    fn defaults_and_matching() {
        let h = Hotkeys::default();
        assert_eq!(h.matches(110, Mods::default()), Some(Scope::Word));
        let sup = Mods {
            sup: true,
            ..Mods::default()
        };
        assert_eq!(h.matches(110, sup), Some(Scope::Phrase));
        let shift = Mods {
            shift: true,
            ..Mods::default()
        };
        assert_eq!(
            h.matches(110, shift),
            None,
            "Shift+Insert is paste, not ours"
        );
    }
}
