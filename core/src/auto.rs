// SPDX-License-Identifier: GPL-3.0-only
//! Automatic correction: is a just-typed word in the wrong layout?
//! Letter-trigram models for English and Russian (built by `tools/gen-model`
//! from hunspell dictionaries) plus a dictionary veto.

use crate::engine::Stroke;
use crate::selection::Table;
use std::collections::HashSet;

/// Words shorter than this are never converted: too little evidence.
pub const MIN_LEN: usize = 3;
/// How much more likely (mean log2 per trigram) the other layout's reading must be.
pub const MARGIN: f32 = 2.0;
/// The other layout's reading must itself look like a word at least this much.
pub const FLOOR: f32 = -6.0;

/// Latin tokens typed on purpose that hunspell's English dictionary lacks and whose
/// Russian-layout reading looks like Russian (`http` → `реез`): trigrams alone can't
/// tell them from a mistake, so they are always left alone.
const TECH_WORDS: &[&str] = &[
    "http", "https", "www", "html", "url", "uri", "api", "ssh", "sudo", "npm", "git", "json",
    "yaml", "toml", "sql", "css", "dns", "ftp", "tcp", "udp", "usb", "pdf", "iso", "cli", "gui",
];

/// Words known to be real in a language: typed as-is, they are never converted.
#[derive(Default)]
pub struct Veto {
    en: HashSet<String>,
    ru: HashSet<String>,
}

impl Veto {
    #[must_use]
    pub fn from_words<S: AsRef<str>>(lang: Lang, words: impl IntoIterator<Item = S>) -> Self {
        let mut v = Self::default();
        v.extend(lang, words);
        v
    }

    pub fn extend<S: AsRef<str>>(&mut self, lang: Lang, words: impl IntoIterator<Item = S>) {
        let set = match lang {
            Lang::En => &mut self.en,
            Lang::Ru => &mut self.ru,
        };
        set.extend(words.into_iter().map(|w| w.as_ref().to_lowercase()));
    }

    #[must_use]
    pub fn contains(&self, lang: Lang, word: &str) -> bool {
        let set = match lang {
            Lang::En => &self.en,
            Lang::Ru => &self.ru,
        };
        set.contains(&word.to_lowercase())
    }
}

/// The text `strokes` type under a layout table, or `None` if a key isn't in it.
#[must_use]
pub fn render(strokes: &[Stroke], table: &Table) -> Option<String> {
    strokes
        .iter()
        .map(|s| table.iter().find(|&(_, t)| t == s).map(|(&c, _)| c))
        .collect()
}

/// Should `typed` (as it appears now) be retyped as `other` (the same keys in the
/// other layout)? Conservative: identifiers, short words and known words stay.
#[must_use]
pub fn should_convert(
    typed: &str,
    other: &str,
    model: &Model,
    veto: &Veto,
    exceptions: &[String],
) -> bool {
    should_convert_with(typed, other, model, veto, exceptions, MARGIN, FLOOR)
}

/// `should_convert` with explicit thresholds (for `tools/gen-model --eval`).
#[must_use]
pub fn should_convert_with(
    typed: &str,
    other: &str,
    model: &Model,
    veto: &Veto,
    exceptions: &[String],
    margin: f32,
    floor: f32,
) -> bool {
    if typed.chars().count() < MIN_LEN || typed.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    // `myVar`, `Pass1word`: an uppercase letter after the first one, with lowercase around.
    let mixed_case =
        typed.chars().skip(1).any(char::is_uppercase) && typed.chars().any(char::is_lowercase);
    if mixed_case
        || exceptions
            .iter()
            .any(|e| e.eq_ignore_ascii_case(typed) || e.to_lowercase() == typed.to_lowercase())
    {
        return false;
    }
    let (Some(cur), Some(to)) = (Lang::of(typed), Lang::of(other)) else {
        return false;
    };
    // Judge the typed word by its letters: quotes and brackets around it (`'agent`,
    // `[advent`) would otherwise make it "impossible" and skip veto and margin.
    // (On the Russian layout those keys are letters: `,skj` is "было".)
    // Sentence punctuation typed after a word is meant as is: retyping it would turn
    // "hello," into "руддщб" (`,` and `б` share a key).
    if typed.ends_with([',', '.', ';', ':', '!', '?']) {
        return false;
    }
    let core = typed.trim_matches(|c: char| cur.index(c).is_none());
    if core.is_empty() {
        return false;
    }
    if cur == to
        || veto.contains(cur, core)
        || (cur == Lang::En && TECH_WORDS.contains(&core.to_lowercase().as_str()))
    {
        return false;
    }
    let Some(theirs) = model.lang(to).score(other) else {
        return false;
    };
    if theirs < floor {
        return false;
    }
    // A reading impossible in the current language (e.g. `c]tim`) needs no margin.
    model
        .lang(cur)
        .score(typed)
        .is_none_or(|ours| theirs - ours >= margin)
}

