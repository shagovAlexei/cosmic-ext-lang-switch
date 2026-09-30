// SPDX-License-Identifier: GPL-3.0-only
use crate::hotkey::{DEFAULT_PHRASE, DEFAULT_WORD, Hotkeys};
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            hotkey_word: DEFAULT_WORD.into(),
            hotkey_phrase: DEFAULT_PHRASE.into(),
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
    fn custom_hotkey_is_used() {
        let c = Config {
            hotkey_word: "F9".into(),
            ..Config::default()
        };
        assert_eq!(c.hotkeys().word, "F9".parse::<Hotkey>().unwrap());
    }
}
