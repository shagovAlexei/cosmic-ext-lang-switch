// SPDX-License-Identifier: GPL-3.0-only
//! Per-layout character tables, built from the compositor's xkb config.
use lsc::config::{Xkb, labels};
use lsc::engine::Stroke;
use lsc::keys::{Kind, kind};
use lsc::selection::Table;
use xkbcommon::xkb;

/// Per layout group: (panel label, xkb layout code, human description from the xkb
/// registry). A layout missing from the registry is described by its code.
pub fn describe(cfg: &Xkb) -> Vec<(String, String, String)> {
    let registry = xkb_data::all_keyboard_layouts().ok();
    let mut variants = cfg.variant.split(',');
    cfg.layout
        .split(',')
        .zip(labels(cfg))
        .map(|(layout, label)| {
            let variant = variants.next().unwrap_or("");
            let entry = registry
                .as_ref()
                .and_then(|r| r.layouts().iter().find(|l| l.name() == layout));
            let description = entry.map(|l| {
                l.variants()
                    .and_then(|vs| vs.iter().find(|v| v.name() == variant))
                    .map_or(l.description(), |v| v.description())
            });
            (
                label,
                layout.to_owned(),
                description.unwrap_or(layout).to_owned(),
            )
        })
        .collect()
}

/// Dictionary words (hunspell stems, no affix expansion) that auto-correction must
/// never touch when typed as-is. Returns the veto and how many words it holds; a
/// missing dictionary just leaves that language unprotected.
pub fn load_veto(dir: &std::path::Path) -> (lsc::auto::Veto, usize) {
    let mut veto = lsc::auto::Veto::default();
    let mut count = 0;
    for (file, lang) in [
        ("en_US.dic", lsc::auto::Lang::En),
        ("ru_RU.dic", lsc::auto::Lang::Ru),
    ] {
        match std::fs::read_to_string(dir.join(file)) {
            Ok(text) => {
                let words: Vec<&str> = text
                    .lines()
                    .skip(1)
                    .filter_map(|l| l.split('/').next())
                    .collect();
                count += words.len();
                veto.extend(lang, words);
            }
            Err(e) => log::warn!("{file}: {e}; auto-correction has no dictionary veto for it"),
        }
    }
    (veto, count)
}

/// One table per layout group: which printable key (plain or with Shift) types each
/// character. Empty if xkb can't compile the config.
pub fn build(cfg: &Xkb) -> Vec<Table> {
    let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let Some(keymap) = xkb::Keymap::new_from_names(
        &ctx,
        "",
        "",
        cfg.layout.as_str(),
        cfg.variant.as_str(),
        None,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    ) else {
        return Vec::new();
    };
    (0..keymap.num_layouts())
        .map(|group| {
            let mut table = Table::new();
            for code in (1..=255u16).filter(|&c| kind(c) == Kind::Printable) {
                // xkb keycodes are evdev codes + 8.
                let key = xkb::Keycode::new(u32::from(code) + 8);
                for (level, shift) in [(0, false), (1, true)] {
                    for &sym in keymap.key_get_syms_by_level(key, group, level) {
                        if let Some(c) =
                            char::from_u32(xkb::keysym_to_utf32(sym)).filter(|&c| c != '\0')
                        {
                            table.entry(c).or_insert(Stroke { code, shift });
                        }
                    }
                }
            }
            table
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us_and_russian_share_physical_keys() {
        let t = build(&Xkb {
            layout: "us,by".into(),
            variant: ",ru".into(),
        });
        assert_eq!(t.len(), 2);
        assert_eq!(
            t[0][&'q'],
            Stroke {
                code: 16,
                shift: false
            }
        );
        assert_eq!(
            t[0][&'Q'],
            Stroke {
                code: 16,
                shift: true
            }
        );
        assert_eq!(
            t[1][&'й'],
            Stroke {
                code: 16,
                shift: false
            }
        );
        assert_eq!(
            t[1][&'№'],
            Stroke {
                code: 4,
                shift: true
            }
        );
        assert_eq!(
            t[0][&' '],
            Stroke {
                code: 57,
                shift: false
            }
        );
    }

    #[test]
    fn layouts_are_described() {
        let d = describe(&Xkb {
            layout: "us,by".into(),
            variant: ",ru".into(),
        });
        assert_eq!(
            d,
            [
                ("US".into(), "us".into(), "English (US)".into()),
                ("RU".into(), "by".into(), "Russian (Belarus)".into()),
            ]
        );
    }

    #[test]
    fn unknown_layout_falls_back_to_its_code() {
        let d = describe(&Xkb {
            layout: "zz".into(),
            variant: String::new(),
        });
        assert_eq!(d, [("ZZ".into(), "zz".into(), "zz".into())]);
    }

    #[test]
    fn veto_reads_hunspell_stems() {
        let dir = std::env::temp_dir().join(format!("lsw-veto-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("en_US.dic"), "2\nhello/MS\nGrep\n").unwrap();
        let (veto, count) = load_veto(&dir);
        assert_eq!(count, 2);
        assert!(veto.contains(lsc::auto::Lang::En, "hello"));
        assert!(veto.contains(lsc::auto::Lang::En, "grep"));
        assert!(
            !veto.contains(lsc::auto::Lang::Ru, "hello"),
            "ru_RU.dic missing: empty"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bad_layout_gives_no_tables() {
        assert!(
            build(&Xkb {
                layout: "no-such-layout".into(),
                variant: String::new()
            })
            .is_empty()
        );
    }
}