/// A language the models know.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Ru,
}

impl Lang {
    /// Model letters. Index 0 in a trigram is the word boundary, letters start at 1.
    #[must_use]
    pub fn alphabet(self) -> &'static str {
        match self {
            Lang::En => "abcdefghijklmnopqrstuvwxyz",
            Lang::Ru => "абвгдеёжзийклмнопрстуфхцчшщъыьэюя",
        }
    }

    /// Size of one trigram axis: the letters plus the boundary.
    #[must_use]
    pub fn axis(self) -> usize {
        self.alphabet().chars().count() + 1
    }

    /// 1-based index of a lowercase letter, `None` if not in this alphabet.
    #[must_use]
    pub fn index(self, c: char) -> Option<usize> {
        let lower = c.to_lowercase().next()?;
        self.alphabet()
            .chars()
            .position(|a| a == lower)
            .map(|i| i + 1)
    }

    /// The language whose alphabet the word's first letter belongs to.
    #[must_use]
    pub fn of(word: &str) -> Option<Lang> {
        let c = word.chars().find(|c| c.is_alphabetic())?;
        [Lang::En, Lang::Ru]
            .into_iter()
            .find(|l| l.index(c).is_some())
    }
}

/// Quantization of log2 probabilities stored in the tables: `q = round(log2(p) * SCALE)`.
pub const SCALE: f32 = 8.0;

/// A dense table of quantized log2 P(c3 | c1 c2), indexed `(c1 * axis + c2) * axis + c3`.
pub struct Trigrams {
    lang: Lang,
    table: &'static [u8],
}

impl Trigrams {
    /// Mean log2 probability per trigram of the word framed by boundaries;
    /// `None` if it has a character outside the alphabet.
    #[must_use]
    pub fn score(&self, word: &str) -> Option<f32> {
        let n = self.lang.axis();
        let mut idx = vec![0, 0];
        for c in word.chars() {
            idx.push(self.lang.index(c)?);
        }
        idx.push(0);
        let sum: f32 = idx
            .windows(3)
            .map(|w| f32::from(self.table[(w[0] * n + w[1]) * n + w[2]].cast_signed()) / SCALE)
            .sum();
        #[allow(clippy::cast_precision_loss)] // windows count is tiny
        Some(sum / (idx.len() - 2) as f32)
    }
}

/// Both language models.
pub struct Model {
    en: Trigrams,
    ru: Trigrams,
}

impl Model {
    /// The models built into the binary (`core/data/*.bin`, from `tools/gen-model`).
    #[must_use]
    pub fn builtin() -> Self {
        Self {
            en: Trigrams {
                lang: Lang::En,
                table: include_bytes!("../data/trigrams-en.bin"),
            },
            ru: Trigrams {
                lang: Lang::Ru,
                table: include_bytes!("../data/trigrams-ru.bin"),
            },
        }
    }

