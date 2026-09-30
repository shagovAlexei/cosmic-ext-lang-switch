// SPDX-License-Identifier: GPL-3.0-only
//! Linux evdev key codes (linux/input-event-codes.h) we care about.

pub const BACKSPACE: u16 = 14;
pub const SPACE: u16 = 57;
pub const LEFTSHIFT: u16 = 42;
/// Unbound on COSMIC: tapped to keep a held Super from counting as a lone Super tap.
pub const F24: u16 = 194;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Produces a character in the current layout; goes into the buffer.
    Printable,
    Backspace,
    /// Anything that may move the caret or change focus: clears the buffer.
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    Shift,
    Ctrl,
    Alt,
    Super,
}

#[must_use]
pub fn kind(code: u16) -> Kind {
    match code {
        // digits row, q..], a..' `, \ z../, space, the ISO <> key
        2..=13 | 16..=27 | 30..=41 | 43..=53 | SPACE | 86 => Kind::Printable,
        BACKSPACE => Kind::Backspace,
        _ => Kind::Reset,
    }
}

#[must_use]
pub fn modifier(code: u16) -> Option<Modifier> {
    // ponytail: RAlt (100) counts as Alt even when it's AltGr (lv3:ralt_switch);
    // AltGr chars then clear the buffer. Split it out if that bites.
    match code {
        42 | 54 => Some(Modifier::Shift),
        29 | 97 => Some(Modifier::Ctrl),
        56 | 100 => Some(Modifier::Alt),
        125 | 126 => Some(Modifier::Super),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_digits_punctuation_and_space_are_printable() {
        for code in [2, 13, 16, 27, 30, 40, 41, 43, 44, 53, SPACE, 86] {
            assert_eq!(kind(code), Kind::Printable, "code {code}");
        }
    }

    #[test]
    fn navigation_enter_tab_and_mouse_reset() {
        // Esc, Enter, Tab, KP Enter, arrows, Home, End, Delete, BTN_LEFT, BTN_TOUCH
        for code in [1, 28, 15, 96, 103, 105, 106, 108, 102, 107, 111, 272, 330] {
            assert_eq!(kind(code), Kind::Reset, "code {code}");
        }
    }

    #[test]
    fn backspace_is_backspace() {
        assert_eq!(kind(BACKSPACE), Kind::Backspace);
    }

    #[test]
    fn both_sides_of_each_modifier() {
        assert_eq!(modifier(42), Some(Modifier::Shift));
        assert_eq!(modifier(54), Some(Modifier::Shift));
        assert_eq!(modifier(29), Some(Modifier::Ctrl));
        assert_eq!(modifier(97), Some(Modifier::Ctrl));
        assert_eq!(modifier(56), Some(Modifier::Alt));
        assert_eq!(modifier(100), Some(Modifier::Alt));
        assert_eq!(modifier(125), Some(Modifier::Super));
        assert_eq!(modifier(126), Some(Modifier::Super));
        assert_eq!(modifier(30), None);
    }
}
