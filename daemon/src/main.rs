// SPDX-License-Identifier: GPL-3.0-only
mod input;
mod layout;
mod service;
mod tables;

use cosmic_config::ConfigGet;
use lsc::config::{COMP_ID, Xkb, labels};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};

fn comp_xkb() -> Xkb {
    cosmic_config::Config::new(COMP_ID, 1)
        .and_then(|c| c.get::<Xkb>("xkb_config"))
        .unwrap_or_default()
}

fn comp_layouts() -> Vec<String> {
    labels(&comp_xkb())
}

/// The primary selection (highlighted text).
fn read_selection() -> Result<String, wl_clipboard_rs::paste::Error> {
    use wl_clipboard_rs::paste::{ClipboardType, MimeType, Seat, get_contents};
    let (mut pipe, _) = get_contents(ClipboardType::Primary, Seat::Unspecified, MimeType::Text)?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut pipe, &mut text)
        .map_err(wl_clipboard_rs::paste::Error::PipeCreation)?;
    Ok(text)
}

/// Delete the selection, switch to the other layout, retype it there.
async fn selection_actions(
    tables: &[lsc::selection::Table],
    group: u32,
    unknown: lsc::selection::Unknown,
) -> Option<Vec<Action>> {
    let read = tokio::task::spawn_blocking(read_selection);
    let text = tokio::time::timeout(Duration::from_millis(300), read)
        .await
        .ok()?
        .ok()?
        .ok()?;
    let (to, keys) = lsc::selection::convert(&text, tables, group, unknown)?;
    if keys.len() < text.chars().count() {
        log::warn!(
            "selection: {} character(s) no layout can type were left out",
            text.chars().count() - keys.len()
        );
    }
    Some(vec![
        Action::Backspace(1),
        Action::SwitchLayout(to),
        Action::Type(keys),
    ])
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
    let n = input::spawn_new_devices(&Arc::new(Mutex::new(HashSet::new())), &tx, false);
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
    let sel = tokio::task::spawn_blocking(read_selection).await;
    let missing = matches!(
        sel,
        Ok(Err(wl_clipboard_rs::paste::Error::MissingProtocol { .. }))
    );
    line(
        "selection",
        !missing,
        if missing {
            "no data-control protocol".into()
        } else {
            String::new()
        },
    );
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
) -> std::io::Result<bool> {
    let mut switched = true;
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
                // Replay only after the compositor applied the group. If it never
                // does, the replay retypes the text in the old layout: it comes back
                // unchanged rather than garbled.
                switched =
                    tokio::time::timeout(Duration::from_millis(300), group.wait_for(|x| x == g))
                        .await
                        .is_ok_and(|r| r.is_ok());
            }
            // Expanded into the actions above before `exec` is called.
            Action::ConvertSelection | Action::AddException(_) => {}
            Action::Type(strokes) => {
                for s in strokes {
                    kb.stroke(*s)?;
                    tokio::time::sleep(TAP).await;
                }
            }
        }
    }
    Ok(switched)
}

/// Push the auto-correction settings from the config into the engine.
fn apply_auto(engine: &mut Engine, config: &Config, active_app: &str) {
    engine.auto.enabled = config.auto_enabled;
    engine.auto.exceptions.clone_from(&config.auto_exceptions);
    engine.auto.app_excluded = config.app_excluded(active_app);
}

/// Remember undone auto-corrections; the config watch then reloads them.
fn save_exceptions(actions: &[Action]) {
    if !actions.iter().any(|a| matches!(a, Action::AddException(_))) {
        return;
    }
    let mut config = load_config();
    let mut words = config.auto_exceptions.clone();
    for a in actions {
        if let Action::AddException(word) = a
            && !words.contains(word)
        {
            words.push(word.clone());
        }
    }
    // Only this field: the settings window may be writing others right now.
    if let Err(e) = cosmic_config::Config::new(APP_ID, Config::VERSION)
        .and_then(|h| config.set_auto_exceptions(&h, words))
    {
        log::warn!("saving auto-correction exceptions: {e:?}");
    }
}

fn load_config() -> Config {
    cosmic_config::Config::new(APP_ID, Config::VERSION)
        .map(|h| Config::get_entry(&h).unwrap_or_else(|(_, c)| c))
        .unwrap_or_default()
}

/// Updates D-Bus state. A failure only loses one property-change signal, so it
/// is logged instead of taking the daemon (and the keyboard grab) down.
async fn publish(conn: &zbus::Connection, f: impl FnOnce(&mut Service)) {
    if let Err(e) = try_publish(conn, f).await {
        log::warn!("D-Bus publish failed: {e}");
    }
}

