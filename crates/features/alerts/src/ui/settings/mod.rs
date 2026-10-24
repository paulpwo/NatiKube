//! Alerts section of the Settings window (task A5).
//!
//! [`AlertsSettingsSection`] is a self-contained view the settings window
//! embeds as a child. It edits a copy of [`AlertsConfig`] and reports every
//! change as an [`AlertsSettingsEvent`]; it never touches `settings.json` or
//! the history. The integrator maps events to `AppCommand`s.

mod edit;
mod view;

pub use edit::{AlertsEdit, apply_edit, format_duration, parse_list};
pub use view::{AlertClusterEntry, AlertsSettingsEvent, AlertsSettingsSection};
