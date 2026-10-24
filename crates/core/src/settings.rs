use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::alerts::AlertsConfig;
use crate::appearance::Appearance;
use crate::cluster::ClusterId;
use crate::navigation::ViewId;
use crate::prefs::{
    Clamp, DataPrefs, LogPrefs, TerminalPrefs, UiPrefs, UpdatePrefs, UserPrefs, lenient,
};

/// Which namespaces a table shows.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NamespaceFilter {
    #[default]
    All,
    /// Only these namespaces. An empty set behaves like [`NamespaceFilter::All`].
    Only(BTreeSet<String>),
}

impl NamespaceFilter {
    pub fn matches(&self, namespace: &str) -> bool {
        match self {
            Self::All => true,
            Self::Only(set) => set.is_empty() || set.contains(namespace),
        }
    }

    /// Text for the namespace dropdown.
    pub fn label(&self) -> String {
        match self {
            Self::Only(set) if set.len() == 1 || set.len() == 2 => {
                set.iter().cloned().collect::<Vec<_>>().join(", ")
            }
            Self::Only(set) if set.len() > 2 => format!("{} namespaces", set.len()),
            _ => "All namespaces".to_string(),
        }
    }
}

/// State remembered per cluster.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClusterPrefs {
    #[serde(deserialize_with = "lenient")]
    pub namespaces: NamespaceFilter,
    #[serde(deserialize_with = "lenient")]
    pub last_view: Option<ViewId>,
}

/// Deserializes the per-cluster map entry by entry: an entry of the wrong
/// shape is skipped instead of dropping every other cluster's state, and a
/// non-map value yields an empty map.
fn tolerant_clusters<'de, D>(deserializer: D) -> Result<BTreeMap<String, ClusterPrefs>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Valid(ClusterPrefs),
        Invalid(serde::de::IgnoredAny),
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Maybe {
        Map(BTreeMap<String, Entry>),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Maybe::deserialize(deserializer)? {
        Maybe::Map(map) => map
            .into_iter()
            .filter_map(|(key, entry)| match entry {
                Entry::Valid(prefs) => Some((key, prefs)),
                Entry::Invalid(_) => None,
            })
            .collect(),
        Maybe::Other(_) => BTreeMap::new(),
    })
}

/// The persisted application settings (a JSON file owned by the clusters
/// feature). Unknown fields are ignored and missing ones default, so the file
/// stays forward and backward compatible.
///
/// Parsing is tolerant per field: a value of the wrong type or an unknown
/// appearance falls back to that field's default instead of failing the whole
/// file, and numeric preferences are clamped to their valid range.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Extra kubeconfig files or folders beyond `~/.kube/config`/`$KUBECONFIG`.
    #[serde(deserialize_with = "lenient")]
    pub extra_kubeconfigs: Vec<PathBuf>,
    #[serde(deserialize_with = "lenient")]
    pub last_cluster: Option<ClusterId>,
    #[serde(deserialize_with = "tolerant_clusters")]
    pub clusters: BTreeMap<String, ClusterPrefs>,
    /// Whole-window visual style (missing or unknown in files: Standard).
    #[serde(deserialize_with = "lenient")]
    pub appearance: Appearance,
    /// Settings window, Appearance section.
    #[serde(deserialize_with = "lenient")]
    pub ui: UiPrefs,
    /// Settings window, Terminal section.
    #[serde(deserialize_with = "lenient")]
    pub terminal: TerminalPrefs,
    /// Settings window, Logs section.
    #[serde(deserialize_with = "lenient")]
    pub logs: LogPrefs,
    /// Settings window, Data section.
    #[serde(deserialize_with = "lenient")]
    pub data: DataPrefs,
    /// Alert rules per cluster, OS notifications and history retention
    /// (everything off by default).
    #[serde(deserialize_with = "lenient")]
    pub alerts: AlertsConfig,
    /// Settings window, Updates section.
    #[serde(deserialize_with = "lenient")]
    pub updates: UpdatePrefs,
}

impl Settings {
    /// Preferences for a cluster (defaults when none were saved).
    pub fn prefs(&self, cluster: &ClusterId) -> ClusterPrefs {
        self.clusters
            .get(cluster.as_str())
            .cloned()
            .unwrap_or_default()
    }

    /// Snapshot of the preferences the Settings window edits.
    pub fn user_prefs(&self) -> UserPrefs {
        UserPrefs {
            appearance: self.appearance,
            ui: self.ui.clone(),
            terminal: self.terminal.clone(),
            logs: self.logs.clone(),
            data: self.data.clone(),
            updates: self.updates.clone(),
        }
    }

    /// Replaces the preference fields (clamped); cluster state, last cluster
    /// and kubeconfig sources are untouched.
    pub fn apply_user_prefs(&mut self, mut prefs: UserPrefs) {
        prefs.clamp_in_place();
        self.appearance = prefs.appearance;
        self.ui = prefs.ui;
        self.terminal = prefs.terminal;
        self.logs = prefs.logs;
        self.data = prefs.data;
        self.updates = prefs.updates;
    }

    /// Namespace filter to use when opening `cluster`: the saved explicit
    /// filter, otherwise the Settings → Data default namespace, otherwise all.
    pub fn initial_namespaces(&self, cluster: &ClusterId) -> NamespaceFilter {
        let saved = self.prefs(cluster).namespaces;
        let explicit = matches!(&saved, NamespaceFilter::Only(set) if !set.is_empty());
        match (&self.data.default_namespace, explicit) {
            (Some(namespace), false) => NamespaceFilter::Only(BTreeSet::from([namespace.clone()])),
            _ => saved,
        }
    }

    pub fn prefs_mut(&mut self, cluster: &ClusterId) -> &mut ClusterPrefs {
        self.clusters.entry(cluster.to_string()).or_default()
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SettingsError {
    #[error("settings I/O: {0}")]
    Io(String),
    #[error("settings format: {0}")]
    Format(String),
}

/// Port: where settings live. The clusters feature provides the file-backed
/// implementation; tests and previews use [`MemorySettingsStore`].
pub trait SettingsStore: Send + Sync {
    fn load(&self) -> Result<Settings, SettingsError>;
    fn save(&self, settings: &Settings) -> Result<(), SettingsError>;

    /// [`load`](Self::load) for startup: a failure yields the defaults plus a
    /// message to show the user (non-blocking), instead of an error the
    /// caller has to swallow.
    fn load_reporting(&self) -> LoadReport {
        match self.load() {
            Ok(settings) => LoadReport {
                settings,
                warning: None,
            },
            Err(error) => LoadReport {
                settings: Settings::default(),
                warning: Some(error.to_string()),
            },
        }
    }
}

/// Result of [`SettingsStore::load_reporting`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoadReport {
    pub settings: Settings,
    /// Set when the stored settings could not be read; the defaults are in use.
    pub warning: Option<String>,
}

/// In-memory store for tests and previews.
#[derive(Debug, Default)]
pub struct MemorySettingsStore {
    inner: Mutex<Settings>,
}

impl SettingsStore for MemorySettingsStore {
    fn load(&self) -> Result<Settings, SettingsError> {
        Ok(self.inner.lock().map(|s| s.clone()).unwrap_or_default())
    }

    fn save(&self, settings: &Settings) -> Result<(), SettingsError> {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = settings.clone();
        }
        Ok(())
    }
}
