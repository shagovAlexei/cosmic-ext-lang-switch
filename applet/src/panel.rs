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
    Open(Open),
    Done,
}

pub struct Applet {
    core: Core,
    popup: Option<Id>,
    config: Config,
    daemon: Daemon,
    /// Kept as a field because `text_button` borrows it.
    label: String,
}

impl Applet {
    /// A trailing dot marks "correction off".
    fn refresh_label(&mut self) {
        let name = self.daemon.label();
        self.label = if self.config.enabled {
            name.to_owned()
        } else {
            format!("{name}·")
        };
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
            Message::Config(c) => self.config = c,
            Message::Daemon(d) => self.daemon = d,
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
        self.core
            .applet
            .autosize_window(
                self.core
                    .applet
                    .text_button(self.label.as_str(), Message::TogglePopup),
            )
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
        let toggle = widget::row::with_capacity(2)
            .align_y(Alignment::Center)
            .push(widget::text::body(fl!("enabled")).width(Length::Fill))
            .push(widget::toggler(self.config.enabled).on_toggle(Message::SetEnabled));
        list = list
            .push(padded_control(toggle))
            .push(separator())
            .push(
                menu_button(widget::text::body(fl!("keyboard-settings")))
                    .on_press(Message::Open(Open::Keyboard)),
            )
            .push(
                menu_button(widget::text::body(fl!("region-settings")))
                    .on_press(Message::Open(Open::Region)),
            )
            .push(
                menu_button(widget::text::body(fl!("app-settings")))
                    .on_press(Message::Open(Open::Settings)),
            );
        self.core.applet.popup_container(list).into()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}
