// SPDX-License-Identifier: GPL-3.0-only
//! The settings window: `cosmic-ext-lang-switch --settings`.
use crate::daemon::{self, Daemon};
use crate::fl;
use cosmic::app::{Core, Task};
use cosmic::iced::Length;
use cosmic::iced::keyboard::{self, Key, Modifiers, key::Named};
use cosmic::iced::{Event, Subscription, event};
use cosmic::widget::{self, settings};
use cosmic::{Application, ApplicationExt, Element};
use lsc::config::{APP_ID, Config};
use lsc::hotkey::{DEFAULT_PHRASE, DEFAULT_SELECTION, DEFAULT_WORD, Hotkey, Mods};

pub const SETTINGS_ID: &str = "io.github.shagovAlexei.cosmic-ext-lang-switch.settings";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Word,
    Phrase,
    Selection,
}

#[derive(Clone, Debug)]
pub enum Message {
    Config(Config),
    Daemon(Daemon),
    Record(Slot),
    Key(Key, Modifiers),
    SetAbortOnUnknown(bool),
    SetLanguage(usize),
    Reset,
    Done,
}

pub struct SettingsApp {
    core: Core,
    config: Config,
    daemon: Daemon,
    recording: Option<Slot>,
    hint: Option<String>,
    /// Dropdown labels, rebuilt when the UI language changes.
    languages: Vec<String>,
}

/// Config values behind the language dropdown, in its order.
const LANGUAGES: [&str; 3] = ["", "en", "ru"];

fn language_labels() -> Vec<String> {
    vec![fl!("language-system"), "English".into(), "Русский".into()]
}

impl SettingsApp {
    fn field(&mut self, slot: Slot) -> &mut String {
        match slot {
            Slot::Word => &mut self.config.hotkey_word,
            Slot::Phrase => &mut self.config.hotkey_phrase,
            Slot::Selection => &mut self.config.hotkey_selection,
        }
    }

    fn stop_recording(&mut self) -> Task<Message> {
        self.recording = None;
        Task::perform(daemon::set_paused(false), |()| {
            cosmic::action::app(Message::Done)
        })
    }

    fn hotkey_item(&self, title: String, slot: Slot) -> Element<'_, Message> {
        let text = if self.recording == Some(slot) {
            fl!("press-keys")
        } else {
            match slot {
                Slot::Word => self.config.hotkey_word.clone(),
                Slot::Phrase => self.config.hotkey_phrase.clone(),
                Slot::Selection => self.config.hotkey_selection.clone(),
            }
        };
        settings::item(
            title,
            widget::button::standard(text).on_press(Message::Record(slot)),
        )
        .into()
    }
}

fn key_event(event: Event, _: event::Status, _: cosmic::iced::window::Id) -> Option<Message> {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
            Some(Message::Key(key, modifiers))
        }
        _ => None,
    }
}

