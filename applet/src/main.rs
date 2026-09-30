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
    i18n::init(&i18n::requested(&daemon::load_config().language));
    if std::env::args().any(|a| a == "--settings") {
        // Only the minimum is binding for the compositor; the size is a wish.
        let window = cosmic::app::Settings::default()
            .size(cosmic::iced::Size::new(560.0, 720.0))
            .size_limits(
                cosmic::iced::Limits::NONE
                    .min_width(480.0)
                    .min_height(600.0),
            );
        return cosmic::app::run_single_instance::<settings::SettingsApp>(window, settings::Flags);
    }
    cosmic::applet::run::<panel::Applet>(())
}
