# cosmic-ext-lang-switch — фаза A: план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Load the `cosmic-applet` skill before Tasks 5–7.

**Goal:** хоткей исправляет последнее слово или фразу, набранные не в той раскладке, на COSMIC (Wayland); аплет в панели показывает раскладку, вкл/выкл и хоткеи.

**Architecture:** workspace из трёх crate. `core` — чистая логика (буфер, движок исправлений, хоткеи, конфиг, D-Bus-прокси), покрыта тестами. `daemon` (`systemd --user`) читает evdev, печатает через uinput, переключает раскладку по Wayland-протоколу `zcosmic_keyboard_layout_v1` и публикует состояние по D-Bus. `applet` — тонкий UI на libcosmic поверх D-Bus и cosmic-config.

**Tech Stack:** Rust edition 2024 (локально rustc 1.98.1), libcosmic `ef490df50b0a05a21c494c3f75737581bf0b39d9`, cosmic-config (та же rev), evdev 0.13 (feature `tokio`), wayland-client 0.31, cosmic-protocols 0.2, zbus 5, logind-zbus 5, tokio 1.

**Spec:** `docs/superpowers/specs/2026-09-30-lang-switch-phase-a-design.md`

## Global Constraints

- Имена: applet `cosmic-ext-lang-switch`, daemon `cosmic-ext-lang-switch-daemon`, APP_ID `io.github.shagovAlexei.cosmic-ext-lang-switch`, D-Bus `io.github.shagovAlexei.CosmicExtLangSwitch`, путь `/io/github/shagovAlexei/CosmicExtLangSwitch`.
- Хоткеи по умолчанию: слово `Insert`, фраза `Super+Insert`. Выделенный текст (`Alt+Insert`) в этот план не входит, см. Task 1.
- Демон никогда не пишет нажатия в лог и на диск. Буфер живёт только в памяти, не больше 256 нажатий.
- Лицензия GPL-3.0-only, автор `Shagov Alexei <shagov.alexei@gmail.com>`.
- Все строки UI через `fl!()`, обязательно en и ru.
- libcosmic и cosmic-config закреплены на одной rev, иначе типы конфига не совпадут.

## Отклонения от спецификации (осознанные, YAGNI)

- `Enabled` хранится в cosmic-config, а не в D-Bus. Аплет пишет конфиг, демон его отслеживает. Методов `Toggle` и `Corrected` нет.
- `ConvertSelection` пока нет, решение по нему принимается в Task 1.
- Hotplug сделан пересканированием `/dev/input` раз в 2 секунды вместо inotify (помечено `ponytail:`).
- Строки `logind` в `--check` нет: без сигналов Lock демон продолжает работать, а буфер от пароля всё равно очищается нажатием Enter. Демон пишет в лог warn, если сигналы недоступны.

## Review Focus

1. **Модификатор хоткея ещё зажат во время повтора.** При `Super+Insert` удерживаемый Super смешался бы с нашими BackSpace. Исправление срабатывает только после отпускания всех модификаторов (тест `phrase_fires_after_super_released`, Task 3).
2. **Пароль на экране блокировки.** Пароль, набранный перед Enter, не должен остаться в буфере и перепечататься после разблокировки (тест `enter_clears_buffer_so_password_is_never_replayed`, Task 3; сигналы Lock и Unlock в Task 6).
3. **Клик тачпадом.** Тап по тачпаду не даёт `BTN_LEFT`, только `BTN_TOUCH`, и без сброса Insert стёр бы чужой текст (тест `touch_resets_buffer`, Task 3; устройства с `BTN_TOUCH` читаются в Task 5).
4. **Смена раскладки извне** (Super+Space или аплет) между набором и хоткеем. Буфер сбрасывается, а собственное переключение демона буфер не сбрасывает (тест `external_group_change_resets_but_own_does_not`, Task 3).
5. **Собственные события uinput** не должны попадать обратно в буфер. Виртуальное устройство отфильтровывается по имени (проверка `wanted()` в Task 5, ручная проверка в Task 8).

---

### Task 1: Spike — можно ли читать выделенный текст без фокуса

Код одноразовый, в репозиторий не коммитится. Цель — ответить на вопрос, возможна ли фаза A2.

- [ ] **Step 1:** Проверить, какие data-control протоколы отдаёт cosmic-comp:
  ```bash
  wayland-info 2>/dev/null | grep -iE 'data_control|primary_selection' || sudo apt install -y wayland-utils
  ```
- [ ] **Step 2:** Если есть `ext_data_control_manager_v1` или `zwlr_data_control_manager_v1`: `sudo apt install -y wl-clipboard`, выделить текст в gedit, выполнить `wl-paste --primary`. Если текст напечатался, primary selection читается без Ctrl+C.
- [ ] **Step 3:** Если протоколов нет, повторить с `COSMIC_DATA_CONTROL_ENABLED=1`, заданным для cosmic-comp (`/etc/environment` и перелогин), и отметить, что для этого нужна правка окружения.
- [ ] **Step 4:** Дописать в конец спецификации раздел `## Результат spike «выделенное»` (что работает и нужна ли переменная), затем закоммитить:
  ```bash
  git add docs/superpowers/specs && git commit -m "docs: selection spike result"
  ```

### Task 2: Workspace + core: коды клавиш и хоткеи

**Files:**
- Create: `Cargo.toml`, `core/Cargo.toml`, `core/src/lib.rs`, `core/src/keys.rs`, `core/src/hotkey.rs`, `justfile`

**Interfaces:**
- Produces: `keys::{SPACE, BACKSPACE, LEFTSHIFT, Kind, Modifier, kind(u16) -> Kind, modifier(u16) -> Option<Modifier>}`; `hotkey::{Mods { shift, ctrl, alt, sup }, Mods::any(), Mods::set(Modifier, bool), Hotkey { mods, key: u16 }, impl FromStr + Display for Hotkey, Hotkeys { word, phrase }, Hotkeys::default(), Hotkeys::matches(u16, Mods) -> Option<Scope>}`. `Scope` определяется здесь, в `hotkey.rs`, и переэкспортируется.

