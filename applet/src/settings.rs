// SPDX-License-Identifier: GPL-3.0-only
//! The settings window: `cosmic-ext-lang-switch --settings`.
use crate::daemon::{self, Daemon};
use crate::fl;
use cosmic::app::{Core, Task};
use cosmic::iced::keyboard::{self, Key, Modifiers, key::Named};
use cosmic::iced::{Event, Subscription, event};
use cosmic::widget::{self, settings};
use cosmic::{Application, ApplicationExt, Element};
use lsc::config::{APP_ID, Config};
use lsc::hotkey::{DEFAULT_PHRASE, DEFAULT_SELECTION, DEFAULT_WORD, Hotkey, Mods};

/// Lets libcosmic keep one settings window: a second launch activates the first.
#[derive(Clone, Debug, Default)]
pub struct Flags;

impl cosmic::app::CosmicFlags for Flags {
    type SubCommand = String;
    type Args = Vec<String>;
}

/// No hyphens: single-instance mode turns this into a D-Bus name and object path.
pub const SETTINGS_ID: &str = "io.github.shagovAlexei.CosmicExtLangSwitch.Settings";

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
    Reset(Slot),
    Tab(widget::segmented_button::Entity),
    SetAuto(bool),
    NewApp(String),
    AddApp,
    RemoveApp(String),
    RemoveWord(String),
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
    tabs: widget::segmented_button::SingleSelectModel,
    /// The app id being typed into the "add excluded app" field.
    new_app: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    General,
    Auto,
}

/// Tab labels follow the UI language, so the model is rebuilt on a language change.
fn tabs(active: Page) -> widget::segmented_button::SingleSelectModel {
    let mut model = widget::segmented_button::ModelBuilder::default()
        .insert(|b| b.text(fl!("tab-general")).data(Page::General))
        .insert(|b| b.text(fl!("tab-auto")).data(Page::Auto))
        .build();
    let pos = u16::from(active == Page::Auto);
    model.activate_position(pos);
    model
}

/// Config values behind the language dropdown, in its order.
const LANGUAGES: [&str; 3] = ["", "en", "ru"];

fn language_labels() -> Vec<String> {
    vec![fl!("language-system"), "English".into(), "Русский".into()]
}

impl SettingsApp {
    /// Saves `value` as the hotkey for `slot` unless another action already uses it.
    fn assign(&mut self, slot: Slot, value: String) -> bool {
        let mut candidate = self.config.clone();
        *field(&mut candidate, slot) = value;
        if candidate.hotkeys().conflict() {
            self.hint = Some(fl!("hotkey-taken"));
            return false;
        }
        self.config = candidate;
        let value = field(&mut self.config, slot).clone();
        daemon::save(|h| cosmic::cosmic_config::ConfigSet::set(h, key(slot), value).map(|()| true));
        self.hint = None;
        true
    }

    fn stop_recording(&mut self) -> Task<Message> {
        self.recording = None;
        Task::perform(daemon::set_paused(false), |()| {
            cosmic::action::app(Message::Done)
        })
    }

    fn page(&self) -> Page {
        self.tabs
            .active_data::<Page>()
            .copied()
            .unwrap_or(Page::General)
    }

    /// A list row: the text and a remove button.
    fn removable<'a>(text: &'a str, on_remove: Message) -> Element<'a, Message> {
        settings::item_row(vec![
            widget::text::body(text)
                .width(cosmic::iced::Length::Fill)
                .into(),
            widget::button::icon(widget::icon::from_name("edit-delete-symbolic"))
                .on_press(on_remove)
                .into(),
        ])
        .into()
    }

    fn auto_page(&self) -> Vec<Element<'_, Message>> {
        let switch = settings::section().add(settings::item(
            fl!("auto-enabled"),
            widget::toggler(self.config.auto_enabled).on_toggle(Message::SetAuto),
        ));
        let mut apps = settings::section().title(fl!("auto-apps"));
        for app in &self.config.auto_excluded_apps {
            apps = apps.add(Self::removable(app, Message::RemoveApp(app.clone())));
        }
        apps = apps.add(settings::item_row(vec![
            widget::text_input(fl!("app-id-placeholder"), &self.new_app)
                .on_input(Message::NewApp)
                .on_submit(|_| Message::AddApp)
                .into(),
            widget::button::standard(fl!("add"))
                .on_press(Message::AddApp)
                .into(),
        ]));
        let mut words = settings::section().title(fl!("auto-words"));
        if self.config.auto_exceptions.is_empty() {
            words = words.add(settings::item_row(vec![
                widget::text::caption(fl!("auto-words-empty")).into(),
            ]));
        }
        for word in &self.config.auto_exceptions {
            words = words.add(Self::removable(word, Message::RemoveWord(word.clone())));
        }
        vec![switch.into(), apps.into(), words.into()]
    }

    fn hotkey_item(&self, title: String, slot: Slot) -> Element<'_, Message> {
        let current = match slot {
            Slot::Word => &self.config.hotkey_word,
            Slot::Phrase => &self.config.hotkey_phrase,
            Slot::Selection => &self.config.hotkey_selection,
        };
        let text = if self.recording == Some(slot) {
            fl!("press-keys")
        } else {
            current.clone()
        };
        let reset = widget::button::icon(widget::icon::from_name("edit-undo-symbolic"))
            .on_press_maybe((current != default(slot)).then_some(Message::Reset(slot)));
        let controls = widget::row::with_capacity(2)
            .spacing(8)
            .align_y(cosmic::iced::Alignment::Center)
            .push(widget::button::standard(text).on_press(Message::Record(slot)))
            .push(widget::tooltip(
                reset,
                widget::text::body(fl!("reset-default")),
                widget::tooltip::Position::Bottom,
            ));
        settings::item(title, controls).into()
    }
}

