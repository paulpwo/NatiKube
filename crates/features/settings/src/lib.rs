//! F8 — Settings window (⌘,), available in every appearance.
//!
//! * `domain`: pure section model (settings stream, tests first).
//! * `ui`: [`SettingsView`] and [`open_settings_window`].
//!
//! Contract: the shell calls [`open_settings_window`] on
//! `AppCommand::OpenSettings` (or focuses the existing window), subscribes to
//! the returned view's `AppCommand` events and calls
//! [`SettingsView::set_settings`] whenever the shell's `Settings` change
//! behind the window's back (catalog reload, import, remove).

pub mod domain;
pub mod ui;

pub use domain::SettingsSection;
pub use ui::{ExtraPage, SettingsView, open_settings_window};
