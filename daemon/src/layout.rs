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
        state: &mut Self,
        _: &ZcosmicKeyboardLayoutV1,
        event: zcosmic_keyboard_layout_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
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