impl Application for SettingsApp {
    type Executor = cosmic::executor::multi::Executor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = SETTINGS_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, (): ()) -> (Self, Task<Message>) {
        let mut app = Self {
            core,
            config: daemon::load_config(),
            daemon: Daemon::default(),
            recording: None,
            hint: None,
            languages: language_labels(),
        };
        app.set_header_title(fl!("settings-title"));
        let task = match app.core.main_window_id() {
            Some(id) => app.set_window_title(fl!("settings-title"), id),
            None => Task::none(),
        };
        (app, task)
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            self.core
                .watch_config::<Config>(APP_ID)
                .map(|u| Message::Config(u.config)),
            daemon::subscription().map(Message::Daemon),
        ];
        if self.recording.is_some() {
            subs.push(event::listen_with(key_event));
        }
        Subscription::batch(subs)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Config(c) => {
                if c.language != self.config.language {
                    crate::i18n::init(&crate::i18n::requested(&c.language));
                    self.languages = language_labels();
                }
                self.config = c;
            }
            Message::Daemon(d) => self.daemon = d,
            Message::Record(slot) => {
                self.recording = Some(slot);
                self.hint = None;
                // The daemon swallows the current hotkeys; pause it so they reach us.
                return Task::perform(daemon::set_paused(true), |()| {
                    cosmic::action::app(Message::Done)
                });
            }
            Message::Key(key, m) => {
                let Some(slot) = self.recording else {
                    return Task::none();
                };
                let Key::Named(named) = key else {
                    self.hint = Some(fl!("hotkey-invalid"));
                    return Task::none();
                };
                match named {
                    Named::Escape => return self.stop_recording(),
                    Named::Shift
                    | Named::Control
                    | Named::Alt
                    | Named::Super
                    | Named::Meta
                    | Named::Hyper => {
                        return Task::none();
                    }
                    _ => {}
                }
                let mods = Mods {
                    shift: m.shift(),
                    ctrl: m.control(),
                    alt: m.alt(),
                    sup: m.logo(),
                };
                let Some(hotkey) = Hotkey::recorded(&format!("{named:?}"), mods) else {
                    self.hint = Some(fl!("hotkey-invalid"));
                    return Task::none();
                };
                let mut candidate = self.config.clone();
                *match slot {
                    Slot::Word => &mut candidate.hotkey_word,
                    Slot::Phrase => &mut candidate.hotkey_phrase,
                    Slot::Selection => &mut candidate.hotkey_selection,
                } = hotkey.to_string();
                if candidate.hotkeys().conflict() {
                    self.hint = Some(fl!("hotkey-taken"));
                    return Task::none();
                }
                *self.field(slot) = hotkey.to_string();
                daemon::save_config(&self.config);
                self.hint = None;
                return self.stop_recording();
            }
            Message::SetAbortOnUnknown(on) => {
                self.config.abort_on_unknown = on;
                daemon::save_config(&self.config);
            }
            Message::SetLanguage(i) => {
                // The config watch applies it (here and in the panel).
                self.config.language = LANGUAGES[i].into();
                daemon::save_config(&self.config);
                crate::i18n::init(&crate::i18n::requested(&self.config.language));
                self.languages = language_labels();
            }
            Message::Reset => {
                self.config.hotkey_word = DEFAULT_WORD.into();
                self.config.hotkey_phrase = DEFAULT_PHRASE.into();
                self.config.hotkey_selection = DEFAULT_SELECTION.into();
                daemon::save_config(&self.config);
                self.hint = None;
            }
            Message::Done => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let mut hotkeys = settings::section()
            .title(fl!("section-hotkeys"))
            .add(self.hotkey_item(fl!("hotkey-word"), Slot::Word))
            .add(self.hotkey_item(fl!("hotkey-phrase"), Slot::Phrase))
            .add(self.hotkey_item(fl!("hotkey-selection"), Slot::Selection));
        if let Some(h) = &self.hint {
            hotkeys = hotkeys.add(settings::item_row(vec![
                widget::text::caption(h.as_str()).into(),
            ]));
        }
        hotkeys = hotkeys.add(settings::item_row(vec![
            widget::Space::new().width(Length::Fill).into(),
            widget::button::text(fl!("reset-defaults"))
                .on_press(Message::Reset)
                .into(),
        ]));
        let behavior = settings::section()
            .title(fl!("section-behavior"))
            .add(settings::item(
                fl!("language"),
                widget::dropdown(
                    &self.languages,
                    LANGUAGES.iter().position(|&l| l == self.config.language),
                    Message::SetLanguage,
                ),
            ))
            .add(settings::item(
                fl!("abort-on-unknown"),
                widget::toggler(self.config.abort_on_unknown).on_toggle(Message::SetAbortOnUnknown),
            ));
        // Only shown when something is wrong; the popup has the on/off switch.
        let mut sections: Vec<Element<'_, Message>> = Vec::with_capacity(3);
        if let Some(w) = self.daemon.warning() {
            sections.push(widget::text::body(w).into());
        }
        sections.push(hotkeys.into());
        sections.push(behavior.into());
        widget::scrollable(settings::view_column(sections).padding([8, 16])).into()
    }
}
