//! Alerts UI: the header bell with the history panel (task A4), the in-app
//! toasts and the Settings section (task A5).

use gpui_kit::Hsla;
use natikube_ui_kit::Palette;

use crate::domain::Severity;

pub mod bell;
pub mod settings;
pub mod toast;

/// Dot and accent color of a severity, shared by the bell, the toasts and
/// Settings.
pub(crate) fn severity_color(severity: Severity, p: Palette) -> Hsla {
    match severity {
        Severity::Info => p.accent,
        Severity::Warning => p.warning,
        Severity::Critical => p.error,
    }
}
