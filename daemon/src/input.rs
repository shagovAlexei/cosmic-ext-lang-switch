// SPDX-License-Identifier: GPL-3.0-only
use evdev::{AttributeSet, Device, EventType, InputEvent, KeyCode, uinput::VirtualDevice};
use lsc::engine::Stroke;
use lsc::keys::{BACKSPACE, LEFTSHIFT};
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
        let Ok(mut stream) = dev.into_event_stream() else {
            continue;
        };
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
