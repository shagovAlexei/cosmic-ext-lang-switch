// SPDX-License-Identifier: GPL-3.0-only
//! Automatic correction: is a just-typed word in the wrong layout?
//! Letter-trigram models for English and Russian (built by `tools/gen-model`
//! from hunspell dictionaries) plus a dictionary veto.

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

    #[test]
    fn language_is_detected_from_letters() {
        assert_eq!(Lang::of("hello"), Some(Lang::En));
        assert_eq!(Lang::of("привет"), Some(Lang::Ru));
        assert_eq!(Lang::of("123"), None);
    }
}
