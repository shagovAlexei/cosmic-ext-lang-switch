// SPDX-License-Identifier: GPL-3.0-only
use crate::wayland::Wayland;
use std::sync::Arc;

pub struct Service {
    /// (panel label, xkb layout code, description) per layout group.
    pub layouts: Vec<(String, String, String)>,
    pub current: u32,
    pub status: String,
    pub layout: Option<Arc<Wayland>>,
    pub pause: tokio::sync::mpsc::UnboundedSender<bool>,
}

#[zbus::interface(name = "io.github.shagovAlexei.CosmicExtLangSwitch")]
impl Service {
    fn set_layout(&self, index: u32) {
        if let Some(l) = &self.layout
            && (index as usize) < self.layouts.len()
        {
            l.set_group(index);
        }
    }
    fn set_paused(&self, paused: bool) {
        let _ = self.pause.send(paused);
    }
    #[zbus(property)]
    fn layouts(&self) -> Vec<(String, String, String)> {
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

/// Forwards logind Lock of the user's graphical session. COSMIC never sends
/// Unlock, so the daemon must not pause on Lock: it only clears state, and the
/// password typed on the lock screen is dropped by the Enter or click that ends it.
pub async fn lock_signals(tx: tokio::sync::mpsc::UnboundedSender<()>) -> zbus::Result<()> {
    use futures_util::StreamExt;
    let conn = zbus::Connection::system().await?;
    let manager = logind_zbus::manager::ManagerProxy::new(&conn).await?;
    // "auto" resolves to the user's display session even from a systemd --user unit.
    // "auto" resolves the caller's session; from a Flatpak sandbox it may not, so fall
    // back to this user's session on seat0.
    let path = match manager.get_session("auto").await {
        Ok(p) => p,
        Err(e) => {
            let uid = std::os::unix::fs::MetadataExt::uid(&std::fs::metadata("/proc/self")?);
            manager
                .list_sessions()
                .await?
                .into_iter()
                .find(|s| s.uid() == uid && s.seat() == "seat0")
                .map(|s| s.path().clone())
                .ok_or(e)?
        }
    };
    let session = logind_zbus::session::SessionProxy::builder(&conn)
        .path(path)?
        .build()
        .await?;
    let mut lock = session.receive_lock().await?;
    while lock.next().await.is_some() {
        let _ = tx.send(());
    }
    Ok(())
}