- [ ] **Step 1: Workspace**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["core"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "GPL-3.0-only"
authors = ["Shagov Alexei <shagov.alexei@gmail.com>"]
repository = "https://github.com/shagovAlexei/cosmic-ext-lang-switch"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
zbus = { version = "5", default-features = false, features = ["tokio"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time"] }
log = "0.4"
simple_logger = "5"
cosmic-config = { git = "https://github.com/pop-os/libcosmic", rev = "ef490df50b0a05a21c494c3f75737581bf0b39d9", default-features = false, features = ["macro"] }
```

`core/Cargo.toml`:
```toml
[package]
name = "cosmic-ext-lang-switch-core"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[dependencies]
serde.workspace = true
zbus.workspace = true
cosmic-config.workspace = true

[dev-dependencies]
ron = "0.8"
```

`core/src/lib.rs`:
```rust
// SPDX-License-Identifier: GPL-3.0-only
//! Pure logic shared by the daemon and the applet. No device or Wayland I/O here.

pub mod hotkey;
pub mod keys;
```

`justfile` (packaging recipes are added in Task 8):
```make
name := 'cosmic-ext-lang-switch'
export APPID := 'io.github.shagovAlexei.cosmic-ext-lang-switch'

[private]
default:
    @just --list

# fmt + clippy -D warnings + tests — what CI runs
verify:
    cargo fmt --all --check
    cargo clippy --all-targets -- -D warnings
    cargo test --all

check *args:
    cargo fmt --all
    cargo clippy --all-targets {{args}} -- -W clippy::pedantic
```

- [ ] **Step 2: Failing tests**

`core/src/keys.rs`:
```rust
// SPDX-License-Identifier: GPL-3.0-only
//! Linux evdev key codes (linux/input-event-codes.h) we care about.

pub const BACKSPACE: u16 = 14;
pub const SPACE: u16 = 57;
pub const LEFTSHIFT: u16 = 42;

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
    todo!()
}

#[must_use]
pub fn modifier(code: u16) -> Option<Modifier> {
    todo!()
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
```

`core/src/hotkey.rs`:
```rust
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
    ("Insert", 110), ("Pause", 119), ("ScrollLock", 70), ("Menu", 127),
    ("F1", 59), ("F2", 60), ("F3", 61), ("F4", 62), ("F5", 63), ("F6", 64),
    ("F7", 65), ("F8", 66), ("F9", 67), ("F10", 68), ("F11", 87), ("F12", 88),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_key_case_insensitive() {
        let h: Hotkey = "insert".parse().unwrap();
        assert_eq!(h, Hotkey { mods: Mods::default(), key: 110 });
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
        let sup = Mods { sup: true, ..Mods::default() };
        assert_eq!(h.matches(110, sup), Some(Scope::Phrase));
        let shift = Mods { shift: true, ..Mods::default() };
        assert_eq!(h.matches(110, shift), None, "Shift+Insert is paste, not ours");
    }
}
```
Add `pub use hotkey::Scope;` to `lib.rs`.

- [ ] **Step 3:** `cargo test -p cosmic-ext-lang-switch-core`. Ожидается: не компилируется (`Mods::any`, `FromStr` и остальное не определены) или паника на `todo!()`.

- [ ] **Step 4: Implement**

В `keys.rs` заменить тела:
```rust
pub fn kind(code: u16) -> Kind {
    match code {
        // 1..=0 - =, q..], a..' `, \ z../, space, the ISO <> key
        2..=13 | 16..=27 | 30..=41 | 43..=53 | SPACE | 86 => Kind::Printable,
        BACKSPACE => Kind::Backspace,
        _ => Kind::Reset,
    }
}

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
```
В `hotkey.rs` дописать:
```rust
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
        let key = parts.pop().filter(|k| !k.is_empty()).ok_or("empty hotkey")?;
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
        let name = KEY_NAMES.iter().find(|&&(_, c)| c == self.key).map_or("?", |&(n, _)| n);
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
```

- [ ] **Step 5:** `just verify`. Ожидается: все тесты PASS, clippy без ошибок.
- [ ] **Step 6: Commit**
```bash
git add Cargo.toml Cargo.lock justfile core && git commit -m "core: key classification and hotkey parsing"
```

### Task 3: core — движок исправлений

**Files:**
- Create: `core/src/engine.rs`; Modify: `core/src/lib.rs` (`pub mod engine;`)

**Interfaces:**
- Consumes: `keys::*`, `hotkey::{Hotkeys, Mods, Scope}` из Task 2.
- Produces: `engine::{MAX_STROKES, Stroke { code: u16, shift: bool }, Action::{Backspace(usize), SwitchLayout(u32), Type(Vec<Stroke>)}, Engine}` с методами `Engine::new(Hotkeys)`, `set_hotkeys(Hotkeys)`, `set_layouts(u32)`, `set_enabled(bool)`, `reset()`, `on_group(u32)`, `on_key(code: u16, value: i32) -> Option<Vec<Action>>`. `value` берётся из evdev: 0 = отпускание, 1 = нажатие, 2 = автоповтор.

- [ ] **Step 1: Failing tests** (`core/src/engine.rs`):
```rust
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
        codes.iter().map(|&code| Stroke { code, shift: false }).collect()
    }

    #[test]
    fn insert_retypes_last_word_in_next_layout() {
        let mut e = engine();
        typed(&mut e, &GHBDTN);
        assert_eq!(
            tap(&mut e, INSERT),
            Some(vec![Action::Backspace(6), Action::SwitchLayout(1), Action::Type(strokes(&GHBDTN))])
        );
    }

    #[test]
    fn word_includes_trailing_space_but_not_previous_word() {
        let mut e = engine();
        typed(&mut e, &[35, 18, 38, 38, 24, keys::SPACE]); // "hello "
        typed(&mut e, &GHBDTN);
        typed(&mut e, &[keys::SPACE]);
        let mut want = strokes(&GHBDTN);
        want.push(Stroke { code: keys::SPACE, shift: false });
        assert_eq!(
            tap(&mut e, INSERT),
            Some(vec![Action::Backspace(7), Action::SwitchLayout(1), Action::Type(want)])
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
            Some(vec![Action::Backspace(6), Action::SwitchLayout(0), Action::Type(strokes(&GHBDTN))])
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
        assert_eq!(got[2], Action::Type(vec![Stroke { code: 34, shift: true }]));
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
        assert!(tap(&mut e, INSERT).is_some(), "own switch kept the buffer, undo works");
    }

    #[test]
    fn buffer_is_capped() {
        let mut e = engine();
        typed(&mut e, &[34; MAX_STROKES + 10]);
        e.on_key(SUPER, 1);
        e.on_key(INSERT, 1);
        e.on_key(INSERT, 0);
        assert_eq!(e.on_key(SUPER, 0).unwrap()[0], Action::Backspace(MAX_STROKES));
    }

    #[test]
    fn only_spaces_is_nothing_to_fix() {
        let mut e = engine();
        typed(&mut e, &[keys::SPACE, keys::SPACE]);
        assert_eq!(tap(&mut e, INSERT), None);
    }
}
```

- [ ] **Step 2:** `cargo test -p cosmic-ext-lang-switch-core engine`. Ожидается: ошибка компиляции, `Engine::new` не найден.

- [ ] **Step 3: Implement** (вставить над `#[cfg(test)]`):
```rust
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
                self.strokes.push(Stroke { code, shift: self.mods.shift });
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
            return Some(vec![Action::Backspace(last.len), Action::SwitchLayout(last.from), Action::Type(tail)]);
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
        self.last = Some(Last { len, from: self.group });
        self.expected = Some(to);
        let tail = self.strokes[self.strokes.len() - len..].to_vec();
        Some(vec![Action::Backspace(len), Action::SwitchLayout(to), Action::Type(tail)])
    }
}

/// Last word plus the spaces typed after it; 0 if there's no word.
fn word_len(s: &[Stroke]) -> usize {
    let space = |x: &&Stroke| x.code == keys::SPACE;
    let trailing = s.iter().rev().take_while(space).count();
    let word = s[..s.len() - trailing].iter().rev().take_while(|x| !space(x)).count();
    if word == 0 { 0 } else { word + trailing }
}
```

- [ ] **Step 4:** `just verify`. Ожидается: все тесты движка PASS. Если `phrase_fires_after_super_released` падает, пересчитать ожидание: `h` + пробел + 6 = 8 нажатий, тест верен.
- [ ] **Step 5:** `git add core && git commit -m "core: correction engine with undo and safety resets"`

### Task 4: core — конфиг, подписи раскладок, D-Bus-прокси

**Files:**
- Create: `core/src/config.rs`, `core/src/dbus.rs`; Modify: `core/src/lib.rs`

**Interfaces:**
- Produces: `config::{APP_ID, COMP_ID = "com.system76.CosmicComp", Config { enabled: bool, hotkey_word: String, hotkey_phrase: String }` (derive `CosmicConfigEntry`, `#[version = 1]`), `Config::hotkeys() -> Hotkeys`, `Xkb { layout: String, variant: String }`, `labels(&Xkb) -> Vec<String>`}; `dbus::{BUS_NAME, PATH, LangSwitchProxy}` с `set_layout(u32)` и свойствами `layouts: Vec<String>`, `current_layout: u32`, `status: String`. Значения status: `"ok" | "no-input-access" | "no-layout-protocol"`.

