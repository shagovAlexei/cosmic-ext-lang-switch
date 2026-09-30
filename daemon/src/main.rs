// SPDX-License-Identifier: GPL-3.0-only
mod input;
mod layout;

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

#[tokio::main]
async fn main() {
    if std::env::args().any(|a| a == "--check") {
        std::process::exit(check().await);
    }
}
