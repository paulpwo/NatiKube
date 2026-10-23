use serde::{Deserialize, Serialize};

/// Whole-window visual style, switchable at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    /// The Freelens-like UI NatiKube shipped with. Never changes.
    #[default]
    Standard,
    /// Design A: top bar, grouped sidebar, status pills, usage bars.
    Command,
    /// Design B: macOS source-list look, zebra rows, inspector.
    Native,
    /// Design C: icon rail, health cards, workloads grouped by namespace.
    Observatory,
}

impl Appearance {
    /// Menu / palette label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Command => "Command",
            Self::Native => "macOS Native",
            Self::Observatory => "Observatory",
        }
    }

    /// Lowercase identifier used by `--appearance` and in settings.json.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Command => "command",
            Self::Native => "native",
            Self::Observatory => "observatory",
        }
    }

    /// Every appearance, in menu order.
    pub fn all() -> [Appearance; 4] {
        [
            Self::Standard,
            Self::Command,
            Self::Native,
            Self::Observatory,
        ]
    }

    /// Parses a slug (case-insensitive); accepts `macos` as an alias of `native`.
    pub fn from_slug(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "standard" => Some(Self::Standard),
            "command" => Some(Self::Command),
            "native" | "macos" => Some(Self::Native),
            "observatory" => Some(Self::Observatory),
            _ => None,
        }
    }
}