- [ ] **Step 1: Failing tests** (`core/src/config.rs`):
```rust
// SPDX-License-Identifier: GPL-3.0-only
use crate::hotkey::{DEFAULT_PHRASE, DEFAULT_WORD, Hotkey, Hotkeys};
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let xkb = Xkb { layout: "us,ru".into(), variant: "intl,".into() };
        assert_eq!(labels(&xkb), ["US", "RU"]);
    }

    #[test]
    fn empty_variant_string() {
        let xkb = Xkb { layout: "us".into(), variant: String::new() };
        assert_eq!(labels(&xkb), ["US"]);
    }

    #[test]
    fn bad_hotkey_in_config_falls_back_to_default() {
        let c = Config { hotkey_word: "Nope".into(), ..Config::default() };
        assert_eq!(c.hotkeys(), Hotkeys::default());
    }

    #[test]
    fn custom_hotkey_is_used() {
        let c = Config { hotkey_word: "F9".into(), ..Config::default() };
        assert_eq!(c.hotkeys().word, "F9".parse::<Hotkey>().unwrap());
    }
}
```

- [ ] **Step 2:** `cargo test -p cosmic-ext-lang-switch-core config`. Ожидается: ошибка компиляции (`labels`, `hotkeys` не найдены). Если derive `CosmicConfigEntry` не компилируется без `cosmic_config` в корне crate, проверить импорт в `~/Projects/cosmic/cosmic-ext-classic-menu-plus/applet/src/config.rs` и повторить его.

- [ ] **Step 3: Implement** (в `config.rs`):
```rust
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
            if (2..=3).contains(&v.len()) { v } else { layout }.to_uppercase()
        })
        .collect()
}
```
`core/src/dbus.rs`:
```rust
// SPDX-License-Identifier: GPL-3.0-only
pub const BUS_NAME: &str = "io.github.shagovAlexei.CosmicExtLangSwitch";
pub const PATH: &str = "/io/github/shagovAlexei/CosmicExtLangSwitch";

#[zbus::proxy(
    interface = "io.github.shagovAlexei.CosmicExtLangSwitch",
    default_service = "io.github.shagovAlexei.CosmicExtLangSwitch",
    default_path = "/io/github/shagovAlexei/CosmicExtLangSwitch"
)]
pub trait LangSwitch {
    fn set_layout(&self, index: u32) -> zbus::Result<()>;
    #[zbus(property)]
    fn layouts(&self) -> zbus::Result<Vec<String>>;
    #[zbus(property)]
    fn current_layout(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn status(&self) -> zbus::Result<String>;
}
```
`lib.rs`: добавить `pub mod config; pub mod dbus; pub mod engine;`.

- [ ] **Step 4:** `just verify`. Ожидается: PASS.
- [ ] **Step 5:** `git add core Cargo.lock && git commit -m "core: config, xkb labels, D-Bus proxy"`

### Task 5: daemon — ввод, uinput, раскладка, `--check`

**Files:**
- Create: `daemon/Cargo.toml`, `daemon/src/main.rs`, `daemon/src/input.rs`, `daemon/src/layout.rs`; Modify: `Cargo.toml` (members += `"daemon"`)

