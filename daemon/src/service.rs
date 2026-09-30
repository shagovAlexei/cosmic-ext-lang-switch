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
    let session = logind_zbus::session::SessionProxy::builder(&conn)
        .path(path)?
        .build()
        .await?;
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
