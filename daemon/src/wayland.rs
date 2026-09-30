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
use cosmic_protocols::toplevel_info::v1::client::{
    zcosmic_toplevel_handle_v1::{self, ZcosmicToplevelHandleV1},
    zcosmic_toplevel_info_v1::{self, ZcosmicToplevelInfoV1},
};
use std::collections::{HashMap, HashSet};
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

use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};

/// `zcosmic_toplevel_handle_v1` state value for the focused window.
const ACTIVATED: u32 = 2;

/// The primary selection's offer and its mime types.
type Primary = Arc<Mutex<Option<(ExtDataControlOfferV1, Vec<String>)>>>;

struct State {
    group: watch::Sender<u32>,
    /// App id of each window that gains focus.
    focus: mpsc::UnboundedSender<String>,
    info: Option<ZcosmicToplevelInfoV1>,
    /// ext handle → app id.
    app_ids: HashMap<ObjectId, String>,
    /// ext handle → its cosmic handle, to destroy it when the window closes.
    cosmic_of: HashMap<ObjectId, ZcosmicToplevelHandleV1>,
    /// cosmic handle → ext handle.
    ext_of: HashMap<ObjectId, ObjectId>,
    /// Cosmic handles currently activated.
    activated: HashSet<ObjectId>,
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

impl Dispatch<ExtForeignToplevelListV1, ()> for State {
    fn event(
        s: &mut Self,
        _: &ExtForeignToplevelListV1,
        e: ext_foreign_toplevel_list_v1::Event,
        (): &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = e
            && let Some(info) = &s.info
        {
            let cosmic = info.get_cosmic_toplevel(&toplevel, qh, ());
            s.ext_of.insert(cosmic.id(), toplevel.id());
            s.cosmic_of.insert(toplevel.id(), cosmic);
        }
    }

    event_created_child!(State, ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ())
    ]);
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for State {
    fn event(
        s: &mut Self,
        h: &ExtForeignToplevelHandleV1,
        e: ext_foreign_toplevel_handle_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            ext_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                let focused = s
                    .cosmic_of
                    .get(&h.id())
                    .is_some_and(|c| s.activated.contains(&c.id()));
                // A focused window whose app id arrives late (or changes) is reported again.
                if s.app_ids.insert(h.id(), app_id.clone()).as_ref() != Some(&app_id) && focused {
                    let _ = s.focus.send(app_id);
                }
            }
            ext_foreign_toplevel_handle_v1::Event::Closed => {
                s.app_ids.remove(&h.id());
                if let Some(cosmic) = s.cosmic_of.remove(&h.id()) {
                    s.ext_of.remove(&cosmic.id());
                    s.activated.remove(&cosmic.id());
                    cosmic.destroy();
                }
                h.destroy();
            }
            _ => {}
        }
    }
}

impl Dispatch<ZcosmicToplevelInfoV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZcosmicToplevelInfoV1,
        _: zcosmic_toplevel_info_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZcosmicToplevelHandleV1, ()> for State {
    fn event(
        s: &mut Self,
        h: &ZcosmicToplevelHandleV1,
        e: zcosmic_toplevel_handle_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let zcosmic_toplevel_handle_v1::Event::State { state } = e else {
            return;
        };
        let activated = state
            .as_chunks::<4>()
            .0
            .iter()
            .any(|&c| u32::from_ne_bytes(c) == ACTIVATED);
        if !activated {
            s.activated.remove(&h.id());
            return;
        }
        // Report every time a window *gains* focus, even the same app again: after the
        // lock screen the daemon waits for such a report to resume auto-correction.
        if s.activated.insert(h.id()) {
            let app = s
                .ext_of
                .get(&h.id())
                .and_then(|ext| s.app_ids.get(ext))
                .cloned()
                .unwrap_or_default();
            let _ = s.focus.send(app);
        }
    }
}

pub struct Wayland {
    conn: Connection,
    layout: Option<ZcosmicKeyboardLayoutV1>,
    primary: Option<Primary>,
    has_focus: bool,
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
        let info = globals
            .bind::<ZcosmicToplevelInfoV1, _, _>(&qh, 2..=3, ())
            .ok();
        let _list = info.as_ref().and_then(|_| {
            globals
                .bind::<ExtForeignToplevelListV1, _, _>(&qh, 1..=1, ())
                .ok()
        });
        let has_focus = info.is_some();
        let mut state = State {
            group,
            focus,
            info,
            app_ids: HashMap::new(),
            cosmic_of: HashMap::new(),
            ext_of: HashMap::new(),
            activated: HashSet::new(),
            offers: HashMap::new(),
            primary: primary.clone().unwrap_or_default(),
        };
        queue.roundtrip(&mut state)?;
        std::thread::spawn(move || while queue.blocking_dispatch(&mut state).is_ok() {});
        Ok(Self {
            conn,
            layout,
            primary,
            has_focus,
        })
    }

    pub fn has_layout(&self) -> bool {
        self.layout.is_some()
    }

    /// Whether the compositor reports toplevels here. Activation events come only on
    /// the privileged connection; on `WAYLAND_DISPLAY` app exclusions stay off.
    pub fn has_focus(&self) -> bool {
        self.has_focus
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
