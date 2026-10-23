//! User preferences edited in the Settings window. Every value is plain data
//! (integers, bools, strings) so [`Settings`](crate::Settings) stays `Eq`.
//! Deserialization is tolerant: a wrong-typed field falls back to its default
//! and out-of-range numbers are clamped, so a hand-edited or future file never
//! gets discarded.

use serde::{Deserialize, Deserializer, Serialize, de::IgnoredAny};

use crate::appearance::Appearance;

/// Deserializes `T`, or `T::default()` when the value has the wrong shape.
pub(crate) fn lenient<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Maybe<T> {
        Value(T),
        Other(IgnoredAny),
    }
    Ok(match Maybe::<T>::deserialize(deserializer)? {
        Maybe::Value(value) => value,
        Maybe::Other(_) => T::default(),
    })
}

/// Brings a value back into its valid range.
pub trait Clamp {
    fn clamp_in_place(&mut self);
}

/// Row/control spacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

impl Density {
    pub fn label(self) -> &'static str {
        match self {
            Self::Comfortable => "Comfortable",
            Self::Compact => "Compact",
        }
    }

    pub fn all() -> [Density; 2] {
        [Self::Comfortable, Self::Compact]
    }
}

/// Appearance section: sizes and fonts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "UiPrefsRaw")]
pub struct UiPrefs {
    /// Base UI font size in px (10..=20, default 13; 13 is scale 1.0).
    pub font_size: u8,
    /// Whole-window zoom in percent (75..=150, default 100).
    pub zoom_percent: u16,
    pub density: Density,
    /// Monospace family; `None` = the system monospace font.
    pub mono_font: Option<String>,
    /// Monospace size in px (8..=20, default 12).
    pub mono_font_size: u8,
}

impl UiPrefs {
    pub const FONT_SIZE: std::ops::RangeInclusive<u8> = 10..=20;
    pub const ZOOM_PERCENT: std::ops::RangeInclusive<u16> = 75..=150;
    pub const MONO_FONT_SIZE: std::ops::RangeInclusive<u8> = 8..=20;
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            font_size: 13,
            zoom_percent: 100,
            density: Density::Comfortable,
            mono_font: None,
            mono_font_size: 12,
        }
    }
}

/// Tolerant shadow of [`UiPrefs`]: bad fields become `None` (= default).
#[derive(Deserialize, Default)]
#[serde(default)]
struct UiPrefsRaw {
    #[serde(deserialize_with = "lenient")]
    font_size: Option<u8>,
    #[serde(deserialize_with = "lenient")]
    zoom_percent: Option<u16>,
    #[serde(deserialize_with = "lenient")]
    density: Option<Density>,
    #[serde(deserialize_with = "lenient")]
    mono_font: Option<String>,
    #[serde(deserialize_with = "lenient")]
    mono_font_size: Option<u8>,
}

impl From<UiPrefsRaw> for UiPrefs {
    fn from(raw: UiPrefsRaw) -> Self {
        let d = Self::default();
        let mut value = Self {
            font_size: raw.font_size.unwrap_or(d.font_size),
            zoom_percent: raw.zoom_percent.unwrap_or(d.zoom_percent),
            density: raw.density.unwrap_or(d.density),
            mono_font: raw.mono_font,
            mono_font_size: raw.mono_font_size.unwrap_or(d.mono_font_size),
        };
        value.clamp_in_place();
        value
    }
}

impl Clamp for UiPrefs {
    fn clamp_in_place(&mut self) {
        self.font_size = self
            .font_size
            .clamp(*Self::FONT_SIZE.start(), *Self::FONT_SIZE.end());
        self.zoom_percent = self
            .zoom_percent
            .clamp(*Self::ZOOM_PERCENT.start(), *Self::ZOOM_PERCENT.end());
        self.mono_font_size = self
            .mono_font_size
            .clamp(*Self::MONO_FONT_SIZE.start(), *Self::MONO_FONT_SIZE.end());
        normalize_name(&mut self.mono_font);
    }
}

/// Terminal section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "TerminalPrefsRaw")]
pub struct TerminalPrefs {
    /// Terminal font size in px (8..=24, default 12).
    pub font_size: u8,
    /// Shell program for local terminals; `None` = the platform default
    /// (`$SHELL` else `/bin/zsh`; `pwsh.exe`, `powershell.exe`, `cmd.exe` on Windows).
    pub shell: Option<String>,
}

impl TerminalPrefs {
    pub const FONT_SIZE: std::ops::RangeInclusive<u8> = 8..=24;
}

impl Default for TerminalPrefs {
    fn default() -> Self {
        Self {
            font_size: 12,
            shell: None,
        }
    }
}

/// Tolerant shadow of [`TerminalPrefs`]: bad fields become `None` (= default).
#[derive(Deserialize, Default)]
#[serde(default)]
struct TerminalPrefsRaw {
    #[serde(deserialize_with = "lenient")]
    font_size: Option<u8>,
    #[serde(deserialize_with = "lenient")]
    shell: Option<String>,
}

