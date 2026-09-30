// SPDX-License-Identifier: GPL-3.0-only
mod daemon;
mod i18n;
mod panel;
mod settings;

fn main() -> cosmic::iced::Result {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .with_module_level("zbus", log::LevelFilter::Warn)
        .with_module_level("tracing", log::LevelFilter::Warn)
        .env()
        .init()
        .ok();
    i18n::init(&i18n_embed::DesktopLanguageRequester::requested_languages());
    if std::env::args().any(|a| a == "--settings") {
        let window = cosmic::app::Settings::default().size(cosmic::iced::Size::new(560.0, 560.0));
        return cosmic::app::run::<settings::SettingsApp>(window, ());
    }
    cosmic::applet::run::<panel::Applet>(())
}
