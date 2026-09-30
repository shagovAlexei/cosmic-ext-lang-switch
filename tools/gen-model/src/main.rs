// SPDX-License-Identifier: GPL-3.0-only
//! Builds `core/data/trigrams-{en,ru}.bin` from hunspell dictionaries.
//!
//! cargo run -p gen-model -- [/usr/share/hunspell]
//!
//! Word forms come from `unmunch` (package hunspell-tools) when it is installed,
//! otherwise from the dictionary stems.
use lsc::auto::{Lang, SCALE};
use std::process::Command;

/// Read every word of a dictionary, lowercase, keeping only words of that alphabet.
fn words(dir: &str, dict: &str, lang: Lang) -> Vec<String> {
    let dic = format!("{dir}/{dict}.dic");
    let aff = format!("{dir}/{dict}.aff");
    let raw = Command::new("unmunch")
        .args([&dic, &aff])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            eprintln!("{dict}: word forms from unmunch");
            String::from_utf8_lossy(&o.stdout).into_owned()
        })
        .unwrap_or_else(|| {
            eprintln!("{dict}: unmunch not found, using stems only");
            std::fs::read_to_string(&dic).unwrap_or_else(|e| panic!("{dic}: {e}"))
        });
    let mut out: Vec<String> = raw
        .lines()
        .skip(1)
        .filter_map(|l| l.split('/').next())
        .map(str::to_lowercase)
        .filter(|w| w.chars().count() >= 2 && w.chars().all(|c| lang.index(c).is_some()))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Quantized log2 P(c3 | c1 c2) with add-k smoothing.
fn build(words: &[String], lang: Lang) -> Vec<u8> {
    let n = lang.axis();
    let mut tri = vec![0u32; n * n * n];
    for w in words {
        let mut idx = vec![0, 0];
        idx.extend(w.chars().map(|c| lang.index(c).unwrap()));
        idx.push(0);
        for t in idx.windows(3) {
            tri[(t[0] * n + t[1]) * n + t[2]] += 1;
        }
    }
    let k = 0.1_f64;
    let mut out = vec![0u8; n * n * n];
    for ctx in 0..n * n {
        let total: u32 = tri[ctx * n..(ctx + 1) * n].iter().sum();
        for c in 0..n {
            let p = (f64::from(tri[ctx * n + c]) + k) / (f64::from(total) + k * n as f64);
            let q = (p.log2() * f64::from(SCALE)).round().clamp(-127.0, 0.0) as i8;
            out[ctx * n + c] = q.cast_unsigned();
        }
    }
    out
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/usr/share/hunspell".into());
    for (dict, lang, file) in [
        ("en_US", Lang::En, "trigrams-en.bin"),
        ("ru_RU", Lang::Ru, "trigrams-ru.bin"),
    ] {
        let w = words(&dir, dict, lang);
        eprintln!("{dict}: {} words", w.len());
        let path = format!("{}/../../core/data/{file}", env!("CARGO_MANIFEST_DIR"));
        std::fs::write(&path, build(&w, lang)).unwrap_or_else(|e| panic!("{path}: {e}"));
        eprintln!("wrote {path}");
    }
}
