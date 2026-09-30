// SPDX-License-Identifier: GPL-3.0-only
//! What the panel popup and the settings window share: daemon state over D-Bus,
//! our config, launching programs.
use crate::fl;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::futures::{SinkExt, StreamExt, channel::mpsc::Sender};
use cosmic::iced::{Subscription, stream};
use lsc::config::{APP_ID, Config};
use lsc::dbus::LangSwitchProxy;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Daemon {
    /// (panel label, xkb layout code, description) per layout group.
    pub layouts: Vec<(String, String, String)>,
    pub current: u32,
    /// Empty when the daemon isn't on the bus.
    pub status: String,
}

impl Daemon {
    /// Panel label of the active layout.
    pub fn label(&self) -> &str {
        self.layouts
            .get(self.current as usize)
            .map_or("??", |l| l.0.as_str())
    }

    /// What's wrong, if anything, in words for the user.
    pub fn warning(&self) -> Option<String> {
        match self.status.as_str() {
            "ok" => None,
            "" => Some(fl!("daemon-missing")),
            "no-input-access" => Some(fl!("no-input-access")),
            _ => Some(fl!("no-layout-protocol")),
        }
    }
}

/// Sends daemon state on every property change; returns only on a D-Bus error.
async fn watch(out: &mut Sender<Daemon>) -> zbus::Result<()> {
    let conn = zbus::Connection::session().await?;
    let p = LangSwitchProxy::new(&conn).await?;
    let mut cur = p.receive_current_layout_changed().await;
    let mut lay = p.receive_layouts_changed().await;
    let mut owner = p.inner().receive_owner_changed().await?;
    loop {
        let d = Daemon {
            layouts: p.layouts().await?,
            current: p.current_layout().await?,
            status: p.status().await?,
        };
        let _ = out.send(d).await;
        tokio::select! {
            _ = cur.next() => {}
            _ = lay.next() => {}
            o = owner.next() => if o.is_none_or(|o| o.is_none()) { return Ok(()) },
        }
    }
}

/// Streams daemon state; reconnects every 2 s while the daemon is absent.
pub fn subscription() -> Subscription<Daemon> {
    Subscription::run(|| {
        stream::channel(16, |mut out: Sender<Daemon>| async move {
            loop {
                let _ = watch(&mut out).await;
                let _ = out.send(Daemon::default()).await;
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        })
    })
}

async fn proxy() -> zbus::Result<LangSwitchProxy<'static>> {
    LangSwitchProxy::new(&zbus::Connection::session().await?).await
}

pub async fn set_layout(index: u32) {
    if let Err(e) = async { proxy().await?.set_layout(index).await }.await {
        log::error!("set_layout: {e}");
    }
}

pub async fn set_paused(paused: bool) {
    if let Err(e) = async { proxy().await?.set_paused(paused).await }.await {
        log::error!("set_paused: {e}");
    }
}

pub fn load_config() -> Config {
    cosmic_config::Config::new(APP_ID, Config::VERSION)
        .map(|h| Config::get_entry(&h).unwrap_or_else(|(_, c)| c))
        .unwrap_or_default()
}

/// Writes one field (`|h| config.set_x(h, v)`), never the whole config: the daemon,
/// the popup and the settings window each hold their own copy, possibly stale.
pub fn save(set: impl FnOnce(&cosmic_config::Config) -> Result<bool, cosmic_config::Error>) {
    if let Err(e) = cosmic_config::Config::new(APP_ID, Config::VERSION).and_then(|h| set(&h)) {
        log::error!("config write: {e:?}");
    }
}

/// Unless a daemon already runs, starts one next to our binary, handing it the panel's
/// privileged Wayland connection (only there does cosmic-comp show a sandboxed process
/// the layout, selection and focus protocols). When that daemon exits, so does the
/// applet, with an error code: only then does the panel restart it, with a fresh
/// connection, and the new instance starts a new daemon.
pub async fn run_daemon() {
    use std::os::fd::{FromRawFd, OwnedFd};
    // Taken (and closed if unused) either way, so programs we launch don't inherit it.
    let socket = lsc::fd::socket_fd(std::env::var("X_PRIVILEGED_WAYLAND_SOCKET").ok().as_deref())
        // SAFETY: the panel passes this fd to us for exactly this use; checked open above.
        .map(|fd| unsafe { OwnedFd::from_raw_fd(fd) });
    let running = async {
        let conn = zbus::Connection::session().await?;
        let name = zbus::names::BusName::try_from(lsc::dbus::BUS_NAME)?;
        let dbus = zbus::fdo::DBusProxy::new(&conn).await?;
        Ok::<_, zbus::Error>(dbus.name_has_owner(name).await?)
    };
    if running.await.unwrap_or(false) {
        return;
    }
    let Some(socket) = socket else {
        log::warn!("no privileged Wayland socket (not run by the panel): daemon not started");
        return;
    };
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let spawned = tokio::process::Command::new(exe.with_file_name("cosmic-ext-lang-switch-daemon"))
        .stdin(std::process::Stdio::from(socket))
        .env(lsc::fd::DAEMON_WAYLAND_FD, "0")
        .spawn();
    match spawned {
        Ok(mut child) => {
            let status = child.wait().await;
            log::warn!("daemon exited ({status:?}); restarting the applet");
            std::process::exit(1);
        }
        Err(e) => log::error!("starting the daemon: {e}"),
    }
}

/// Starts a host program (cosmic-settings), from inside Flatpak too.
pub fn launch_host(program: &str, arg: &str) {
    if std::path::Path::new("/.flatpak-info").exists() {
        let mut cmd = std::process::Command::new("flatpak-spawn");
        cmd.args(["--host", program, arg]);
        tokio::spawn(cosmic::process::spawn(cmd));
    } else {
        launch(program, arg);
    }
}

/// Starts a program detached from the applet.
pub fn launch(program: impl AsRef<std::ffi::OsStr>, arg: &str) {
    let mut cmd = std::process::Command::new(program);
    cmd.arg(arg);
    tokio::spawn(cosmic::process::spawn(cmd));
}