fn field(config: &mut Config, slot: Slot) -> &mut String {
    match slot {
        Slot::Word => &mut config.hotkey_word,
        Slot::Phrase => &mut config.hotkey_phrase,
        Slot::Selection => &mut config.hotkey_selection,
    }
}

/// The config entry holding `slot`'s hotkey.
fn key(slot: Slot) -> &'static str {
    match slot {
        Slot::Word => "hotkey_word",
        Slot::Phrase => "hotkey_phrase",
        Slot::Selection => "hotkey_selection",
    }
}

fn default(slot: Slot) -> &'static str {
    match slot {
        Slot::Word => DEFAULT_WORD,
        Slot::Phrase => DEFAULT_PHRASE,
        Slot::Selection => DEFAULT_SELECTION,
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
    type Flags = Flags;
    type Message = Message;
    const APP_ID: &'static str = SETTINGS_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _: Flags) -> (Self, Task<Message>) {
        let mut app = Self {
            core,
            config: daemon::load_config(),
            daemon: Daemon::default(),
            recording: None,
            hint: None,
            languages: language_labels(),
            tabs: tabs(Page::General),
            new_app: String::new(),
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
                    self.tabs = tabs(self.page());
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
                if !self.assign(slot, hotkey.to_string()) {
                    return Task::none();
                }
                return self.stop_recording();
            }
            Message::SetAbortOnUnknown(on) => {
                daemon::save(|h| self.config.set_abort_on_unknown(h, on));
            }
            Message::SetLanguage(i) => {
                // The config watch applies it (here and in the panel).
                daemon::save(|h| self.config.set_language(h, LANGUAGES[i].into()));
                crate::i18n::init(&crate::i18n::requested(&self.config.language));
                self.languages = language_labels();
                self.tabs = tabs(self.page());
            }
            Message::Tab(id) => self.tabs.activate(id),
            Message::SetAuto(on) => {
                daemon::save(|h| self.config.set_auto_enabled(h, on));
            }
            Message::NewApp(text) => self.new_app = text,
            Message::AddApp => {
                let app = self.new_app.trim().to_owned();
                if !app.is_empty() && !self.config.auto_excluded_apps.contains(&app) {
                    let mut apps = self.config.auto_excluded_apps.clone();
                    apps.push(app);
                    daemon::save(|h| self.config.set_auto_excluded_apps(h, apps));
                }
                self.new_app.clear();
            }
            // By value, not index: a config reload may have reordered the list.
            Message::RemoveApp(app) => {
                let mut apps = self.config.auto_excluded_apps.clone();
                apps.retain(|a| *a != app);
                daemon::save(|h| self.config.set_auto_excluded_apps(h, apps));
            }
            Message::RemoveWord(word) => {
                let mut words = self.config.auto_exceptions.clone();
                words.retain(|w| *w != word);
                daemon::save(|h| self.config.set_auto_exceptions(h, words));
            }
            Message::Reset(slot) => {
                self.assign(slot, default(slot).into());
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
        // Green dot when the service works, red with the reason otherwise.
        let theme = cosmic::theme::active();
        let (text, color) = match self.daemon.warning() {
            None => (fl!("daemon-ok"), theme.cosmic().success_color()),
            Some(w) => (w, theme.cosmic().destructive_color()),
        };
        let status = settings::section()
            .title(fl!("section-status"))
            .add(settings::item_row(vec![
                widget::text::body("●")
                    .class(cosmic::theme::Text::Color(color.into()))
                    .into(),
                widget::text::body(text).into(),
            ]));
        let mut sections: Vec<Element<'_, Message>> = vec![
            widget::segmented_control::horizontal(&self.tabs)
                .on_activate(Message::Tab)
                .into(),
        ];
        match self.page() {
            Page::General => sections.extend([hotkeys.into(), behavior.into(), status.into()]),
            Page::Auto => sections.extend(self.auto_page()),
        }
        widget::scrollable(settings::view_column(sections).padding([8, 16])).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_id_is_a_valid_dbus_name_and_path() {
        // libcosmic's single-instance mode serves D-Bus at a path built from the
        // app id and exits the process if that fails (hyphens are not allowed).
        let path = format!("/{}", SETTINGS_ID.replace('.', "/"));
        assert!(
            zbus::zvariant::ObjectPath::try_from(path.as_str()).is_ok(),
            "{path}"
        );
        assert!(zbus::names::WellKnownName::try_from(SETTINGS_ID).is_ok());
    }
}