async fn try_publish(conn: &zbus::Connection, f: impl FnOnce(&mut Service)) -> zbus::Result<()> {
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
    // INFO unless RUST_LOG says otherwise (simple_logger defaults to TRACE). Never log key events.
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .with_module_level("zbus", log::LevelFilter::Warn)
        .with_module_level("tracing", log::LevelFilter::Warn)
        .env()
        .init()?;

    let (key_tx, mut key_rx) = mpsc::unbounded_channel();
    let seen: input::Seen = Arc::default();
    // Before grabbing keyboards: loading takes ~0.1 s and would stall typing.
    let (veto, words) = tables::load_veto(std::path::Path::new("/usr/share/hunspell"));
    log::info!("auto-correction dictionary veto: {words} words");
    // The virtual keyboard must exist before any keyboard is grabbed: it forwards their keys.
    let mut keyboard = input::Keyboard::new()
        .inspect_err(|e| log::error!("uinput: {e}"))
        .ok();
    let grab = keyboard.is_some();
    let devices = input::spawn_new_devices(&seen, &key_tx, grab);
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
    let mut layouts = tables::describe(&comp_xkb());
    let mut tables = tables::build(&comp_xkb());
    let mut engine = Engine::new(config.hotkeys());
    engine.set_layouts(layouts.len() as u32);
    engine.set_enabled(config.enabled && status == "ok");
    engine.on_group(*group_rx.borrow_and_update());
    engine.auto.veto = veto;
    engine.auto.tables.clone_from(&tables);
    // Set by the applet over D-Bus; empty until it reports (then nothing is excluded).
    let mut active_app = String::new();
    apply_auto(&mut engine, &config, &active_app);

    let (cfg_tx, mut cfg_rx) = mpsc::unbounded_channel();
    let watch_cfg = |id: &str, version| {
        let tx = cfg_tx.clone();
        cosmic_config::Config::new(id, version)?.watch(move |_, _| {
            let _ = tx.send(());
        })
    };
    let _app_watch = watch_cfg(APP_ID, Config::VERSION)?;
    let _comp_watch = watch_cfg(COMP_ID, 1)?;

    let (pause_tx, mut pause_rx) = mpsc::unbounded_channel();
    // A settings window may close mid-recording; never stay paused for long.
    let mut pause_until: Option<tokio::time::Instant> = None;
    let (app_tx, mut app_rx) = mpsc::unbounded_channel();
    let service = Service {
        pause: pause_tx,
        active_app: app_tx,
        layouts: layouts.clone(),
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

    let mut rescan = tokio::time::interval(Duration::from_secs(2));
    loop {
        tokio::select! {
            Some((code, value, grabbed)) = key_rx.recv() => {
                let out = engine.feed(code, value);
                if grabbed && let Some(kb) = keyboard.as_mut() {
                    let sent = if out.forward { kb.key(code, value) } else { Ok(()) };
                    if let Err(e) = sent.and_then(|()| out.tap_instead.map_or(Ok(()), |k| kb.tap(k))) {
                        log::error!("forward failed: {e}");
                    }
                }
                let Some(mut actions) = out.actions else { continue };
                if actions == [Action::ConvertSelection] {
                    let group = *group_rx.borrow();
                    let Some(a) = selection_actions(&tables, group, config.unknown()).await else { continue };
                    actions = a;
                }
                if let (Some(kb), Some(l)) = (keyboard.as_mut(), layout.as_deref()) {
                    match exec(kb, l, &mut group_rx, &actions).await {
                        Ok(true) => {}
                        Ok(false) => {
                            log::warn!("layout switch not confirmed within 300 ms; text left as typed");
                            engine.switch_failed();
                        }
                        Err(e) => log::error!("replay failed: {e}"),
                    }
                }
                save_exceptions(&actions);
                let g = *group_rx.borrow_and_update();
                engine.on_group(g);
                publish(&conn, |s| s.current = g).await;
            }
            Ok(()) = group_rx.changed() => {
                let g = *group_rx.borrow_and_update();
                engine.on_group(g);
                publish(&conn, |s| s.current = g).await;
            }
            Some(()) = cfg_rx.recv() => {
                config = load_config();
                layouts = tables::describe(&comp_xkb());
                tables = tables::build(&comp_xkb());
                engine.set_hotkeys(config.hotkeys());
                engine.auto.tables.clone_from(&tables);
                apply_auto(&mut engine, &config, &active_app);
                engine.set_layouts(layouts.len() as u32);
                engine.set_enabled(config.enabled && status == "ok");
                let l = layouts.clone();
                publish(&conn, |s| s.layouts = l).await;
            }
            Some(app) = app_rx.recv() => {
                // New focus: the typed buffer belongs to the old window.
                engine.reset();
                engine.auto.app_excluded = config.app_excluded(&app);
                active_app = app;
            }
            Some(p) = pause_rx.recv() => {
                engine.set_paused(p);
                pause_until = p.then(|| tokio::time::Instant::now() + Duration::from_secs(30));
            }
            () = tokio::time::sleep_until(pause_until.unwrap_or_else(tokio::time::Instant::now)), if pause_until.is_some() => {
                engine.set_paused(false);
                pause_until = None;
                log::info!("pause expired");
            }
            Some(()) = lock_rx.recv() => {
                engine.reset();
                engine.release_mods();
                // No Unlock signal on COSMIC: auto-correction stays off (the lock screen
                // takes passwords) until the applet reports a focused window again.
                engine.auto.app_excluded = true;
                log::info!("session locked: buffer cleared, auto-correction suspended");
            }
            _ = rescan.tick() => { input::spawn_new_devices(&seen, &key_tx, grab); }
        }
    }
}
