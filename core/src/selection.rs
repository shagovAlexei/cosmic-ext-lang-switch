// SPDX-License-Identifier: GPL-3.0-only
//! Retyping selected text in the other layout. Pure: layout tables come from the daemon.

use crate::engine::Stroke;
use std::collections::HashMap;

/// Characters one layout group types, and the key that types each.
pub type Table = HashMap<char, Stroke>;

/// What to do with a character no layout can type (emoji and the like).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unknown {
    /// Leave it out of the retyped text.
    Keep,
    /// Retype nothing.
    Abort,
}

const ENTER: u16 = 28;
const TAB: u16 = 15;

/// Plans retyping `text` in the layout after the one it was typed in.
/// Returns the target group and the keys to press once that group is active.
/// "Keep" for an unknown character means it is skipped: a key can't type it.
#[must_use]
pub fn convert(
    text: &str,
    tables: &[Table],
    current: u32,
    unknown: Unknown,
) -> Option<(u32, Vec<Stroke>)> {
    if text.is_empty() || tables.len() < 2 {
        return None;
    }
    let hits = |t: &Table| text.chars().filter(|c| t.contains_key(c)).count();
    let current = current as usize % tables.len();
    // Most characters typeable wins; ties keep the current group.
    let source = (0..tables.len())
        .max_by_key(|&g| (hits(&tables[g]), g == current))
        .unwrap_or(current);
    let target = (source + 1) % tables.len();
    let mut keys = Vec::with_capacity(text.len());
    for c in text.chars() {
        let key = match c {
            '\n' => Some(Stroke {
                code: ENTER,
                shift: false,
            }),
            '\t' => Some(Stroke {
                code: TAB,
                shift: false,
            }),
            _ => tables[source]
                .get(&c)
                .or_else(|| tables[target].get(&c))
                .copied(),
        };
        match (key, unknown) {
            (Some(k), _) => keys.push(k),
            (None, Unknown::Keep) => {}
            (None, Unknown::Abort) => return None,
        }
    }
    Some((u32::try_from(target).ok()?, keys))
}

#[cfg(test)]
mod tests {
    use super::*;

    // us: q w e -> 16 17 18; ru: й ц у on the same keys. Both: '1' (2), ' ' (57).
    // '№' only in ru (Shift+3 = code 4). Shifted letters use shift: true.
    fn tables() -> Vec<Table> {
        let s = |code, shift| Stroke { code, shift };
        let us: Table = [
            ('q', s(16, false)),
            ('w', s(17, false)),
            ('e', s(18, false)),
            ('Q', s(16, true)),
            ('1', s(2, false)),
            (' ', s(57, false)),
            ('#', s(4, true)),
        ]
        .into();
        let ru: Table = [
            ('й', s(16, false)),
            ('ц', s(17, false)),
            ('у', s(18, false)),
            ('Й', s(16, true)),
            ('1', s(2, false)),
            (' ', s(57, false)),
            ('№', s(4, true)),
        ]
        .into();
        vec![us, ru]
    }

    fn codes(v: &[Stroke]) -> Vec<(u16, bool)> {
        v.iter().map(|s| (s.code, s.shift)).collect()
    }

    #[test]
    fn latin_goes_to_next_layout() {
        let (to, keys) = convert("Qwe", &tables(), 0, Unknown::Keep).unwrap();
        assert_eq!(to, 1);
        assert_eq!(codes(&keys), [(16, true), (17, false), (18, false)]);
    }

    #[test]
    fn cyrillic_is_detected_even_if_current_layout_is_ru() {
        let (to, keys) = convert("йцу", &tables(), 1, Unknown::Keep).unwrap();
        assert_eq!(to, 0);
        assert_eq!(codes(&keys), [(16, false), (17, false), (18, false)]);
    }

    #[test]
    fn shared_and_target_only_chars_are_kept() {
        // '1' and ' ' exist in both; '№' only in the target (ru).
        let (_, keys) = convert("q 1№", &tables(), 0, Unknown::Keep).unwrap();
        assert_eq!(
            codes(&keys),
            [(16, false), (57, false), (2, false), (4, true)]
        );
    }

    #[test]
    fn newline_and_tab_become_enter_and_tab() {
        let (_, keys) = convert("q\n\tw", &tables(), 0, Unknown::Keep).unwrap();
        assert_eq!(
            codes(&keys),
            [(16, false), (28, false), (15, false), (17, false)]
        );
    }

    #[test]
    fn unknown_chars_are_skipped_or_abort() {
        let (_, keys) = convert("q😀w", &tables(), 0, Unknown::Keep).unwrap();
        assert_eq!(codes(&keys), [(16, false), (17, false)]);
        assert_eq!(convert("q😀w", &tables(), 0, Unknown::Abort), None);
    }

    #[test]
    fn nothing_to_do() {
        assert_eq!(convert("", &tables(), 0, Unknown::Keep), None);
        assert_eq!(convert("qwe", &tables()[..1], 0, Unknown::Keep), None);
    }
}
