//! Pure form logic: every control in the window maps to one [`PrefEdit`], and
//! [`apply_edit`] produces the next full preference set (clamped), which the
//! view emits as `AppCommand::SettingsChanged`.

use std::ops::RangeInclusive;
use std::path::Path;

use natikube_core::{Appearance, Clamp, Density, UserPrefs};

/// One user edit in the Settings window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefEdit {
    Appearance(Appearance),
    UiFontSize(u8),
    Zoom(u16),
    Density(Density),
    MonoFont(String),
    MonoFontSize(u8),
    TerminalFontSize(u8),
    Shell(String),
    TailLines(u32),
    LogTimestamps(bool),
    LogWrap(bool),
    MetricsPollSecs(u16),
    DefaultNamespace(String),
    CheckUpdatesOnStartup(bool),
}

fn blank_is_none(text: String) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Returns `prefs` with `edit` applied and every value back in range.
pub fn apply_edit(prefs: &UserPrefs, edit: PrefEdit) -> UserPrefs {
    let mut next = prefs.clone();
    match edit {
        PrefEdit::Appearance(value) => next.appearance = value,
        PrefEdit::UiFontSize(value) => next.ui.font_size = value,
        PrefEdit::Zoom(value) => next.ui.zoom_percent = value,
        PrefEdit::Density(value) => next.ui.density = value,
        PrefEdit::MonoFont(text) => next.ui.mono_font = blank_is_none(text),
        PrefEdit::MonoFontSize(value) => next.ui.mono_font_size = value,
        PrefEdit::TerminalFontSize(value) => next.terminal.font_size = value,
        PrefEdit::Shell(text) => next.terminal.shell = blank_is_none(text),
        PrefEdit::TailLines(value) => next.logs.tail_lines = value,
        PrefEdit::LogTimestamps(value) => next.logs.timestamps = value,
        PrefEdit::LogWrap(value) => next.logs.wrap = value,
        PrefEdit::MetricsPollSecs(value) => next.data.metrics_poll_secs = value,
        PrefEdit::DefaultNamespace(text) => next.data.default_namespace = blank_is_none(text),
        PrefEdit::CheckUpdatesOnStartup(value) => next.updates.check_on_startup = value,
    }
    next.clamp_in_place();
    next
}

/// Moves `current` by `delta` inside `range` (stepper buttons).
pub fn step<T>(current: T, delta: i64, range: &RangeInclusive<T>) -> T
where
    T: Copy + Into<i64> + TryFrom<i64>,
{
    let (lo, hi) = ((*range.start()).into(), (*range.end()).into());
    let value = (current.into().saturating_add(delta)).clamp(lo, hi);
    T::try_from(value).unwrap_or(current)
}

/// Parses a typed whole number and clamps it; `None` when it is not digits.
pub fn parse_number(text: &str, range: &RangeInclusive<u32>) -> Option<u32> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value = text.parse::<u64>().unwrap_or(u64::MAX);
    Some(value.clamp(u64::from(*range.start()), u64::from(*range.end())) as u32)
}

/// Shows a path with the home directory abbreviated to `~`.
pub fn display_path(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home
        && let Ok(rest) = path.strip_prefix(home)
    {
        return if rest.as_os_str().is_empty() {
            "~".to_string()
        } else {
            format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display())
        };
    }
    path.display().to_string()
}
