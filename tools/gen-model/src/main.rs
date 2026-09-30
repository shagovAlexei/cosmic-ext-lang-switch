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

/// Same physical keys: US (ANSI) and Russian ЙЦУКЕН.
const US: &str = "`qwertyuiop[]asdfghjkl;'zxcvbnm,.";
const RU: &str = "ёйцукенгшщзхъфывапролджэячсмитьбю";

fn remap(word: &str, from: &str, to: &str) -> Option<String> {
    word.chars()
        .map(|c| {
            from.chars()
                .position(|f| f == c)
                .and_then(|i| to.chars().nth(i))
        })
        .collect()
}

/// Detection rate and false-positive rate on the dictionaries, over a threshold grid.
/// No veto: dictionary words would be vetoed anyway; this measures the trigrams
/// alone, a stand-in for words the dictionary lacks (inflections, names, slang).
fn eval(dir: &str) {
    let model = lsc::auto::Model::builtin();
    let veto = lsc::auto::Veto::default();
    let en = words(dir, "en_US", Lang::En);
    let ru = words(dir, "ru_RU", Lang::Ru);
    let long = |w: &&String| w.chars().count() >= lsc::auto::MIN_LEN;
    // (typed, other, should convert)
    let mut cases: Vec<(String, String, bool)> = Vec::new();
    for w in en.iter().filter(long) {
        if let Some(r) = remap(w, US, RU) {
            cases.push((r.clone(), w.clone(), true)); // English typed on the Russian layout
            cases.push((w.clone(), r, false)); // English typed right
        }
    }
    for w in ru.iter().filter(long) {
        if let Some(u) = remap(w, RU, US) {
            cases.push((u.clone(), w.clone(), true));
            cases.push((w.clone(), u, false));
        }
    }
    let pos = cases.iter().filter(|c| c.2).count();
    let neg = cases.len() - pos;
    println!("cases: {pos} wrong-layout, {neg} right-layout");
    println!("margin floor  caught%  false+%");
    for margin in [0.5_f32, 1.0, 1.5, 2.0, 2.5, 3.0] {
        for floor in [-7.0_f32, -6.0, -5.5, -5.0, -4.5] {
            let (mut tp, mut fp) = (0, 0);
            for (typed, other, want) in &cases {
                let got =
                    lsc::auto::should_convert_with(typed, other, &model, &veto, &[], margin, floor);
                match (got, want) {
                    (true, true) => tp += 1,
                    (true, false) => fp += 1,
                    _ => {}
                }
            }
            #[allow(clippy::cast_precision_loss)]
            let pct = |a: usize, b: usize| 100.0 * a as f64 / b as f64;
            println!(
                "{margin:>6} {floor:>5}  {:>6.2}  {:>6.3}",
                pct(tp, pos),
                pct(fp, neg)
            );
        }
    }
}

fn main() {
    if std::env::args().any(|a| a == "--eval") {
        let dir = std::env::args()
            .skip(1)
            .find(|a| a != "--eval")
            .unwrap_or_else(|| "/usr/share/hunspell".into());
        return eval(&dir);
    }
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
