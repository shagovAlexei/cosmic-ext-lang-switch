// SPDX-License-Identifier: GPL-3.0-only
mod app;
mod i18n;

fn main() -> cosmic::iced::Result {
    simple_logger::init_with_env().ok();
    i18n::init(&i18n_embed::DesktopLanguageRequester::requested_languages());
    cosmic::applet::run::<app::Applet>(())
}
