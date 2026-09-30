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
use std::error::Error;
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use tokio::sync::{mpsc, watch};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_keyboard::WlKeyboard, wl_registry::WlRegistry, wl_seat::WlSeat};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};

struct State {
    group: watch::Sender<u32>,
    #[allow(dead_code)] // used from Task 4
    focus: mpsc::UnboundedSender<String>,
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

pub struct Wayland {
    conn: Connection,
    layout: Option<ZcosmicKeyboardLayoutV1>,
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
        let mut state = State { group, focus };
        queue.roundtrip(&mut state)?;
        std::thread::spawn(move || while queue.blocking_dispatch(&mut state).is_ok() {});
        Ok(Self { conn, layout })
    }

    pub fn has_layout(&self) -> bool {
        self.layout.is_some()
    }

    pub fn set_group(&self, group: u32) {
        if let Some(l) = &self.layout {
            l.set_group(group);
            let _ = self.conn.flush();
        }
    }
}
