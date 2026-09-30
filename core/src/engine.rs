// SPDX-License-Identifier: GPL-3.0-only
//! Keystroke buffer and correction planning. Fed raw evdev key events,
//! returns what the daemon should do. Never logs keystrokes.

use crate::hotkey::{Hotkeys, Mods, Scope};
use crate::keys::{self, Kind};

pub const MAX_STROKES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stroke {
    pub code: u16,
    pub shift: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Backspace(usize),
    SwitchLayout(u32),
    Type(Vec<Stroke>),
}

#[derive(Clone, Copy, Debug)]
struct Last {
    len: usize,
    from: u32,
}

#[derive(Debug)]
pub struct Engine {
    enabled: bool,
    hotkeys: Hotkeys,
    strokes: Vec<Stroke>,
    mods: Mods,
    pending: Option<Scope>,
    last: Option<Last>,
    group: u32,
    layouts: u32,
    expected: Option<u32>,
}

impl Engine {
    #[must_use]
    pub fn new(hotkeys: Hotkeys) -> Self {
        Self {
            enabled: true,
            hotkeys,
            strokes: Vec::new(),
            mods: Mods::default(),
            pending: None,
            last: None,
            group: 0,
            layouts: 0,
            expected: None,
        }
    }

    pub fn set_hotkeys(&mut self, hotkeys: Hotkeys) {
        self.hotkeys = hotkeys;
    }

    pub fn set_layouts(&mut self, count: u32) {
        self.layouts = count;
        self.reset();
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.strokes.clear();
        self.last = None;
        self.pending = None;
    }

    /// Active layout group reported by the compositor.
    pub fn on_group(&mut self, group: u32) {
        if self.expected == Some(group) {
            self.expected = None;
        } else if group != self.group {
            self.reset();
        }
        self.group = group;
    }

    pub fn on_key(&mut self, code: u16, value: i32) -> Option<Vec<Action>> {
        if let Some(m) = keys::modifier(code) {
            self.mods.set(m, value != 0);
            return self.fire_if_released(value);
        }
        if value == 0 {
            return self.fire_if_released(value);
        }
        if let Some(scope) = self.hotkeys.matches(code, self.mods) {
            if value == 1 && self.enabled {
                self.pending = Some(scope);
            }
            return None;
        }
        if !self.enabled {
            return None;
        }
        if self.mods.ctrl || self.mods.alt || self.mods.sup {
            self.reset();
            return None;
        }
        match keys::kind(code) {
            Kind::Printable => {
                self.last = None;
                self.strokes.push(Stroke {
                    code,
                    shift: self.mods.shift,
                });
                if self.strokes.len() > MAX_STROKES {
                    self.strokes.remove(0);
                }
            }
            Kind::Backspace => {
                self.last = None;
                self.strokes.pop();
            }
            Kind::Reset => self.reset(),
        }
        None
    }

    /// Corrections run only once every key is up, so a held modifier
    /// (Super of Super+Insert) never mixes into the replayed keys.
    fn fire_if_released(&mut self, value: i32) -> Option<Vec<Action>> {
        if value != 0 || self.mods.any() {
            return None;
        }
        let scope = self.pending.take()?;
        if let Some(last) = self.last.take() {
            let tail = self.strokes[self.strokes.len() - last.len..].to_vec();
            self.expected = Some(last.from);
            return Some(vec![
                Action::Backspace(last.len),
                Action::SwitchLayout(last.from),
                Action::Type(tail),
            ]);
        }
        if self.layouts < 2 {
            return None;
        }
        let len = match scope {
            Scope::Word => word_len(&self.strokes),
            Scope::Phrase => self.strokes.len(),
        };
        if len == 0 {
            return None;
        }
        let to = (self.group + 1) % self.layouts;
        self.last = Some(Last {
            len,
            from: self.group,
        });
        self.expected = Some(to);
        let tail = self.strokes[self.strokes.len() - len..].to_vec();
        Some(vec![
            Action::Backspace(len),
            Action::SwitchLayout(to),
            Action::Type(tail),
        ])
    }
}