impl From<TerminalPrefsRaw> for TerminalPrefs {
    fn from(raw: TerminalPrefsRaw) -> Self {
        let d = Self::default();
        let mut value = Self {
            font_size: raw.font_size.unwrap_or(d.font_size),
            shell: raw.shell,
        };
        value.clamp_in_place();
        value
    }
}

impl Clamp for TerminalPrefs {
    fn clamp_in_place(&mut self) {
        self.font_size = self
            .font_size
            .clamp(*Self::FONT_SIZE.start(), *Self::FONT_SIZE.end());
        normalize_name(&mut self.shell);
    }
}

/// Logs section (defaults for newly opened log tabs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "LogPrefsRaw")]
pub struct LogPrefs {
    /// Lines requested on open (10..=100_000, default 1000).
    pub tail_lines: u32,
    /// Show timestamps by default.
    pub timestamps: bool,
    /// Wrap long lines by default.
    pub wrap: bool,
}

impl LogPrefs {
    pub const TAIL_LINES: std::ops::RangeInclusive<u32> = 10..=100_000;
}

impl Default for LogPrefs {
    fn default() -> Self {
        Self {
            tail_lines: 1000,
            timestamps: false,
            wrap: false,
        }
    }
}

/// Tolerant shadow of [`LogPrefs`]: bad fields become `None` (= default).
#[derive(Deserialize, Default)]
#[serde(default)]
struct LogPrefsRaw {
    #[serde(deserialize_with = "lenient")]
    tail_lines: Option<u32>,
    #[serde(deserialize_with = "lenient")]
    timestamps: Option<bool>,
    #[serde(deserialize_with = "lenient")]
    wrap: Option<bool>,
}

impl From<LogPrefsRaw> for LogPrefs {
    fn from(raw: LogPrefsRaw) -> Self {
        let d = Self::default();
        let mut value = Self {
            tail_lines: raw.tail_lines.unwrap_or(d.tail_lines),
            timestamps: raw.timestamps.unwrap_or(d.timestamps),
            wrap: raw.wrap.unwrap_or(d.wrap),
        };
        value.clamp_in_place();
        value
    }
}

impl Clamp for LogPrefs {
    fn clamp_in_place(&mut self) {
        self.tail_lines = self
            .tail_lines
            .clamp(*Self::TAIL_LINES.start(), *Self::TAIL_LINES.end());
    }
}

/// Data section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "DataPrefsRaw")]
pub struct DataPrefs {
    /// `metrics.k8s.io` poll interval in seconds (5..=300, default 15).
    pub metrics_poll_secs: u16,
    /// Namespace selected when a cluster has no saved filter; `None` = all.
    pub default_namespace: Option<String>,
}

impl DataPrefs {
    pub const METRICS_POLL_SECS: std::ops::RangeInclusive<u16> = 5..=300;
}

impl Default for DataPrefs {
    fn default() -> Self {
        Self {
            metrics_poll_secs: 15,
            default_namespace: None,
        }
    }
}

/// Tolerant shadow of [`DataPrefs`]: bad fields become `None` (= default).
#[derive(Deserialize, Default)]
#[serde(default)]
struct DataPrefsRaw {
    #[serde(deserialize_with = "lenient")]
    metrics_poll_secs: Option<u16>,
    #[serde(deserialize_with = "lenient")]
    default_namespace: Option<String>,
}

impl From<DataPrefsRaw> for DataPrefs {
    fn from(raw: DataPrefsRaw) -> Self {
        let d = Self::default();
        let mut value = Self {
            metrics_poll_secs: raw.metrics_poll_secs.unwrap_or(d.metrics_poll_secs),
            default_namespace: raw.default_namespace,
        };
        value.clamp_in_place();
        value
    }
}

impl Clamp for DataPrefs {
    fn clamp_in_place(&mut self) {
        self.metrics_poll_secs = self.metrics_poll_secs.clamp(
            *Self::METRICS_POLL_SECS.start(),
            *Self::METRICS_POLL_SECS.end(),
        );
        normalize_name(&mut self.default_namespace);
    }
}

/// Blank strings mean "unset".
fn normalize_name(value: &mut Option<String>) {
    if let Some(text) = value {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            *value = None;
        } else if trimmed.len() != text.len() {
            *value = Some(trimmed.to_string());
        }
    }
}

/// Everything the Settings window edits, as one value. A snapshot of the
/// preference part of [`Settings`](crate::Settings); it is never serialized
/// itself. Carried by `AppCommand::SettingsChanged`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserPrefs {
    pub appearance: Appearance,
    pub ui: UiPrefs,
    pub terminal: TerminalPrefs,
    pub logs: LogPrefs,
    pub data: DataPrefs,
}

impl Clamp for UserPrefs {
    fn clamp_in_place(&mut self) {
        self.ui.clamp_in_place();
        self.terminal.clamp_in_place();
        self.logs.clamp_in_place();
        self.data.clamp_in_place();
    }
}
