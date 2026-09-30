// SPDX-License-Identifier: GPL-3.0-only
use crate::fl;
use cosmic::app::{Core, Task};
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::futures::{SinkExt, StreamExt, channel::mpsc::Sender};
use cosmic::iced::platform_specific::shell::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{Subscription, stream, window::Id};
use cosmic::widget::{self, settings};
use cosmic::{Application, Element};
use lsc::config::{APP_ID, Config};
use lsc::dbus::LangSwitchProxy;
use lsc::hotkey::Hotkey;

#[derive(Clone, Debug, Default)]
pub struct Daemon {
    pub layouts: Vec<String>,
    pub current: u32,
    /// Empty when the daemon isn't on the bus.
    pub status: String,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Config(Config),
    Daemon(Daemon),
    SetLayout(u32),
    SetEnabled(bool),
    WordInput(String),
    PhraseInput(String),
}

pub struct Applet {
    core: Core,
    popup: Option<Id>,
    config: Config,
    daemon: Daemon,
    word: String,
    phrase: String,
    label: String,
}

impl Applet {
    fn save(&self) {
        if let Ok(h) = cosmic_config::Config::new(APP_ID, Config::VERSION)
            && let Err(e) = self.config.write_entry(&h)
        {
            log::error!("config write: {e:?}");
        }
    }

    /// Panel text; a trailing dot marks "correction off". Kept as a field because `text_button` borrows it.
    fn refresh_label(&mut self) {
        let name = self
            .daemon
            .layouts
            .get(self.daemon.current as usize)
            .map_or("??", String::as_str);
        self.label = if self.config.enabled {
            name.to_owned()
        } else {
            format!("{name}·")
        };
    }

    /// Validates on each keystroke; only a parseable hotkey is written.
    fn hotkey_row<'a>(
        &'a self,
        title: String,
        value: &'a str,
        on_input: fn(String) -> Message,
    ) -> Element<'a, Message> {
        let mut input = widget::text_input("Insert", value).on_input(on_input);
        if value.parse::<Hotkey>().is_err() {
            input = input
                .helper_text(fl!("hotkey-invalid"))
                .error(fl!("hotkey-invalid"));
        }
        settings::item(title, input).into()
    }
}

/// Sends daemon state on every property change; returns only on a D-Bus error.
async fn watch_daemon(out: &mut Sender<Message>) -> zbus::Result<()> {
    let conn = zbus::Connection::session().await?;
    let p = LangSwitchProxy::new(&conn).await?;
    let mut cur = p.receive_current_layout_changed().await;
    let mut lay = p.receive_layouts_changed().await;
    let mut owner = p.inner().receive_owner_changed().await?;
    loop {
        let d = Daemon {
            layouts: p.layouts().await?,
            current: p.current_layout().await?,
            status: p.status().await?,
        };
        let _ = out.send(Message::Daemon(d)).await;
        tokio::select! {
            _ = cur.next() => {}
            _ = lay.next() => {}
            o = owner.next() => if o.is_none_or(|o| o.is_none()) { return Ok(()) },
        }
    }
}

/// Streams daemon state; reconnects every 2 s while the daemon is absent.
fn daemon_subscription() -> Subscription<Message> {
    Subscription::run(|| {
        stream::channel(16, |mut out: Sender<Message>| async move {
            loop {
                let _ = watch_daemon(&mut out).await;
                let _ = out.send(Message::Daemon(Daemon::default())).await;
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        })
    })
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

    fn init(core: Core, _: ()) -> (Self, Task<Message>) {
        let config = cosmic_config::Config::new(APP_ID, Config::VERSION)
            .map(|h| Config::get_entry(&h).unwrap_or_else(|(_, c)| c))
            .unwrap_or_default();
        let (word, phrase) = (config.hotkey_word.clone(), config.hotkey_phrase.clone());
        let mut applet = Self {
            core,
            popup: None,
            config,
            daemon: Daemon::default(),
            word,
            phrase,
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
            daemon_subscription(),
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
                return Task::perform(
                    async move {
                        let conn = zbus::Connection::session().await?;
                        LangSwitchProxy::new(&conn).await?.set_layout(i).await
                    },
                    |r: zbus::Result<()>| {
                        if let Err(e) = r {
                            log::error!("set_layout: {e}");
                        }
                        cosmic::action::none()
                    },
                );
            }
            Message::SetEnabled(on) => {
                self.config.enabled = on;
                self.save();
            }
            Message::WordInput(s) => {
                if s.parse::<Hotkey>().is_ok() {
                    self.config.hotkey_word = s.clone();
                    self.save();
                }
                self.word = s;
            }
            Message::PhraseInput(s) => {
                if s.parse::<Hotkey>().is_ok() {
                    self.config.hotkey_phrase = s.clone();
                    self.save();
                }
                self.phrase = s;
            }
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
        let warning = match self.daemon.status.as_str() {
            "ok" => None,
            "" => Some(fl!("daemon-missing")),
            "no-input-access" => Some(fl!("no-input-access")),
            _ => Some(fl!("no-layout-protocol")),
        };
        let mut layouts = widget::Column::new();
        for (i, name) in self.daemon.layouts.iter().enumerate() {
            let i = i as u32;
            layouts = layouts.push(widget::radio(
                name.as_str(),
                i,
                Some(self.daemon.current),
                Message::SetLayout,
            ));
        }
        let mut col = widget::Column::new().spacing(8).padding(12);
        if let Some(w) = warning {
            col = col.push(widget::text::body(w));
        }
        col = col
            .push(settings::item(
                fl!("enabled"),
                widget::toggler(self.config.enabled).on_toggle(Message::SetEnabled),
            ))
            .push(layouts)
            .push(self.hotkey_row(fl!("hotkey-word"), &self.word, Message::WordInput))
            .push(self.hotkey_row(fl!("hotkey-phrase"), &self.phrase, Message::PhraseInput));
        self.core.applet.popup_container(col).into()
    }
}
