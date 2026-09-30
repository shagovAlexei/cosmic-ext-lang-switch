// SPDX-License-Identifier: GPL-3.0-only
//! The panel button and its popup, laid out like COSMIC's input-sources applet.
use crate::daemon::{self, Daemon};
use crate::fl;
use cosmic::app::{Core, Task};
use cosmic::applet::{menu_button, padded_control};
use cosmic::iced::platform_specific::shell::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{Alignment, Length, Subscription, window::Id};
use cosmic::widget::{self, divider};
use cosmic::{Application, Element};
use lsc::config::{APP_ID, Config};

#[derive(Clone, Copy, Debug)]
pub enum Open {
    Keyboard,
    Region,
    Settings,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Config(Config),
    Daemon(Daemon),
    SetLayout(u32),
    SetEnabled(bool),
    SetAuto(bool),
    ActiveApp(String),
    Open(Open),
    Done,
}

pub struct Applet {
    core: Core,
    popup: Option<Id>,
    config: Config,
    daemon: Daemon,
    /// Focused window's app id, forwarded to the daemon (it can't see it itself).
    active_app: Option<String>,
    /// Kept as a field because `text_button` borrows it.
    label: String,
}

impl Applet {
    fn refresh_label(&mut self) {
        self.daemon.label().clone_into(&mut self.label);
    }
}

fn separator<'a>() -> Element<'a, Message> {
    let s = cosmic::theme::active().cosmic().spacing;
    padded_control(divider::horizontal::default())
        .padding([s.space_xxs, s.space_s])
        .into()
}

impl Application for Applet {
    type Executor = cosmic::executor::multi::Executor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, (): ()) -> (Self, Task<Message>) {
        let mut applet = Self {
            core,
            popup: None,
            config: daemon::load_config(),
            daemon: Daemon::default(),
            active_app: None,
            label: String::new(),
        };
        applet.refresh_label();
        (applet, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            self.core
                .watch_config::<Config>(APP_ID)
                .map(|u| Message::Config(u.config)),
            daemon::subscription().map(Message::Daemon),
            crate::toplevel::subscription().map(Message::ActiveApp),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                }
                let id = Id::unique();
                self.popup = Some(id);
                let settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap(),
                    id,
                    None,
                    None,
                    None,
                );
                return get_popup(settings);
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::Config(c) => {
                if c.language != self.config.language {
                    crate::i18n::init(&crate::i18n::requested(&c.language));
                }
                self.config = c;
            }
            Message::Daemon(d) => {
                // A (re)started daemon doesn't know the focused app yet.
                let came_up = self.daemon.status.is_empty() && !d.status.is_empty();
                self.daemon = d;
                if came_up && let Some(app) = self.active_app.clone() {
                    return Task::perform(daemon::set_active_app(app), |()| {
                        cosmic::action::app(Message::Done)
                    });
                }
            }
            // Already sent to the daemon by the subscription; kept for a daemon restart.
            Message::ActiveApp(app) => self.active_app = Some(app),
            Message::SetAuto(on) => {
                self.config.auto_enabled = on;
                daemon::save_config(&self.config);
            }
            Message::SetLayout(i) => {
                return Task::perform(daemon::set_layout(i), |()| {
                    cosmic::action::app(Message::Done)
                });
            }
            Message::SetEnabled(on) => {
                self.config.enabled = on;
                daemon::save_config(&self.config);
            }
            Message::Open(what) => {
                match what {
                    Open::Keyboard => daemon::launch("cosmic-settings", "keyboard"),
                    Open::Region => daemon::launch("cosmic-settings", "region-language"),
                    Open::Settings => {
                        let me = std::env::current_exe()
                            .unwrap_or_else(|_| "cosmic-ext-lang-switch".into());
                        daemon::launch(me, "--settings");
                    }
                }
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                }
            }
            Message::Done => {}
        }
        self.refresh_label();
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let mut text = widget::text(self.label.as_str());
        if !self.config.enabled {
            // Correction off: the label is dimmed.
            let mut dim: cosmic::iced::Color =
                cosmic::theme::active().cosmic().on_bg_color().into();
            dim.a = 0.4;
            text = text.class(cosmic::theme::Text::Color(dim));
        }
        self.core
            .applet
            .autosize_window(self.core.applet.text_button(text, Message::TogglePopup))
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        let mut list = widget::column::with_capacity(12).padding([8, 0]);
        if let Some(w) = self.daemon.warning() {
            list = list
                .push(padded_control(widget::text::body(w)))
                .push(separator());
        }
        for (i, (_, code, description)) in self.daemon.layouts.iter().enumerate() {
            let i = u32::try_from(i).unwrap_or(u32::MAX);
            let mut title = widget::text::body(description.as_str());
            if i == self.daemon.current {
                title = title.font(cosmic::font::bold());
            }
            let row = widget::column::with_capacity(2)
                .push(title)
                .push(widget::text::caption(code.as_str()));
            list = list.push(menu_button(row).on_press(Message::SetLayout(i)));
        }
        if !self.daemon.layouts.is_empty() {
            list = list.push(separator());
        }
        let switch = |label: String, on: bool, msg: fn(bool) -> Message| {
            padded_control(
                widget::row::with_capacity(2)
                    .align_y(Alignment::Center)
                    .push(widget::text::body(label).width(Length::Fill))
                    .push(widget::toggler(on).on_toggle(msg)),
            )
        };
        let button = |icon: &'static str, tip: String, open: Open| {
            widget::tooltip(
                widget::button::icon(widget::icon::from_name(icon)).on_press(Message::Open(open)),
                widget::text::body(tip),
                widget::tooltip::Position::Top,
            )
        };
        let menu = widget::row::with_capacity(5)
            .spacing(cosmic::theme::active().cosmic().spacing.space_l)
            .push(widget::Space::new().width(Length::Fill))
            .push(button(
                "input-keyboard-symbolic",
                fl!("keyboard-settings"),
                Open::Keyboard,
            ))
            .push(button(
                "preferences-desktop-locale-symbolic",
                fl!("region-settings"),
                Open::Region,
            ))
            .push(button(
                "emblem-system-symbolic",
                fl!("app-settings"),
                Open::Settings,
            ))
            .push(widget::Space::new().width(Length::Fill));
        list = list
            .push(switch(
                fl!("enabled"),
                self.config.enabled,
                Message::SetEnabled,
            ))
            .push(switch(
                fl!("auto-enabled"),
                self.config.auto_enabled,
                Message::SetAuto,
            ))
            .push(separator())
            .push(padded_control(menu));
        self.core.applet.popup_container(list).into()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}
