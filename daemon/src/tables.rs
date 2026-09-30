// SPDX-License-Identifier: GPL-3.0-only
//! Per-layout character tables, built from the compositor's xkb config.
use lsc::config::Xkb;
use lsc::engine::Stroke;
use lsc::keys::{Kind, kind};
use lsc::selection::Table;
use xkbcommon::xkb;

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
