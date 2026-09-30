// SPDX-License-Identifier: GPL-3.0-only
//! The daemon's one Wayland connection: keyboard layout, focused app, primary selection.
//!
//! Under the panel this is the privileged connection the applet hands over (our stdin,
//! `COSMIC_EXT_LANG_SWITCH_WAYLAND_FD=0`): cosmic-comp shows these protocols to a sandboxed
//! client only there. Run by hand it is `WAYLAND_DISPLAY`, where focus events don't come.
use cosmic_protocols::keyboard_layout::v1::client::{
    zcosmic_keyboard_layout_manager_v1::ZcosmicKeyboardLayoutManagerV1,
    zcosmic_keyboard_layout_v1::{self, ZcosmicKeyboardLayoutV1},
};
use std::collections::HashMap;
use std::error::Error;
use std::io::Read;
use std::os::fd::AsFd;
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_keyboard::WlKeyboard, wl_registry::WlRegistry, wl_seat::WlSeat};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, backend::ObjectId, delegate_noop, event_created_child,
};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::{self, ExtDataControlDeviceV1},
    ext_data_control_manager_v1::ExtDataControlManagerV1,
    ext_data_control_offer_v1::{self, ExtDataControlOfferV1},
};

/// The primary selection's offer and its mime types.
type Primary = Arc<Mutex<Option<(ExtDataControlOfferV1, Vec<String>)>>>;

struct State {
    group: watch::Sender<u32>,
    #[allow(dead_code)] // used from Task 4
    focus: mpsc::UnboundedSender<String>,
    /// Offers whose mime types are still arriving.
    offers: HashMap<ObjectId, Vec<String>>,
    /// The current primary selection (highlighted text).
    primary: Primary,
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as wayland_client::Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
delegate_noop!(State: ignore WlSeat);
delegate_noop!(State: ignore WlKeyboard);
delegate_noop!(State: ZcosmicKeyboardLayoutManagerV1);

impl Dispatch<ZcosmicKeyboardLayoutV1, ()> for State {
    fn event(
        s: &mut Self,
        _: &ZcosmicKeyboardLayoutV1,
        e: zcosmic_keyboard_layout_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zcosmic_keyboard_layout_v1::Event::Group { group } = e {
            s.group.send_replace(group);
        }
    }
}

delegate_noop!(State: ExtDataControlManagerV1);

impl Dispatch<ExtDataControlDeviceV1, ()> for State {
    fn event(
        s: &mut Self,
        _: &ExtDataControlDeviceV1,
        e: ext_data_control_device_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            ext_data_control_device_v1::Event::DataOffer { id } => {
                s.offers.insert(id.id(), Vec::new());
            }
            ext_data_control_device_v1::Event::PrimarySelection { id } => {
                let new = id.map(|o| {
                    let mimes = s.offers.remove(&o.id()).unwrap_or_default();
                    (o, mimes)
                });
                if let Some((old, _)) = std::mem::replace(&mut *s.primary.lock().unwrap(), new) {
                    old.destroy();
                }
            }
            // The clipboard (Ctrl+C) isn't ours to read.
            ext_data_control_device_v1::Event::Selection { id: Some(o) } => {
                s.offers.remove(&o.id());
                o.destroy();
            }
            _ => {}
        }
    }

    event_created_child!(State, ExtDataControlDeviceV1, [
        ext_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ExtDataControlOfferV1, ())
    ]);
}

impl Dispatch<ExtDataControlOfferV1, ()> for State {
    fn event(
        s: &mut Self,
        o: &ExtDataControlOfferV1,
        e: ext_data_control_offer_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_data_control_offer_v1::Event::Offer { mime_type } = e
            && let Some(m) = s.offers.get_mut(&o.id())
        {
            m.push(mime_type);
        }
    }
}

pub struct Wayland {
    conn: Connection,
    layout: Option<ZcosmicKeyboardLayoutV1>,
    primary: Option<Primary>,
}

impl Wayland {
    /// Connects (privileged fd if given, else `WAYLAND_DISPLAY`), binds what the
    /// compositor offers, reads the current group, then dispatches on its own thread.
    pub fn connect(
        group: watch::Sender<u32>,
        focus: mpsc::UnboundedSender<String>,
    ) -> Result<Self, Box<dyn Error>> {
        let conn =
            match lsc::fd::socket_fd(std::env::var(lsc::fd::DAEMON_WAYLAND_FD).ok().as_deref()) {
                Some(fd) => {
                    // SAFETY: the applet hands this fd to us for exactly this use. The
                    // duplicate is close-on-exec; the original (our stdin) is closed.
                    let fd = unsafe { OwnedFd::from_raw_fd(fd) }.try_clone()?;
                    Connection::from_socket(UnixStream::from(fd))?
                }
                None => Connection::connect_to_env()?,
            };
        let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
        let qh = queue.handle();
        let seat: WlSeat = globals.bind(&qh, 1..=1, ())?;
        let layout = globals
            .bind::<ZcosmicKeyboardLayoutManagerV1, _, _>(&qh, 1..=1, ())
            .ok()
            .map(|m| m.get_keyboard_layout(&seat.get_keyboard(&qh, ()), &qh, ()));
        let primary = globals
            .bind::<ExtDataControlManagerV1, _, _>(&qh, 1..=1, ())
            .ok()
            .map(|m| {
                m.get_data_device(&seat, &qh, ());
                Primary::default()
            });
        let mut state = State {
            group,
            focus,
            offers: HashMap::new(),
            primary: primary.clone().unwrap_or_default(),
        };
        queue.roundtrip(&mut state)?;
        std::thread::spawn(move || while queue.blocking_dispatch(&mut state).is_ok() {});
        Ok(Self {
            conn,
            layout,
            primary,
        })
    }

    pub fn has_layout(&self) -> bool {
        self.layout.is_some()
    }

    pub fn has_selection(&self) -> bool {
        self.primary.is_some()
    }

    /// The highlighted text. Blocks until its owner has written it: call off the
    /// async runtime, under a timeout.
    // ponytail: an owner that never closes the pipe leaves one blocking thread behind.
    pub fn read_selection(&self) -> Option<String> {
        let (offer, mime) = {
            let primary = self.primary.as_ref()?.lock().unwrap();
            let (offer, mimes) = primary.as_ref()?;
            (offer.clone(), text_mime(mimes)?)
        };
        let (mut read, write) = std::io::pipe().ok()?;
        offer.receive(mime.to_owned(), write.as_fd());
        self.conn.flush().ok()?;
        drop(write);
        let mut text = String::new();
        read.read_to_string(&mut text).ok()?;
        Some(text)
    }

    pub fn set_group(&self, group: u32) {
        if let Some(l) = &self.layout {
            l.set_group(group);
            let _ = self.conn.flush();
        }
    }
}

/// Text types we can read, most preferred first.
const TEXT_MIMES: &[&str] = &["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];

/// The most preferred text type the selection's owner offers.
pub fn text_mime(offered: &[String]) -> Option<&'static str> {
    TEXT_MIMES
        .iter()
        .copied()
        .find(|m| offered.iter().any(|o| o == m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_mime_prefers_utf8_plain_text() {
        let offered = |m: &[&str]| m.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            text_mime(&offered(&[
                "text/html",
                "text/plain",
                "text/plain;charset=utf-8"
            ])),
            Some("text/plain;charset=utf-8")
        );
        assert_eq!(
            text_mime(&offered(&["UTF8_STRING", "text/plain"])),
            Some("UTF8_STRING")
        );
        assert_eq!(text_mime(&offered(&["image/png"])), None);
    }
}
