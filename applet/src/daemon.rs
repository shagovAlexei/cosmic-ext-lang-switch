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

pub fn save_config(config: &Config) {
    if let Ok(h) = cosmic_config::Config::new(APP_ID, Config::VERSION)
        && let Err(e) = config.write_entry(&h)
    {
        log::error!("config write: {e:?}");
    }
}

/// Starts a program detached from the applet.
pub fn launch(program: impl AsRef<std::ffi::OsStr>, arg: &str) {
    let mut cmd = std::process::Command::new(program);
    cmd.arg(arg);
    tokio::spawn(cosmic::process::spawn(cmd));
}
