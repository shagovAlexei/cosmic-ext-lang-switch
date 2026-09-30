// SPDX-License-Identifier: GPL-3.0-only
//! Pure logic shared by the daemon and the applet. No device or Wayland I/O here.

pub mod hotkey;
pub mod keys;
pub use hotkey::Scope;
pub mod config;
pub mod dbus;
pub mod engine;
