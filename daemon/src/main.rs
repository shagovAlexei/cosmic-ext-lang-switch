// SPDX-License-Identifier: GPL-3.0-only
mod input;
mod layout;
mod service;

use cosmic_config::ConfigGet;
use lsc::config::{COMP_ID, Xkb, labels};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};

fn comp_layouts() -> Vec<String> {
    cosmic_config::Config::new(COMP_ID, 1)
        .and_then(|c| c.get::<Xkb>("xkb_config"))
        .map(|x| labels(&x))
        .unwrap_or_default()
}

/// Probes each backend through the same code the daemon uses. Exit code 1 if any is missing.
async fn check() -> i32 {
    let mut bad = 0;
    let mut line = |name: &str, ok: bool, detail: String| {
        println!(
            "{name:<10} {:<8} {detail}",
            if ok { "ok" } else { "MISSING" }
        );
        bad += i32::from(!ok);
    };
    let (tx, _rx) = mpsc::unbounded_channel();
    let n = input::spawn_new_devices(&Arc::new(Mutex::new(HashSet::new())), &tx);
    line(
        "input",
        n > 0,
        format!("{n} device(s) readable (need group `input`)"),
    );
    let kb = input::Keyboard::new();
    line(
        "uinput",
        kb.is_ok(),
        kb.err().map_or_else(String::new, |e| e.to_string()),
    );
    let (gtx, grx) = watch::channel(0);
    let l = layout::Layout::connect(gtx);
    line(
        "layout",
        l.is_ok(),
        l.err().map_or_else(
            || format!("current group {}", *grx.borrow()),
            |e| e.to_string(),
        ),
    );
    let names = comp_layouts();
    line("layouts", names.len() >= 2, names.join(","));
    bad.min(1)
}

use cosmic_config::CosmicConfigEntry;
use lsc::config::{APP_ID, Config};
use lsc::dbus::{BUS_NAME, PATH};
use lsc::engine::{Action, Engine};
use service::Service;
use std::time::Duration;

// ponytail: fixed per-key delay; raise if some app drops replayed keys.
const TAP: Duration = Duration::from_millis(3);

async fn exec(
    kb: &mut input::Keyboard,
    layout: &layout::Layout,
    group: &mut watch::Receiver<u32>,
    actions: &[Action],
) -> std::io::Result<()> {
    for a in actions {
        match a {
            Action::Backspace(n) => {
                for _ in 0..*n {
                    kb.backspace()?;
                    tokio::time::sleep(TAP).await;
                }
            }
            Action::SwitchLayout(g) => {
                layout.set_group(*g);
                // Replay only after the compositor applied the group.
                let _ =
                    tokio::time::timeout(Duration::from_millis(300), group.wait_for(|x| x == g))
                        .await;
            }
            Action::Type(strokes) => {
                for s in strokes {
                    kb.stroke(*s)?;
                    tokio::time::sleep(TAP).await;
                }
            }
        }
    }
    Ok(())
}

fn load_config() -> Config {
    cosmic_config::Config::new(APP_ID, Config::VERSION)
        .map(|h| Config::get_entry(&h).unwrap_or_else(|(_, c)| c))
        .unwrap_or_default()
}

async fn publish(conn: &zbus::Connection, f: impl FnOnce(&mut Service)) -> zbus::Result<()> {
    let iface = conn.object_server().interface::<_, Service>(PATH).await?;
    let mut s = iface.get_mut().await;
    f(&mut s);
    s.layouts_changed(iface.signal_emitter()).await?;
    s.current_layout_changed(iface.signal_emitter()).await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|a| a == "--check") {
        std::process::exit(check().await);
    }
    simple_logger::init_with_env()?; // log state changes only, never key events

    let (key_tx, mut key_rx) = mpsc::unbounded_channel();
    let seen: input::Seen = Arc::default();
    let devices = input::spawn_new_devices(&seen, &key_tx);
    let mut keyboard = input::Keyboard::new()
        .inspect_err(|e| log::error!("uinput: {e}"))
        .ok();
    let (group_tx, mut group_rx) = watch::channel(0);
    let layout = layout::Layout::connect(group_tx)
        .inspect_err(|e| log::error!("layout protocol: {e}"))
        .ok()
        .map(Arc::new);
    let status = match (devices > 0 && keyboard.is_some(), layout.is_some()) {
        (false, _) => "no-input-access",
        (true, false) => "no-layout-protocol",
        (true, true) => "ok",
    };

    let mut config = load_config();
    let mut names = comp_layouts();
    let mut engine = Engine::new(config.hotkeys());
    engine.set_layouts(names.len() as u32);
    engine.set_enabled(config.enabled && status == "ok");
    engine.on_group(*group_rx.borrow_and_update());

    let (cfg_tx, mut cfg_rx) = mpsc::unbounded_channel();
    let watch_cfg = |id: &str, version| {
        let tx = cfg_tx.clone();
        cosmic_config::Config::new(id, version)?.watch(move |_, _| {
            let _ = tx.send(());
        })
    };
    let _app_watch = watch_cfg(APP_ID, Config::VERSION)?;
    let _comp_watch = watch_cfg(COMP_ID, 1)?;

    let service = Service {
        layouts: names.clone(),
        current: *group_rx.borrow(),
        status: status.into(),
        layout: layout.clone(),
    };
    let conn = zbus::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(PATH, service)?
        .build()
        .await?;

    let (lock_tx, mut lock_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        if let Err(e) = service::lock_signals(lock_tx).await {
            log::warn!("logind lock signals unavailable: {e}");
        }
    });

    let mut locked = false;
    let mut rescan = tokio::time::interval(Duration::from_secs(2));
    loop {
        tokio::select! {
            Some((code, value)) = key_rx.recv() => {
                if locked { continue; }
                let Some(actions) = engine.on_key(code, value) else { continue };
                if let (Some(kb), Some(l)) = (keyboard.as_mut(), layout.as_deref())
                    && let Err(e) = exec(kb, l, &mut group_rx, &actions).await
                {
                    log::error!("replay failed: {e}");
                }
                let g = *group_rx.borrow_and_update();
                engine.on_group(g);
                publish(&conn, |s| s.current = g).await?;
            }
            Ok(()) = group_rx.changed() => {
                let g = *group_rx.borrow_and_update();
                engine.on_group(g);
                publish(&conn, |s| s.current = g).await?;
            }
            Some(()) = cfg_rx.recv() => {
                config = load_config();
                names = comp_layouts();
                engine.set_hotkeys(config.hotkeys());
                engine.set_layouts(names.len() as u32);
                engine.set_enabled(config.enabled && status == "ok");
                let n = names.clone();
                publish(&conn, |s| s.layouts = n).await?;
            }
            Some(l) = lock_rx.recv() => {
                locked = l;
                engine.reset();
                log::info!("session {}", if l { "locked" } else { "unlocked" });
            }
            _ = rescan.tick() => { input::spawn_new_devices(&seen, &key_tx); }
        }
    }
}
