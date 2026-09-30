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
    #[zbus(property)]
    fn layouts(&self) -> zbus::Result<Vec<String>>;
    #[zbus(property)]
    fn current_layout(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn status(&self) -> zbus::Result<String>;
}
