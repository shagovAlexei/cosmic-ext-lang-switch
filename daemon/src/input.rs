// SPDX-License-Identifier: GPL-3.0-only
use evdev::{
    AttributeSet, AttributeSetRef, Device, EventType, InputEvent, KeyCode, RelativeAxisCode,
    uinput::VirtualDevice,
};
use lsc::engine::Stroke;
use lsc::keys::{BACKSPACE, LEFTSHIFT};
use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

pub const VIRTUAL_NAME: &str = "cosmic-ext-lang-switch virtual keyboard";
pub type Seen = Arc<Mutex<HashSet<PathBuf>>>;

/// A key event and whether its device is grabbed (so it must be forwarded by us).
pub type KeyEvent = (u16, i32, bool);

fn is_keyboard(dev: &Device) -> bool {
    dev.supported_keys()
        .is_some_and(|k| k.contains(KeyCode::KEY_A) && k.contains(KeyCode::KEY_SPACE))
}

/// Only pure keyboards are grabbed. A keyboard that is also a pointer (ydotoold,
/// LogiOps) would lose its motion, buttons and wheel: we forward key events only.
fn grabbable(
    keys: Option<&AttributeSetRef<KeyCode>>,
    rel: Option<&AttributeSetRef<RelativeAxisCode>>,
) -> bool {
    keys.is_some_and(|k| {
        k.contains(KeyCode::KEY_A)
            && k.contains(KeyCode::KEY_SPACE)
            && !k.contains(KeyCode::BTN_LEFT)
    }) && !rel.is_some_and(|r| r.contains(RelativeAxisCode::REL_X))
}

/// Keyboards, plus mice and touchpads (their clicks and touches clear the buffer).
/// Our own uinput device is skipped so replayed keys never re-enter the buffer.
fn wanted(dev: &Device) -> bool {
    dev.name() != Some(VIRTUAL_NAME)
        && (is_keyboard(dev)
            || dev
                .supported_keys()
                .is_some_and(|k| k.contains(KeyCode::BTN_LEFT) || k.contains(KeyCode::BTN_TOUCH)))
}

/// Opens devices not already open and sends their key events to `tx`. With `grab`,
/// keyboards are grabbed so the hotkey never reaches apps; the caller forwards the
/// rest. A keyboard that can't be grabbed (e.g. held by keyd) is read passively.
/// Returns how many are open now; 0 means no read access to /dev/input.
// ponytail: rescanned on a 2 s timer instead of inotify; switch if hotplug lag matters.
pub fn spawn_new_devices(seen: &Seen, tx: &UnboundedSender<KeyEvent>, grab: bool) -> usize {
    for (path, mut dev) in evdev::enumerate() {
        if seen.lock().unwrap().contains(&path) || !wanted(&dev) {
            continue;
        }
        let grabbed = grab
            && grabbable(dev.supported_keys(), dev.supported_relative_axes())
            && dev.grab().is_ok();
        let Ok(mut stream) = dev.into_event_stream() else {
            continue;
        };
        seen.lock().unwrap().insert(path.clone());
        let (seen, tx) = (seen.clone(), tx.clone());
        tokio::spawn(async move {
            while let Ok(ev) = stream.next_event().await {
                if ev.event_type() == EventType::KEY
                    && tx.send((ev.code(), ev.value(), grabbed)).is_err()
                {
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
        // Every key a grabbed keyboard may send, minus the BTN_* ranges: with those,
        // libinput would treat this device as a pointer or joystick.
        for code in (1..=0xff).chain(0x160..=0x2bf) {
            keys.insert(KeyCode::new(code));
        }
        Ok(Self(
            VirtualDevice::builder()?
                .name(VIRTUAL_NAME)
                .with_keys(&keys)?
                .build()?,
        ))
    }

    pub fn key(&mut self, code: u16, value: i32) -> io::Result<()> {
        self.0
            .emit(&[InputEvent::new(EventType::KEY.0, code, value)])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_that_is_also_a_pointer_is_not_grabbed() {
        let kbd: AttributeSet<KeyCode> = [KeyCode::KEY_A, KeyCode::KEY_SPACE].into_iter().collect();
        let combo: AttributeSet<KeyCode> = [KeyCode::KEY_A, KeyCode::KEY_SPACE, KeyCode::BTN_LEFT]
            .into_iter()
            .collect();
        let xy: AttributeSet<RelativeAxisCode> = [RelativeAxisCode::REL_X, RelativeAxisCode::REL_Y]
            .into_iter()
            .collect();
        assert!(grabbable(Some(&kbd), None), "plain keyboard");
        assert!(
            !grabbable(Some(&combo), Some(&xy)),
            "ydotoold: keys + buttons + motion"
        );
        assert!(
            !grabbable(Some(&kbd), Some(&xy)),
            "keys + motion, no buttons"
        );
        assert!(!grabbable(Some(&combo), None), "keys + buttons, no motion");
        assert!(!grabbable(None, Some(&xy)), "mouse");
    }
}
