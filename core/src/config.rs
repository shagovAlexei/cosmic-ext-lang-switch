// SPDX-License-Identifier: GPL-3.0-only
use crate::hotkey::{DEFAULT_PHRASE, DEFAULT_SELECTION, DEFAULT_WORD, Hotkeys};
use crate::selection::Unknown;
use cosmic_config::{CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

pub const APP_ID: &str = "io.github.shagovAlexei.cosmic-ext-lang-switch";
pub const COMP_ID: &str = "com.system76.CosmicComp";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, CosmicConfigEntry)]
#[version = 1]
pub struct Config {
    pub enabled: bool,
    pub hotkey_word: String,
    pub hotkey_phrase: String,
    pub hotkey_selection: String,
    /// Retype nothing if the selection holds a character no layout can type.
    pub abort_on_unknown: bool,
    /// UI language: `""` follows the desktop, otherwise `"en"` or `"ru"`.
    pub language: String,
    /// Correct words typed in the wrong layout automatically, on space.
    pub auto_enabled: bool,
    /// App ids where auto-correction never runs; a trailing `*` matches a prefix.
    pub auto_excluded_apps: Vec<String>,
    /// Words (lowercase, as typed) the user undid: never auto-corrected again.
    pub auto_exceptions: Vec<String>,
}

/// Terminals and code editors: typed Latin there is commands and identifiers.
pub const DEFAULT_EXCLUDED_APPS: &[&str] = &[
    "com.system76.CosmicTerm",
    "Alacritty",
    "kitty",
    "org.wezfurlong.wezterm",
    "foot",
    "org.gnome.Console",
    "org.gnome.Terminal",
    "org.gnome.Ptyxis",
    "org.kde.konsole",
    "com.mitchellh.ghostty",
    "code",
    "com.microsoft.VSCode",
    "com.visualstudio.code",
    "codium",
    "dev.zed.Zed",
    "jetbrains-*",
    // Password prompts.
    "pinentry-*",
    "org.freedesktop.PolicyKit*",
];

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            hotkey_word: DEFAULT_WORD.into(),
            hotkey_phrase: DEFAULT_PHRASE.into(),
            hotkey_selection: DEFAULT_SELECTION.into(),
            abort_on_unknown: false,
            language: String::new(),
            auto_enabled: false,
            auto_excluded_apps: DEFAULT_EXCLUDED_APPS
                .iter()
                .map(|&a| a.to_owned())
                .collect(),
            auto_exceptions: Vec::new(),
        }
    }
}

/// The part of cosmic-comp's `xkb_config` we read. Unknown fields are ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Xkb {
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub variant: String,
}

impl Config {
    /// Unparseable hotkeys fall back to the defaults instead of disabling the key.
    #[must_use]
    pub fn hotkeys(&self) -> Hotkeys {
        let d = Hotkeys::default();
        Hotkeys {
            word: self.hotkey_word.parse().unwrap_or(d.word),
            phrase: self.hotkey_phrase.parse().unwrap_or(d.phrase),
            selection: self.hotkey_selection.parse().unwrap_or(d.selection),
        }
    }

    /// Is auto-correction off in this app (by the exclusion list)?
    #[must_use]
    pub fn app_excluded(&self, app_id: &str) -> bool {
        !app_id.is_empty()
            && self
                .auto_excluded_apps
                .iter()
                .any(|pattern| match pattern.strip_suffix('*') {
                    Some(prefix) => app_id.starts_with(prefix),
                    None => app_id == pattern,
                })
    }

    #[must_use]
    pub fn unknown(&self) -> Unknown {
        if self.abort_on_unknown {
            Unknown::Abort
        } else {
            Unknown::Keep
        }
    }
}

/// Panel label per layout group: a 2–3 letter variant wins (`by(ru)` → `RU`).
#[must_use]
pub fn labels(xkb: &Xkb) -> Vec<String> {
    let mut variants = xkb.variant.split(',');
    xkb.layout
        .split(',')
        .map(|layout| {
            let v = variants.next().unwrap_or("");
            if (2..=3).contains(&v.len()) {
                v
            } else {
                layout
            }
            .to_uppercase()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::Hotkey;

    #[test]
    fn reads_real_cosmic_comp_file() {
        let ron = r#"(
            rules: "", model: "", layout: "us,by", variant: ",ru",
            options: Some("lv3:ralt_switch"), repeat_delay: 600, repeat_rate: 25,
        )"#;
        let xkb: Xkb = ron::from_str(ron).unwrap();
        assert_eq!(labels(&xkb), ["US", "RU"]);
    }

    #[test]
    fn long_variants_fall_back_to_layout() {
        let xkb = Xkb {
            layout: "us,ru".into(),
            variant: "intl,".into(),
        };
        assert_eq!(labels(&xkb), ["US", "RU"]);
    }

    #[test]
    fn empty_variant_string() {
        let xkb = Xkb {
            layout: "us".into(),
            variant: String::new(),
        };
        assert_eq!(labels(&xkb), ["US"]);
    }

    #[test]
    fn bad_hotkey_in_config_falls_back_to_default() {
        let c = Config {
            hotkey_word: "Nope".into(),
            ..Config::default()
        };
        assert_eq!(c.hotkeys(), Hotkeys::default());
    }

    #[test]
    fn selection_defaults() {
        let c = Config::default();
        assert_eq!(
            c.hotkeys().selection,
            "Alt+Insert".parse::<Hotkey>().unwrap()
        );
        assert_eq!(c.unknown(), Unknown::Keep);
        let c = Config {
            abort_on_unknown: true,
            ..c
        };
        assert_eq!(c.unknown(), Unknown::Abort);
    }

    #[test]
    fn auto_is_off_and_excludes_terminals_and_code_editors_by_default() {
        let c = Config::default();
        assert!(!c.auto_enabled);
        assert!(c.app_excluded("com.system76.CosmicTerm"));
        assert!(c.app_excluded("jetbrains-idea"), "prefix pattern");
        assert!(!c.app_excluded("org.telegram.desktop"));
        assert!(!c.app_excluded(""), "unknown app: not excluded");
    }

    #[test]
    fn language_defaults_to_system() {
        assert_eq!(Config::default().language, "");
    }

    #[test]
    fn custom_hotkey_is_used() {
        let c = Config {
            hotkey_word: "F9".into(),
            ..Config::default()
        };
        assert_eq!(c.hotkeys().word, "F9".parse::<Hotkey>().unwrap());
    }
}