/// Last word plus the spaces typed after it; 0 if there's no word.
fn word_len(s: &[Stroke]) -> usize {
    let space = |x: &&Stroke| x.code == keys::SPACE;
    let trailing = s.iter().rev().take_while(space).count();
    let word = s[..s.len() - trailing]
        .iter()
        .rev()
        .take_while(|x| !space(x))
        .count();
    if word == 0 { 0 } else { word + trailing }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSERT: u16 = 110;
    const SUPER: u16 = 125;
    const CTRL: u16 = 29;
    const ENTER: u16 = 28;
    // qwerty codes: g h b d t n
    const GHBDTN: [u16; 6] = [34, 35, 48, 32, 20, 49];

    fn engine() -> Engine {
        let mut e = Engine::new(Hotkeys::default());
        e.set_layouts(2);
        e
    }

    fn tap(e: &mut Engine, code: u16) -> Option<Vec<Action>> {
        assert!(e.on_key(code, 1).is_none());
        e.on_key(code, 0)
    }

    fn typed(e: &mut Engine, codes: &[u16]) {
        for &c in codes {
            tap(e, c);
        }
    }

    fn strokes(codes: &[u16]) -> Vec<Stroke> {
        codes
            .iter()
            .map(|&code| Stroke { code, shift: false })
            .collect()
    }

    #[test]
    fn insert_retypes_last_word_in_next_layout() {
        let mut e = engine();
        typed(&mut e, &GHBDTN);
        assert_eq!(
            tap(&mut e, INSERT),
            Some(vec![
                Action::Backspace(6),
                Action::SwitchLayout(1),
                Action::Type(strokes(&GHBDTN))
            ])
        );
    }

    #[test]
    fn word_includes_trailing_space_but_not_previous_word() {
        let mut e = engine();
        typed(&mut e, &[35, 18, 38, 38, 24, keys::SPACE]); // "hello "
        typed(&mut e, &GHBDTN);
        typed(&mut e, &[keys::SPACE]);
        let mut want = strokes(&GHBDTN);
        want.push(Stroke {
            code: keys::SPACE,
            shift: false,
        });
        assert_eq!(
            tap(&mut e, INSERT),
            Some(vec![
                Action::Backspace(7),
                Action::SwitchLayout(1),
                Action::Type(want)
            ])
        );
    }

    #[test]
    fn phrase_fires_after_super_released() {
        let mut e = engine();
        typed(&mut e, &[35, keys::SPACE]);
        typed(&mut e, &GHBDTN);
        assert!(e.on_key(SUPER, 1).is_none());
        assert!(e.on_key(INSERT, 1).is_none());
        assert!(e.on_key(INSERT, 0).is_none(), "Super still held");
        let got = e.on_key(SUPER, 0).expect("fires on Super release");
        assert_eq!(got[0], Action::Backspace(8));
    }

    #[test]
    fn second_press_undoes() {
        let mut e = engine();
        typed(&mut e, &GHBDTN);
        tap(&mut e, INSERT);
        e.on_group(1);
        assert_eq!(
            tap(&mut e, INSERT),
            Some(vec![
                Action::Backspace(6),
                Action::SwitchLayout(0),
                Action::Type(strokes(&GHBDTN))
            ])
        );
    }

    #[test]
    fn enter_clears_buffer_so_password_is_never_replayed() {
        let mut e = engine();
        typed(&mut e, &GHBDTN);
        typed(&mut e, &[ENTER]);
        assert_eq!(tap(&mut e, INSERT), None);
    }

    #[test]
    fn backspace_removes_last_stroke() {
        let mut e = engine();
        typed(&mut e, &GHBDTN);
        typed(&mut e, &[keys::BACKSPACE]);
        assert_eq!(tap(&mut e, INSERT).unwrap()[0], Action::Backspace(5));
    }

    #[test]
    fn ctrl_chord_resets() {
        let mut e = engine();
        typed(&mut e, &GHBDTN);
        e.on_key(CTRL, 1);
        tap(&mut e, 47); // Ctrl+V
        e.on_key(CTRL, 0);
        assert_eq!(tap(&mut e, INSERT), None);
    }

    #[test]
    fn mouse_click_resets() {
        let mut e = engine();
        typed(&mut e, &GHBDTN);
        tap(&mut e, 272);
        assert_eq!(tap(&mut e, INSERT), None);
    }

    #[test]
    fn touch_resets_buffer() {
        let mut e = engine();
        typed(&mut e, &GHBDTN);
        tap(&mut e, 330);
        assert_eq!(tap(&mut e, INSERT), None);
    }

    #[test]
    fn shift_state_is_recorded() {
        let mut e = engine();
        e.on_key(keys::LEFTSHIFT, 1);
        tap(&mut e, 34);
        e.on_key(keys::LEFTSHIFT, 0);
        let got = tap(&mut e, INSERT).unwrap();
        assert_eq!(
            got[2],
            Action::Type(vec![Stroke {
                code: 34,
                shift: true
            }])
        );
    }

    #[test]
    fn autorepeat_appends() {
        let mut e = engine();
        e.on_key(34, 1);
        e.on_key(34, 2);
        e.on_key(34, 0);
        assert_eq!(tap(&mut e, INSERT).unwrap()[0], Action::Backspace(2));
    }

    #[test]
    fn disabled_does_nothing() {
        let mut e = engine();
        e.set_enabled(false);
        typed(&mut e, &GHBDTN);
        assert_eq!(tap(&mut e, INSERT), None);
    }

    #[test]
    fn single_layout_does_nothing() {
        let mut e = engine();
        e.set_layouts(1);
        typed(&mut e, &GHBDTN);
        assert_eq!(tap(&mut e, INSERT), None);
    }

    #[test]
    fn three_layouts_cycle_to_next() {
        let mut e = engine();
        e.set_layouts(3);
        e.on_group(2);
        typed(&mut e, &GHBDTN);
        assert_eq!(tap(&mut e, INSERT).unwrap()[1], Action::SwitchLayout(0));
    }

    #[test]
    fn external_group_change_resets_but_own_does_not() {
        let mut e = engine();
        typed(&mut e, &GHBDTN);
        e.on_group(1); // user pressed Super+Space
        assert_eq!(tap(&mut e, INSERT), None);

        typed(&mut e, &GHBDTN);
        tap(&mut e, INSERT); // we switch 1 -> 0
        e.on_group(0);
        assert!(
            tap(&mut e, INSERT).is_some(),
            "own switch kept the buffer, undo works"
        );
    }

    #[test]
    fn buffer_is_capped() {
        let mut e = engine();
        typed(&mut e, &[34; MAX_STROKES + 10]);
        e.on_key(SUPER, 1);
        e.on_key(INSERT, 1);
        e.on_key(INSERT, 0);
        assert_eq!(
            e.on_key(SUPER, 0).unwrap()[0],
            Action::Backspace(MAX_STROKES)
        );
    }

    #[test]
    fn only_spaces_is_nothing_to_fix() {
        let mut e = engine();
        typed(&mut e, &[keys::SPACE, keys::SPACE]);
        assert_eq!(tap(&mut e, INSERT), None);
    }
}
