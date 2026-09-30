// SPDX-License-Identifier: GPL-3.0-only

//! Provides localization support for this crate.
use i18n_embed::{
    DefaultLocalizer, LanguageLoader, Localizer,
    fluent::{FluentLanguageLoader, fluent_language_loader},
    unic_langid::LanguageIdentifier,
};
use rust_embed::RustEmbed;
use std::sync::LazyLock;

/// Applies the requested language(s) to requested translations from the `fl!()` macro.
pub fn init(requested_languages: &[LanguageIdentifier]) {
    if let Err(why) = localizer().select(requested_languages) {
        log::error!("error while loading fluent localizations: {why}");
    }
}

/// Languages to load for the configured UI language (`""` = the desktop's).
#[must_use]
pub fn requested(language: &str) -> Vec<LanguageIdentifier> {
    match language.parse() {
        Ok(id) if !language.is_empty() => vec![id],
        _ => i18n_embed::DesktopLanguageRequester::requested_languages(),
    }
}

// Get the `Localizer` to be used for localizing this library.
#[must_use]
pub fn localizer() -> Box<dyn Localizer> {
    Box::from(DefaultLocalizer::new(&*LANGUAGE_LOADER, &Localizations))
}

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub static LANGUAGE_LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader: FluentLanguageLoader = fluent_language_loader!();

    loader
        .load_fallback_language(&Localizations)
        .expect("Error while loading fallback language");

    loader
});

/// Request a localized string by ID from the i18n/ directory.
#[macro_export]
macro_rules! fl {
    ($message_id:literal) => {{
        i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id)
   }};

    ($message_id:literal, $($args:expr),*) => {{
        i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id, $($args), *)
   }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_language_wins() {
        assert_eq!(
            requested("ru"),
            vec!["ru".parse::<LanguageIdentifier>().unwrap()]
        );
    }

    #[test]
    fn empty_follows_the_desktop() {
        assert_eq!(
            requested(""),
            i18n_embed::DesktopLanguageRequester::requested_languages()
        );
    }
}
