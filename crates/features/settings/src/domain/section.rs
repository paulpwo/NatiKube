/// A page of the Settings window, in sidebar order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SettingsSection {
    #[default]
    Appearance,
    Clusters,
    Terminal,
    Logs,
    Data,
    Updates,
}

impl SettingsSection {
    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Clusters => "Clusters",
            Self::Terminal => "Terminal",
            Self::Logs => "Logs",
            Self::Data => "Data",
            Self::Updates => "Updates",
        }
    }

    pub fn all() -> [SettingsSection; 6] {
        [
            Self::Appearance,
            Self::Clusters,
            Self::Terminal,
            Self::Logs,
            Self::Data,
            Self::Updates,
        ]
    }
}
