// SPDX-License-Identifier: GPL-3.0-only
//! The focused window's app id, for auto-correction's per-app exclusions.
//!
//! COSMIC reports which window is active only on the privileged Wayland
//! connection that the panel hands its applets (`X_PRIVILEGED_WAYLAND_SOCKET`,
//! granted by `X-HostWaylandDisplay=true` in the .desktop file). The daemon can't
//! see it, so the applet forwards it over D-Bus.
use cosmic::cctk::cosmic_protocols::toplevel_info::v1::client::{
    zcosmic_toplevel_handle_v1::{self, ZcosmicToplevelHandleV1},
    zcosmic_toplevel_info_v1::{self, ZcosmicToplevelInfoV1},
};
use cosmic::cctk::wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    backend::ObjectId,
    event_created_child,
    globals::{GlobalListContents, registry_queue_init},
    protocol::wl_registry::WlRegistry,
};
use cosmic::cctk::wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};
use cosmic::iced::futures::{SinkExt, StreamExt, channel::mpsc};
use cosmic::iced::{Subscription, stream};
use std::collections::{HashMap, HashSet};
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;

/// `zcosmic_toplevel_handle_v1` state value for the focused window.
const ACTIVATED: u32 = 2;

struct State {
    info: ZcosmicToplevelInfoV1,
    /// ext handle → app id.
    app_ids: HashMap<ObjectId, String>,
    /// ext handle → its cosmic handle, to destroy it when the window closes.
    cosmic_of: HashMap<ObjectId, ZcosmicToplevelHandleV1>,
    /// cosmic handle → ext handle.
    ext_of: HashMap<ObjectId, ObjectId>,
    /// Cosmic handles currently activated.
    activated: HashSet<ObjectId>,
    tx: mpsc::UnboundedSender<String>,
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
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
        if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = e {
            let cosmic = s.info.get_cosmic_toplevel(&toplevel, qh, ());
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
                    let _ = s.tx.unbounded_send(app_id);
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
            let _ = s.tx.unbounded_send(app);
        }
    }
}

/// Blocks, sending the app id of each window that gains focus; returns if Wayland goes away.
fn watch(tx: mpsc::UnboundedSender<String>) {
    let Some(socket) = std::env::var("X_PRIVILEGED_WAYLAND_SOCKET")
        .ok()
        .and_then(|fd| fd.parse::<RawFd>().ok())
        // SAFETY: the panel passes this fd to us for exactly this use.
        .map(|fd| unsafe { OwnedFd::from_raw_fd(fd) })
        // The inherited fd isn't close-on-exec, so programs we launch would get
        // privileged Wayland access; its duplicate is.
        .and_then(|fd| fd.try_clone().ok())
        .map(UnixStream::from)
    else {
        log::info!("no privileged Wayland socket (not run by the panel): per-app exclusions off");
        return;
    };
    let run = || -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::from_socket(socket)?;
        let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
        let qh = queue.handle();
        let info: ZcosmicToplevelInfoV1 = globals.bind(&qh, 2..=3, ())?;
        let _list: ExtForeignToplevelListV1 = globals.bind(&qh, 1..=1, ())?;
        let mut state = State {
            info,
            app_ids: HashMap::new(),
            cosmic_of: HashMap::new(),
            ext_of: HashMap::new(),
            activated: HashSet::new(),
            tx,
        };
        loop {
            queue.blocking_dispatch(&mut state)?;
        }
    };
    if let Err(e) = run() {
        log::warn!("active window tracking stopped: {e}");
    }
}

/// The focused window's app id whenever a window gains focus. Each one is also
/// sent to the daemon from here, one after another, so they arrive in order.
pub fn subscription() -> Subscription<String> {
    Subscription::run(|| {
        stream::channel(8, |mut out: mpsc::Sender<String>| async move {
            let (tx, mut rx) = mpsc::unbounded();
            std::thread::spawn(move || watch(tx));
            while let Some(app) = rx.next().await {
                crate::daemon::set_active_app(app.clone()).await;
                let _ = out.send(app).await;
            }
        })
    })
}
