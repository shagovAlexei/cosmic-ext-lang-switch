// SPDX-License-Identifier: GPL-3.0-only
pub const BUS_NAME: &str = "io.github.shagovAlexei.CosmicExtLangSwitch";
pub const PATH: &str = "/io/github/shagovAlexei/CosmicExtLangSwitch";

#[zbus::proxy(
    interface = "io.github.shagovAlexei.CosmicExtLangSwitch",
    default_service = "io.github.shagovAlexei.CosmicExtLangSwitch",
    default_path = "/io/github/shagovAlexei/CosmicExtLangSwitch"
)]
pub trait LangSwitch {
    fn set_layout(&self, index: u32) -> zbus::Result<()>;
    /// Forward every key and fix nothing (while a hotkey is being recorded).
    /// The daemon lifts the pause by itself after 30 s.
    fn set_paused(&self, paused: bool) -> zbus::Result<()>;
    /// The focused window's app id (sent by the applet: only panel applets may see it).
    fn set_active_app(&self, app_id: &str) -> zbus::Result<()>;
    /// Per layout group: (panel label, xkb layout code, description), e.g. ("RU", "by", "Russian (Belarus)").
    #[zbus(property)]
    fn layouts(&self) -> zbus::Result<Vec<(String, String, String)>>;
    #[zbus(property)]
    fn current_layout(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn status(&self) -> zbus::Result<String>;
}
