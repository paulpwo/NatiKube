use crate::alerts::{AlertTarget, AlertsConfig};
use crate::appearance::Appearance;
use crate::cluster::{ClusterId, ConnectionState};
use crate::navigation::ViewId;
use crate::prefs::UserPrefs;
use crate::resource::{ResourceKind, ResourceRef};
use crate::settings::NamespaceFilter;
use std::path::PathBuf;

/// A request to forward a local port to a pod or service port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortForwardSpec {
    /// A pod or a service.
    pub target: ResourceRef,
    pub remote_port: u16,
    /// `None` lets the OS pick a free local port.
    pub local_port: Option<u16>,
}

impl PortForwardSpec {
    pub fn describe(&self) -> String {
        let local = match self.local_port {
            Some(port) => port.to_string(),
            None => "auto".to_string(),
        };
        format!("{} :{local} -> {}", self.target, self.remote_port)
    }
}

/// Cross-feature navigation and action requests. Features never call each
/// other: a view emits an `AppCommand` (as a GPUI `EventEmitter` event) and
/// the shell routes it to whoever owns the behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppCommand {
    /// Make a cluster active (connects if needed).
    SelectCluster(ClusterId),
    /// Switch the main content area.
    Navigate(ViewId),
    /// Show an object in the right detail panel.
    OpenDetail(ResourceRef),
    /// Close the right detail panel.
    CloseDetail,
    /// Open a log tab in the bottom dock.
    OpenLogs {
        pod: ResourceRef,
        container: Option<String>,
    },
    /// Open an exec terminal tab in the bottom dock.
    OpenExec {
        pod: ResourceRef,
        container: Option<String>,
    },
    /// Open a local shell tab in the bottom dock.
    OpenLocalShell,
    /// Start a port-forward and reveal the port-forwards tab.
    StartPortForward(PortForwardSpec),
    /// Report a connection state change (emitted by the shell's connection
    /// manager, consumed by the cluster rail).
    ConnectionChanged {
        cluster: ClusterId,
        state: ConnectionState,
    },
    /// The namespace filter changed; the shell persists it.
    NamespaceFilterChanged(NamespaceFilter),
    /// The resources feature discovered these CRD kinds; the shell lists them
    /// under Custom Resources in the sidebar (grouped by API group).
    SetCustomResources(Vec<ResourceKind>),
    /// Show a transient error to the user.
    ShowError(String),
    /// Switch the whole-window appearance (persisted by the shell).
    SetAppearance(Appearance),
    /// Open the command palette (⌘K).
    OpenPalette,
    /// Close the command palette.
    ClosePalette,
    /// "Import from Freelens…": the shell runs the clusters import, persists
    /// `extra_kubeconfigs` and reloads the catalog.
    ImportFreelens,
    /// "Add kubeconfig…": the shell opens a native file/folder panel, appends
    /// the choice to `extra_kubeconfigs` and reloads the catalog.
    AddKubeconfig,
    /// Rescan kubeconfig files and refresh the cluster list.
    ReloadClusters,
    /// Open (or focus) the Settings window (⌘, / NatiKube → Settings…).
    OpenSettings,
    /// The Settings window edited a preference. Carries the full new
    /// preference set; the shell clamps, persists, applies live (appearance,
    /// fonts, density, ...) and ignores no-op changes.
    SettingsChanged(Box<UserPrefs>),
    /// "Remove" in Settings → Clusters: the shell drops this path from
    /// `extra_kubeconfigs`, persists and reloads the catalog.
    RemoveKubeconfig(PathBuf),
    /// The alert history changed (new alert, read state, cleared): bells
    /// refresh their badge.
    AlertsChanged,
    /// Open the alert history panel (View → Notifications).
    OpenAlerts,
    /// An alert was clicked: switch to its cluster and open its object.
    OpenAlertObject(AlertTarget),
    /// The Alerts settings changed. The shell clamps, persists and applies it
    /// (monitor, OS switch, retention).
    AlertsConfigChanged(Box<AlertsConfig>),
    /// "Send test notification" in Settings → Alerts.
    SendTestNotification,
    /// "Clear history" in Settings → Alerts.
    ClearAlertHistory,
}