    #[must_use]
    pub fn lang(&self, lang: Lang) -> &Trigrams {
        match lang {
            Lang::En => &self.en,
            Lang::Ru => &self.ru,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_words_score_higher_than_wrong_layout_gibberish() {
        let m = Model::builtin();
        assert!(
            m.lang(Lang::En).score("hello").unwrap() > m.lang(Lang::En).score("ghbdtn").unwrap()
        );
        assert!(
            m.lang(Lang::Ru).score("привет").unwrap() > m.lang(Lang::Ru).score("руддщ").unwrap()
        );
    }

    #[test]
    fn scoring_ignores_case() {
        let m = Model::builtin();
        let en = m.lang(Lang::En);
        assert_eq!(en.score("Hello"), en.score("hello"));
    }

    #[test]
    fn characters_outside_the_alphabet_make_a_word_impossible() {
        let m = Model::builtin();
        assert_eq!(m.lang(Lang::En).score("c]tim"), None);
        assert_eq!(m.lang(Lang::Ru).score("hello"), None);
    }

    fn decide(typed: &str, other: &str) -> bool {
        should_convert(typed, other, &Model::builtin(), &Veto::default(), &[])
    }

    #[test]
    fn wrong_layout_words_are_converted() {
        assert!(decide("ghbdtn", "привет"));
        assert!(decide("rfr", "как"));
        assert!(decide("руддщ", "hello"));
        assert!(decide("цщкдв", "world"));
        assert!(
            decide("c]tim", "съешь"),
            "punctuation keys hold Russian letters"
        );
    }

    #[test]
    fn real_words_and_code_are_left_alone() {
        for (typed, other) in [
            ("hello", "руддщ"),
            ("grep", "пкуз"),
            ("привет", "ghbdtn"),
            ("world", "цщкдв"),
        ] {
            assert!(!decide(typed, other), "{typed}");
        }
    }

    /// Same physical keys: US and Russian ЙЦУКЕН (for building test cases).
    fn remap(word: &str, from: &str, to: &str) -> String {
        word.chars()
            .map(|c| {
                from.chars()
                    .position(|f| f == c)
                    .and_then(|i| to.chars().nth(i))
                    .unwrap()
            })
            .collect()
    }
    const US: &str = "`qwertyuiop[]asdfghjkl;'zxcvbnm,.";
    const RU: &str = "ёйцукенгшщзхъфывапролджэячсмитьбю";

    #[test]
    fn tech_words_outside_dictionaries_are_left_alone() {
        // Commands and jargon typed on purpose: no dictionary veto protects them.
        let mut wrong = Vec::new();
        for w in [
            "git",
            "npm",
            "sudo",
            "grep",
            "cargo",
            "rustc",
            "http",
            "https",
            "json",
            "yaml",
            "regex",
            "bash",
            "zsh",
            "vim",
            "nginx",
            "docker",
            "kubectl",
            "ssh",
            "tmux",
            "systemctl",
            "localhost",
            "github",
            "stdout",
            "async",
            "wayland",
            "cosmic",
            "applet",
            "config",
            "linux",
            "ubuntu",
        ] {
            if decide(w, &remap(w, US, RU)) {
                wrong.push(w);
            }
        }
        assert!(
            wrong.len() <= 1,
            "converted on purpose-typed words: {wrong:?}"
        );
    }

    #[test]
    fn common_russian_words_typed_on_us_are_caught() {
        let mut missed = Vec::new();
        for w in [
            "привет",
            "спасибо",
            "пожалуйста",
            "сегодня",
            "завтра",
            "хорошо",
            "работа",
            "вопрос",
            "человек",
            "время",
            "должен",
            "может",
            "только",
            "сейчас",
            "почему",
            "потому",
            "было",
            "будет",
            "надо",
            "здесь",
            "новый",
            "слово",
            "день",
            "дело",
            "жизнь",
        ] {
            if !decide(&remap(w, RU, US), w) {
                missed.push(w);
            }
        }
        assert!(missed.len() <= 2, "missed: {missed:?}");
    }

    #[test]
    fn short_words_digits_and_mixed_case_are_skipped() {
        assert!(!decide("ls", "ды"), "too short");
        assert!(!decide("gh1dtn", "пр1вет"), "digits");
        assert!(!decide("ghbDtn", "приВет"), "mixed case");
        assert!(decide("GHBDTN", "ПРИВЕТ"), "all caps is fine");
    }

    #[test]
    fn quotes_and_brackets_around_a_word_are_left_alone() {
        // Outside the alphabet they made the typed word "impossible", skipping veto and margin.
        let m = Model::builtin();
        let veto = Veto::from_words(Lang::En, ["agent", "aberrant", "advent", "code", "word"]);
        for w in ["'agent", "\"aberrant", "[advent", "`code`", "word'"] {
            assert!(!should_convert(w, &remap_lossy(w), &m, &veto, &[]), "{w}");
        }
        assert!(
            decide("c]tim", "съешь"),
            "a punctuation key inside a word still counts"
        );
    }

    fn remap_lossy(word: &str) -> String {
        word.chars()
            .map(|c| {
                US.chars()
                    .position(|f| f == c)
                    .and_then(|i| RU.chars().nth(i))
                    .unwrap_or(c)
            })
            .collect()
    }

    #[test]
    fn veto_and_exceptions_win() {
        let m = Model::builtin();
        let veto = Veto::from_words(Lang::En, ["ghbdtn"]);
        assert!(!should_convert("ghbdtn", "привет", &m, &veto, &[]));
        assert!(!should_convert(
            "ghbdtn",
            "привет",
            &m,
            &Veto::default(),
            &["ghbdtn".into()]
        ));
    }

    #[test]
    fn strokes_render_through_a_table() {
        let s = |code| crate::engine::Stroke { code, shift: false };
        let table: crate::selection::Table = [('g', s(34)), ('h', s(35))].into();
        assert_eq!(render(&[s(34), s(35)], &table).as_deref(), Some("gh"));
        assert_eq!(render(&[s(34), s(99)], &table), None);
    }

    #[test]
    fn language_is_detected_from_letters() {
        assert_eq!(Lang::of("hello"), Some(Lang::En));
        assert_eq!(Lang::of("привет"), Some(Lang::Ru));
        assert_eq!(Lang::of("123"), None);
    }
}