**Interfaces:**
- Consumes: `lsc::engine::Stroke`, `lsc::keys::{BACKSPACE, LEFTSHIFT}`, `lsc::config::*` (`lsc` — alias of the core crate; not `core`, which would shadow Rust's built-in `core`).
- Produces: `input::{VIRTUAL_NAME, spawn_new_devices(&Seen, &UnboundedSender<(u16, i32)>) -> usize, Keyboard::new() -> io::Result<Keyboard>, Keyboard::key(u16, i32) -> io::Result<()>}`, `type Seen = Arc<Mutex<HashSet<PathBuf>>>`; `layout::{Layout, Layout::connect(watch::Sender<u32>) -> Result<Layout, Box<dyn Error>>, Layout::set_group(&self, u32)}`.

- [ ] **Step 1:** `daemon/Cargo.toml`:
```toml
[package]
name = "cosmic-ext-lang-switch-daemon"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[dependencies]
lsc = { package = "cosmic-ext-lang-switch-core", path = "../core" }
cosmic-config.workspace = true
evdev = { version = "0.13", features = ["tokio"] }
wayland-client = "0.31"
cosmic-protocols = { version = "0.2", default-features = false, features = ["client"] }
zbus.workspace = true
logind-zbus = "5"
futures-util = "0.3"
tokio.workspace = true
log.workspace = true
simple_logger.workspace = true
```
Если в cosmic-protocols 0.2 с crates.io нет модуля `keyboard_layout`, заменить зависимость на `{ git = "https://github.com/pop-os/cosmic-protocols", rev = "32283d7", default-features = false, features = ["client"] }`: в этой ревизии модуль точно есть.

- [ ] **Step 2:** `daemon/src/input.rs`:
```rust
// SPDX-License-Identifier: GPL-3.0-only
use lsc::engine::Stroke;
use lsc::keys::{BACKSPACE, LEFTSHIFT};
use evdev::{AttributeSet, Device, EventType, InputEvent, KeyCode, uinput::VirtualDevice};
use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

pub const VIRTUAL_NAME: &str = "cosmic-ext-lang-switch virtual keyboard";
pub type Seen = Arc<Mutex<HashSet<PathBuf>>>;

/// Keyboards, plus mice and touchpads (their clicks and touches clear the buffer).
/// Our own uinput device is skipped so replayed keys never re-enter the buffer.
fn wanted(dev: &Device) -> bool {
    dev.name() != Some(VIRTUAL_NAME)
        && dev.supported_keys().is_some_and(|k| {
            (k.contains(KeyCode::KEY_A) && k.contains(KeyCode::KEY_SPACE))
                || k.contains(KeyCode::BTN_LEFT)
                || k.contains(KeyCode::BTN_TOUCH)
        })
}

/// Opens devices not already open and forwards their key events as (code, value).
/// Returns how many are open now; 0 means no read access to /dev/input.
// ponytail: rescanned on a 2 s timer instead of inotify; switch if hotplug lag matters.
pub fn spawn_new_devices(seen: &Seen, tx: &UnboundedSender<(u16, i32)>) -> usize {
    for (path, dev) in evdev::enumerate() {
        if seen.lock().unwrap().contains(&path) || !wanted(&dev) {
            continue;
        }
        let Ok(mut stream) = dev.into_event_stream() else { continue };
        seen.lock().unwrap().insert(path.clone());
        let (seen, tx) = (seen.clone(), tx.clone());
        tokio::spawn(async move {
            while let Ok(ev) = stream.next_event().await {
                if ev.event_type() == EventType::KEY && tx.send((ev.code(), ev.value())).is_err() {
                    break;
                }
            }
            seen.lock().unwrap().remove(&path);
        });
    }
    seen.lock().unwrap().len()
}

pub struct Keyboard(VirtualDevice);

impl Keyboard {
    pub fn new() -> io::Result<Self> {
        let mut keys = AttributeSet::<KeyCode>::new();
        for code in 1..=248 {
            keys.insert(KeyCode::new(code));
        }
        Ok(Self(VirtualDevice::builder()?.name(VIRTUAL_NAME).with_keys(&keys)?.build()?))
    }

    pub fn key(&mut self, code: u16, value: i32) -> io::Result<()> {
        self.0.emit(&[InputEvent::new(EventType::KEY.0, code, value)])
    }

    pub fn tap(&mut self, code: u16) -> io::Result<()> {
        self.key(code, 1)?;
        self.key(code, 0)
    }

    pub fn stroke(&mut self, s: Stroke) -> io::Result<()> {
        if s.shift {
            self.key(LEFTSHIFT, 1)?;
        }
        self.tap(s.code)?;
        if s.shift {
            self.key(LEFTSHIFT, 0)?;
        }
        Ok(())
    }

    pub fn backspace(&mut self) -> io::Result<()> {
        self.tap(BACKSPACE)
    }
}
```
Если имена в evdev 0.13 отличаются (`KeyCode` / `Key`, `VirtualDevice::builder` / `VirtualDeviceBuilder::new`), свериться с docs.rs/evdev/0.13.2 и поправить только имена.

- [ ] **Step 3:** `daemon/src/layout.rs`:
```rust
// SPDX-License-Identifier: GPL-3.0-only
//! Headless Wayland client for cosmic's keyboard-layout protocol.
use cosmic_protocols::keyboard_layout::v1::client::{
    zcosmic_keyboard_layout_manager_v1::ZcosmicKeyboardLayoutManagerV1,
    zcosmic_keyboard_layout_v1::{self, ZcosmicKeyboardLayoutV1},
};
use std::error::Error;
use tokio::sync::watch;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_keyboard::WlKeyboard, wl_registry::WlRegistry, wl_seat::WlSeat};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};

struct State {
    group: watch::Sender<u32>,
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as wayland_client::Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
delegate_noop!(State: ignore WlSeat);
delegate_noop!(State: ignore WlKeyboard);
delegate_noop!(State: ZcosmicKeyboardLayoutManagerV1);

impl Dispatch<ZcosmicKeyboardLayoutV1, ()> for State {
    fn event(state: &mut Self, _: &ZcosmicKeyboardLayoutV1, event: zcosmic_keyboard_layout_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let zcosmic_keyboard_layout_v1::Event::Group { group } = event {
            state.group.send_replace(group);
        }
    }
}

pub struct Layout {
    conn: Connection,
    obj: ZcosmicKeyboardLayoutV1,
}

impl Layout {
    /// Connects, reads the current group, then dispatches on its own thread.
    pub fn connect(group: watch::Sender<u32>) -> Result<Self, Box<dyn Error>> {
        let conn = Connection::connect_to_env()?;
        let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
        let qh = queue.handle();
        let seat: WlSeat = globals.bind(&qh, 1..=1, ())?;
        let manager: ZcosmicKeyboardLayoutManagerV1 = globals.bind(&qh, 1..=1, ())?;
        let keyboard = seat.get_keyboard(&qh, ());
        let obj = manager.get_keyboard_layout(&keyboard, &qh, ());
        let mut state = State { group };
        queue.roundtrip(&mut state)?;
        std::thread::spawn(move || while queue.blocking_dispatch(&mut state).is_ok() {});
        Ok(Self { conn, obj })
    }

    pub fn set_group(&self, group: u32) {
        self.obj.set_group(group);
        let _ = self.conn.flush();
    }
}
```

- [ ] **Step 4:** `daemon/src/main.rs`, пока только `--check`:
```rust
// SPDX-License-Identifier: GPL-3.0-only
mod input;
mod layout;

use lsc::config::{COMP_ID, Xkb, labels};
use cosmic_config::ConfigGet;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};

fn comp_layouts() -> Vec<String> {
    cosmic_config::Config::new(COMP_ID, 1)
        .and_then(|c| c.get::<Xkb>("xkb_config"))
        .map(|x| labels(&x))
        .unwrap_or_default()
}

/// Probes each backend through the same code the daemon uses. Exit code 1 if any is missing.
async fn check() -> i32 {
    let mut bad = 0;
    let mut line = |name: &str, ok: bool, detail: String| {
        println!("{name:<10} {:<8} {detail}", if ok { "ok" } else { "MISSING" });
        bad += i32::from(!ok);
    };
    let (tx, _rx) = mpsc::unbounded_channel();
    let n = input::spawn_new_devices(&Arc::new(Mutex::new(HashSet::new())), &tx);
    line("input", n > 0, format!("{n} device(s) readable (need group `input`)"));
    let kb = input::Keyboard::new();
    line("uinput", kb.is_ok(), kb.err().map_or_else(String::new, |e| e.to_string()));
    let (gtx, grx) = watch::channel(0);
    let l = layout::Layout::connect(gtx);
    line("layout", l.is_ok(), l.err().map_or_else(|| format!("current group {}", *grx.borrow()), |e| e.to_string()));
    let names = comp_layouts();
    line("layouts", names.len() >= 2, names.join(","));
    bad.min(1)
}

#[tokio::main]
async fn main() {
    if std::env::args().any(|a| a == "--check") {
        std::process::exit(check().await);
    }
}
```

- [ ] **Step 5:** `cargo build -p cosmic-ext-lang-switch-daemon && ./target/debug/cosmic-ext-lang-switch-daemon --check`. Ожидается: `input ok`, `layout ok`, `layouts ok US,RU`; `uinput` будет `MISSING` до udev-правила из Task 8, либо проверить через `sudo chgrp input /dev/uinput && sudo chmod 660 /dev/uinput`.
- [ ] **Step 6:** `just verify && git add -A daemon Cargo.toml Cargo.lock && git commit -m "daemon: evdev input, uinput keyboard, layout protocol, --check"`

### Task 6: daemon — главный цикл, D-Bus, конфиг, блокировка

**Files:**
- Create: `daemon/src/service.rs`; Modify: `daemon/src/main.rs`

**Interfaces:**
- Consumes: всё из Task 4 и Task 5, `Engine` из Task 3.
- Produces: D-Bus-объект по контракту `lsc::dbus::LangSwitchProxy`. Его использует аплет в Task 7.

- [ ] **Step 1:** `daemon/src/service.rs`:
```rust
// SPDX-License-Identifier: GPL-3.0-only
use crate::layout::Layout;
use std::sync::Arc;

pub struct Service {
    pub layouts: Vec<String>,
    pub current: u32,
    pub status: String,
    pub layout: Option<Arc<Layout>>,
}

#[zbus::interface(name = "io.github.shagovAlexei.CosmicExtLangSwitch")]
impl Service {
    fn set_layout(&self, index: u32) {
        if let Some(l) = &self.layout {
            l.set_group(index);
        }
    }
    #[zbus(property)]
    fn layouts(&self) -> Vec<String> {
        self.layouts.clone()
    }
    #[zbus(property)]
    fn current_layout(&self) -> u32 {
        self.current
    }
    #[zbus(property)]
    fn status(&self) -> String {
        self.status.clone()
    }
}

/// Forwards logind Lock (true) / Unlock (false) of the user's graphical session.
pub async fn lock_signals(tx: tokio::sync::mpsc::UnboundedSender<bool>) -> zbus::Result<()> {
    use futures_util::StreamExt;
    let conn = zbus::Connection::system().await?;
    let manager = logind_zbus::manager::ManagerProxy::new(&conn).await?;
    // "auto" resolves to the user's display session even from a systemd --user unit.
    let path = manager.get_session("auto").await?;
    let session = logind_zbus::session::SessionProxy::builder(&conn).path(path)?.build().await?;
    let mut lock = session.receive_lock().await?;
    let mut unlock = session.receive_unlock().await?;
    loop {
        tokio::select! {
            Some(_) = lock.next() => { let _ = tx.send(true); }
            Some(_) = unlock.next() => { let _ = tx.send(false); }
            else => return Ok(()),
        }
    }
}
```

- [ ] **Step 2:** Заменить `main` в `daemon/src/main.rs` (оставить `comp_layouts` и `check`, добавить `mod service;`):
```rust
use lsc::config::{APP_ID, Config};
use lsc::dbus::{BUS_NAME, PATH};
use lsc::engine::{Action, Engine};
use cosmic_config::CosmicConfigEntry;
use service::Service;
use std::time::Duration;

// ponytail: fixed per-key delay; raise if some app drops replayed keys.
const TAP: Duration = Duration::from_millis(3);

async fn exec(kb: &mut input::Keyboard, layout: &layout::Layout, group: &mut watch::Receiver<u32>, actions: &[Action]) -> std::io::Result<()> {
    for a in actions {
        match a {
            Action::Backspace(n) => {
                for _ in 0..*n {
                    kb.backspace()?;
                    tokio::time::sleep(TAP).await;
                }
            }
            Action::SwitchLayout(g) => {
                layout.set_group(*g);
                // Replay only after the compositor applied the group.
                let _ = tokio::time::timeout(Duration::from_millis(300), group.wait_for(|x| x == g)).await;
            }
            Action::Type(strokes) => {
                for s in strokes {
                    kb.stroke(*s)?;
                    tokio::time::sleep(TAP).await;
                }
            }
        }
    }
    Ok(())
}

fn load_config() -> Config {
    cosmic_config::Config::new(APP_ID, Config::VERSION)
        .map(|h| Config::get_entry(&h).unwrap_or_else(|(_, c)| c))
        .unwrap_or_default()
}

async fn publish(conn: &zbus::Connection, f: impl FnOnce(&mut Service)) -> zbus::Result<()> {
    let iface = conn.object_server().interface::<_, Service>(PATH).await?;
    let mut s = iface.get_mut().await;
    f(&mut s);
    s.layouts_changed(iface.signal_emitter()).await?;
    s.current_layout_changed(iface.signal_emitter()).await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|a| a == "--check") {
        std::process::exit(check().await);
    }
    simple_logger::init_with_env()?; // log state changes only, never key events

    let (key_tx, mut key_rx) = mpsc::unbounded_channel();
    let seen: input::Seen = Arc::default();
    let devices = input::spawn_new_devices(&seen, &key_tx);
    let mut keyboard = input::Keyboard::new().inspect_err(|e| log::error!("uinput: {e}")).ok();
    let (group_tx, mut group_rx) = watch::channel(0);
    let layout = layout::Layout::connect(group_tx).inspect_err(|e| log::error!("layout protocol: {e}")).ok().map(Arc::new);
    let status = match (devices > 0 && keyboard.is_some(), layout.is_some()) {
        (false, _) => "no-input-access",
        (true, false) => "no-layout-protocol",
        (true, true) => "ok",
    };

    let mut config = load_config();
    let mut names = comp_layouts();
    let mut engine = Engine::new(config.hotkeys());
    engine.set_layouts(names.len() as u32);
    engine.set_enabled(config.enabled && status == "ok");
    engine.on_group(*group_rx.borrow_and_update());

    let (cfg_tx, mut cfg_rx) = mpsc::unbounded_channel();
    let watch_cfg = |id: &str, version| {
        let tx = cfg_tx.clone();
        cosmic_config::Config::new(id, version)?.watch(move |_, _| { let _ = tx.send(()); })
    };
    let _app_watch = watch_cfg(APP_ID, Config::VERSION)?;
    let _comp_watch = watch_cfg(COMP_ID, 1)?;

    let service = Service { layouts: names.clone(), current: *group_rx.borrow(), status: status.into(), layout: layout.clone() };
    let conn = zbus::connection::Builder::session()?.name(BUS_NAME)?.serve_at(PATH, service)?.build().await?;

    let (lock_tx, mut lock_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        if let Err(e) = service::lock_signals(lock_tx).await {
            log::warn!("logind lock signals unavailable: {e}");
        }
    });

    let mut locked = false;
    let mut rescan = tokio::time::interval(Duration::from_secs(2));
    loop {
        tokio::select! {
            Some((code, value)) = key_rx.recv() => {
                if locked { continue; }
                let Some(actions) = engine.on_key(code, value) else { continue };
                if let (Some(kb), Some(l)) = (keyboard.as_mut(), layout.as_deref()) {
                    if let Err(e) = exec(kb, l, &mut group_rx, &actions).await {
                        log::error!("replay failed: {e}");
                    }
                }
                let g = *group_rx.borrow_and_update();
                engine.on_group(g);
                publish(&conn, |s| s.current = g).await?;
            }
            Ok(()) = group_rx.changed() => {
                let g = *group_rx.borrow_and_update();
                engine.on_group(g);
                publish(&conn, |s| s.current = g).await?;
            }
            Some(()) = cfg_rx.recv() => {
                config = load_config();
                names = comp_layouts();
                engine.set_hotkeys(config.hotkeys());
                engine.set_layouts(names.len() as u32);
                engine.set_enabled(config.enabled && status == "ok");
                let n = names.clone();
                publish(&conn, |s| s.layouts = n).await?;
            }
            Some(l) = lock_rx.recv() => {
                locked = l;
                engine.reset();
                log::info!("session {}", if l { "locked" } else { "unlocked" });
            }
            _ = rescan.tick() => { input::spawn_new_devices(&seen, &key_tx); }
        }
    }
}
```

- [ ] **Step 3:** `just verify`. Ожидается: сборка и тесты проходят. Clippy `cast_possible_truncation` на `names.len() as u32` исправить через `u32::try_from(names.len()).unwrap_or(u32::MAX)`.
- [ ] **Step 4: Ручная проверка** (uinput доступен, см. Task 5 Step 5): `RUST_LOG=info cargo run -p cosmic-ext-lang-switch-daemon`. В gedit набрать `ghbdtn`, нажать Insert: должно получиться `привет`, раскладка RU. Нажать Insert ещё раз: откат к `ghbdtn`, раскладка EN. Затем `busctl --user introspect io.github.shagovAlexei.CosmicExtLangSwitch /io/github/shagovAlexei/CosmicExtLangSwitch` должен показать свойства. `Super+Esc` (блокировка), разблокировать, нажать Insert: ничего не должно произойти.
- [ ] **Step 5:** `git add daemon Cargo.lock && git commit -m "daemon: main loop, D-Bus service, config watch, lock pause"`

### Task 7: applet

**Files:**
- Create: `applet/Cargo.toml`, `applet/src/main.rs`, `applet/src/app.rs`, `applet/src/i18n.rs`, `applet/i18n.toml`, `applet/i18n/en/cosmic_ext_lang_switch.ftl`, `applet/i18n/ru/cosmic_ext_lang_switch.ftl`; Modify: `Cargo.toml` (members += `"applet"`)

**Interfaces:**
- Consumes: `lsc::config::{APP_ID, Config}`, `lsc::dbus::LangSwitchProxy`.
- Produces: бинарь `cosmic-ext-lang-switch`, запускаемый панелью.

- [ ] **Step 1:** `applet/Cargo.toml`:
```toml
[package]
name = "cosmic-ext-lang-switch"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[dependencies]
lsc = { package = "cosmic-ext-lang-switch-core", path = "../core" }
zbus.workspace = true
tokio.workspace = true
log.workspace = true
simple_logger.workspace = true
i18n-embed = { version = "0.16", features = ["fluent-system", "desktop-requester"] }
i18n-embed-fl = "0.10"
rust-embed = "8"

[dependencies.libcosmic]
git = "https://github.com/pop-os/libcosmic"
rev = "ef490df50b0a05a21c494c3f75737581bf0b39d9"
default-features = false
features = ["applet", "tokio", "wayland"]
```
- [ ] **Step 2:** Скопировать `i18n.rs` и `i18n.toml` из `~/Projects/cosmic/cosmic-ext-classic-menu-plus/applet/` без изменений: макрос `fl!` берёт имя из crate. Строки:

`applet/i18n/en/cosmic_ext_lang_switch.ftl`:
```
enabled = Fix wrong layout
hotkey-word = Fix last word
hotkey-phrase = Fix phrase
hotkey-invalid = Unknown key. Examples: Insert, Super+Insert, Ctrl+F12
daemon-missing = Service is not running: systemctl --user enable --now cosmic-ext-lang-switch
no-input-access = No keyboard access: sudo usermod -aG input $USER, then log in again
no-layout-protocol = This COSMIC version does not expose the keyboard layout protocol
```
`applet/i18n/ru/cosmic_ext_lang_switch.ftl`:
```
enabled = Исправлять раскладку
hotkey-word = Исправить последнее слово
hotkey-phrase = Исправить фразу
hotkey-invalid = Неизвестная клавиша. Примеры: Insert, Super+Insert, Ctrl+F12
daemon-missing = Служба не запущена: systemctl --user enable --now cosmic-ext-lang-switch
no-input-access = Нет доступа к клавиатуре: sudo usermod -aG input $USER, затем перелогиньтесь
no-layout-protocol = Эта версия COSMIC не поддерживает протокол раскладок
```
- [ ] **Step 3:** `applet/src/main.rs`:
```rust
// SPDX-License-Identifier: GPL-3.0-only
mod app;
mod i18n;

fn main() -> cosmic::iced::Result {
    simple_logger::init_with_env().ok();
    i18n::init(&i18n_embed::DesktopLanguageRequester::requested_languages());
    cosmic::applet::run::<app::Applet>(())
}
```
- [ ] **Step 4:** `applet/src/app.rs`. API попапа повторяет `~/Projects/cosmic/cosmic-ext-classic-menu-plus/applet/src/applet.rs` (`toggle_popup`, примерно строки 570–600); при расхождениях с этой rev ориентироваться на него:
```rust
// SPDX-License-Identifier: GPL-3.0-only
use crate::fl;
use lsc::config::{APP_ID, Config};
use lsc::dbus::LangSwitchProxy;
use lsc::hotkey::Hotkey;
use cosmic::app::{Core, Task};
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::platform_specific::shell::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{Subscription, stream, window::Id};
use cosmic::widget::{self, settings};
use cosmic::iced::futures::{SinkExt, StreamExt, channel::mpsc::Sender};
use cosmic::{Application, Element};

#[derive(Clone, Debug, Default)]
pub struct Daemon {
    pub layouts: Vec<String>,
    pub current: u32,
    /// Empty when the daemon isn't on the bus.
    pub status: String,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Config(Config),
    Daemon(Daemon),
    SetLayout(u32),
    SetEnabled(bool),
    WordInput(String),
    PhraseInput(String),
}

pub struct Applet {
    core: Core,
    popup: Option<Id>,
    config: Config,
    daemon: Daemon,
    word: String,
    phrase: String,
}

impl Applet {
    fn save(&self) {
        if let Ok(h) = cosmic_config::Config::new(APP_ID, Config::VERSION) {
            if let Err(e) = self.config.write_entry(&h) {
                log::error!("config write: {e:?}");
            }
        }
    }

    fn label(&self) -> String {
        self.daemon.layouts.get(self.daemon.current as usize).cloned().unwrap_or_else(|| "??".into())
    }

    /// Validates on each keystroke; only a parseable hotkey is written.
    fn hotkey_row<'a>(&'a self, title: String, value: &'a str, on_input: fn(String) -> Message) -> Element<'a, Message> {
        let mut input = widget::text_input("Insert", value).on_input(on_input);
        if value.parse::<Hotkey>().is_err() {
            input = input.helper_text(fl!("hotkey-invalid")).error(fl!("hotkey-invalid"));
        }
        settings::item(title, input).into()
    }
}

/// Sends daemon state on every property change; returns only on a D-Bus error.
async fn watch_daemon(out: &mut Sender<Message>) -> zbus::Result<()> {
    let conn = zbus::Connection::session().await?;
    let p = LangSwitchProxy::new(&conn).await?;
    let mut cur = p.receive_current_layout_changed().await;
    let mut lay = p.receive_layouts_changed().await;
    loop {
        let d = Daemon { layouts: p.layouts().await?, current: p.current_layout().await?, status: p.status().await? };
        let _ = out.send(Message::Daemon(d)).await;
        tokio::select! { _ = cur.next() => {}, _ = lay.next() => {} }
    }
}

/// Streams daemon state; reconnects every 2 s while the daemon is absent.
fn daemon_subscription() -> Subscription<Message> {
    Subscription::run(|| {
        stream::channel(16, |mut out: Sender<Message>| async move {
            loop {
                let _ = watch_daemon(&mut out).await;
                let _ = out.send(Message::Daemon(Daemon::default())).await;
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        })
    })
}

impl Application for Applet {
    type Executor = cosmic::executor::multi::Executor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core { &self.core }
    fn core_mut(&mut self) -> &mut Core { &mut self.core }

    fn init(core: Core, _: ()) -> (Self, Task<Message>) {
        let config = cosmic_config::Config::new(APP_ID, Config::VERSION)
            .map(|h| Config::get_entry(&h).unwrap_or_else(|(_, c)| c))
            .unwrap_or_default();
        let (word, phrase) = (config.hotkey_word.clone(), config.hotkey_phrase.clone());
        (Self { core, popup: None, config, daemon: Daemon::default(), word, phrase }, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            self.core.watch_config::<Config>(APP_ID).map(|u| Message::Config(u.config)),
            daemon_subscription(),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                }
                let id = Id::unique();
                self.popup = Some(id);
                let settings = self.core.applet.get_popup_settings(self.core.main_window_id().unwrap(), id, None, None, None);
                return get_popup(settings);
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::Config(c) => self.config = c,
            Message::Daemon(d) => self.daemon = d,
            Message::SetLayout(i) => {
                return Task::perform(
                    async move {
                        let conn = zbus::Connection::session().await?;
                        LangSwitchProxy::new(&conn).await?.set_layout(i).await
                    },
                    |r: zbus::Result<()>| {
                        if let Err(e) = r { log::error!("set_layout: {e}"); }
                        cosmic::action::none()
                    },
                );
            }
            Message::SetEnabled(on) => {
                self.config.enabled = on;
                self.save();
            }
            Message::WordInput(s) => {
                if s.parse::<Hotkey>().is_ok() {
                    self.config.hotkey_word = s.clone();
                    self.save();
                }
                self.word = s;
            }
            Message::PhraseInput(s) => {
                if s.parse::<Hotkey>().is_ok() {
                    self.config.hotkey_phrase = s.clone();
                    self.save();
                }
                self.phrase = s;
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let label = if self.config.enabled { self.label() } else { format!("{}·", self.label()) };
        self.core.applet.autosize_window(self.core.applet.text_button(label, Message::TogglePopup)).into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        let warning = match self.daemon.status.as_str() {
            "ok" => None,
            "" => Some(fl!("daemon-missing")),
            "no-input-access" => Some(fl!("no-input-access")),
            _ => Some(fl!("no-layout-protocol")),
        };
        let mut layouts = widget::column();
        for (i, name) in self.daemon.layouts.iter().enumerate() {
            let i = i as u32;
            layouts = layouts.push(
                widget::radio(name.as_str(), i, Some(self.daemon.current), Message::SetLayout),
            );
        }
        let mut col = widget::column().spacing(8).padding(12);
        if let Some(w) = warning {
            col = col.push(widget::text::body(w));
        }
        col = col
            .push(settings::item(fl!("enabled"), widget::toggler(self.config.enabled).on_toggle(Message::SetEnabled)))
            .push(layouts)
            .push(self.hotkey_row(fl!("hotkey-word"), &self.word, Message::WordInput))
            .push(self.hotkey_row(fl!("hotkey-phrase"), &self.phrase, Message::PhraseInput));
        self.core.applet.popup_container(col).into()
    }
}
```
Если `cosmic::action::none()` в этой rev нет, добавить `Message::Noop` и возвращать `cosmic::action::app(Message::Noop)`, как в `classic-menu-plus` (`cosmic::action::app(...)`). Если `text_input` в этой rev не поддерживает `helper_text` или `error`, убрать эти вызовы и выводить `fl!("hotkey-invalid")` отдельным `widget::text::caption` под полем.

- [ ] **Step 5:** `just verify`, затем `cargo build --release -p cosmic-ext-lang-switch`. Ожидается: сборка проходит.
- [ ] **Step 6:** `git add applet Cargo.toml Cargo.lock && git commit -m "applet: layout indicator, toggle, hotkey settings"`

### Task 8: packaging, установка, TESTING.md

**Files:**
- Create: `res/io.github.shagovAlexei.cosmic-ext-lang-switch.desktop`, `res/cosmic-ext-lang-switch.service`, `res/60-cosmic-ext-lang-switch.rules`, `TESTING.md`, `README.md`; Modify: `justfile`, `CLAUDE.md` (секция Commands)

- [ ] **Step 1:** Файлы:

`res/io.github.shagovAlexei.cosmic-ext-lang-switch.desktop`:
```ini
[Desktop Entry]
Name=Lang Switch
Name[ru]=Переключатель раскладки
Comment=Fix text typed in the wrong keyboard layout
Comment[ru]=Исправляет текст, набранный не в той раскладке
Exec=cosmic-ext-lang-switch
Terminal=false
Type=Application
Icon=input-keyboard-symbolic
Categories=COSMIC;
NoDisplay=true
X-CosmicApplet=true
X-CosmicHoverPopup=Auto
```
`res/cosmic-ext-lang-switch.service`:
```ini
[Unit]
Description=cosmic-ext-lang-switch keyboard layout corrector
PartOf=graphical-session.target
After=graphical-session.target

[Service]
ExecStart=/usr/bin/cosmic-ext-lang-switch-daemon
Restart=on-failure
RestartSec=2

[Install]
WantedBy=graphical-session.target
```
`res/60-cosmic-ext-lang-switch.rules`:
```
KERNEL=="uinput", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput"
```
- [ ] **Step 2:** Дописать в `justfile`:
```make
prefix := '/usr'

build-release:
    cargo build --release -p {{name}} -p {{name}}-daemon

# sudo just install; then: systemctl --user daemon-reload && systemctl --user enable --now {{name}}
install:
    install -Dm0755 target/release/{{name}} {{prefix}}/bin/{{name}}
    install -Dm0755 target/release/{{name}}-daemon {{prefix}}/bin/{{name}}-daemon
    install -Dm0644 res/{{APPID}}.desktop {{prefix}}/share/applications/{{APPID}}.desktop
    install -Dm0644 res/{{name}}.service {{prefix}}/lib/systemd/user/{{name}}.service
    install -Dm0644 res/60-{{name}}.rules {{prefix}}/lib/udev/rules.d/60-{{name}}.rules
    udevadm control --reload && udevadm trigger --sysname-match=uinput

uninstall:
    rm -f {{prefix}}/bin/{{name}} {{prefix}}/bin/{{name}}-daemon {{prefix}}/share/applications/{{APPID}}.desktop {{prefix}}/lib/systemd/user/{{name}}.service {{prefix}}/lib/udev/rules.d/60-{{name}}.rules

run-daemon:
    env RUST_LOG=info cargo run -p {{name}}-daemon
```
(Пакет .deb, flatpak и rpm в этот план не входят. Их стоит добавить, когда понадобится раздача пользователям. Flatpak для демона невозможен по спецификации.)
- [ ] **Step 3:** Установка и проверка:
```bash
just build-release && sudo just install
systemctl --user daemon-reload && systemctl --user enable --now cosmic-ext-lang-switch
cosmic-ext-lang-switch-daemon --check        # все строки ok, иначе исправить до продолжения
killall cosmic-panel                          # затем добавить «Lang Switch» в панель через Настройки → Рабочий стол → Панель
```
- [ ] **Step 4:** `TESTING.md`: три уровня, как в cosmic-control-center-applet (`--check`, `just verify`, ручные сценарии). Ручная матрица:

| Сценарий | Ожидание |
|---|---|
| gedit: `ghbdtn` + Insert | `привет`, в панели RU |
| тот же Insert повторно | откат к `ghbdtn`, EN |
| `hello ghbdtn` + Super+Insert (отпустить Super) | вся фраза перепечатана в RU |
| терминал COSMIC, Firefox, LibreOffice: слово + Insert | то же. В LibreOffice отметить побочный эффект: Insert включает режим замены |
| набрать слово, кликнуть тачпадом в другое место, Insert | ничего не удаляется |
| набрать слово, Super+Space, Insert | ничего не происходит |
| заблокировать экран, ввести пароль, разблокировать, Insert | ничего не происходит |
| выключить в аплете, набрать, Insert | ничего не происходит |
| сменить хоткей слова на F9 в аплете | F9 работает сразу, Insert больше не исправляет |
| `systemctl --user stop cosmic-ext-lang-switch` | аплет показывает «Служба не запущена» |

Ниже матрицы добавить пустую таблицу `Regression tests`: тест и что он предотвращает. Заполнять по мере исправления багов.
- [ ] **Step 5:** В `CLAUDE.md` добавить раздел `## Commands` (`just verify`, `just build-release && sudo just install`, `just run-daemon`, `cosmic-ext-lang-switch-daemon --check`). `README.md`: что это, установка (группа input, `just install`, `systemctl --user enable --now`), хоткеи по умолчанию.
- [ ] **Step 6:** Пройти ручную матрицу и отметить результаты. Найденные баги исправлять с регрессионным тестом в `core`.
- [ ] **Step 7:** `git add -A && git commit -m "packaging: systemd user unit, udev rule, install recipes, TESTING.md"`

## Verification (вся фаза A)

- `just verify` зелёный.
- `cosmic-ext-lang-switch-daemon --check`: все строки `ok`.
- Ручная матрица из TESTING.md пройдена.
