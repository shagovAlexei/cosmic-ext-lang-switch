// SPDX-License-Identifier: GPL-3.0-only
mod app;
mod i18n;

fn main() -> cosmic::iced::Result {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .with_module_level("zbus", log::LevelFilter::Warn)
        .with_module_level("tracing", log::LevelFilter::Warn)
        .env()
        .init()
        .ok();
    i18n::init(&i18n_embed::DesktopLanguageRequester::requested_languages());
    cosmic::applet::run::<app::Applet>(())
}
