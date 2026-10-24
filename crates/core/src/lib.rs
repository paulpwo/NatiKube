//! NatiKube shared kernel: identifiers, resource descriptors, navigation,
//! the cross-feature [`AppCommand`], settings model and formatting helpers.
//!
//! This crate performs no I/O and depends on neither GPUI nor kube. Every
//! other crate may depend on it; it depends on nothing of ours.

mod alerts;
mod appearance;
mod cluster;
mod command;
mod format;
mod navigation;
mod prefs;
mod resource;
mod settings;

pub use alerts::{
    AlertTarget, AlertsConfig, ClusterAlerts, ParamKind, Retention, RuleConfig, RuleKind,
    RuleParams,
};
pub use appearance::Appearance;
pub use cluster::{ClusterId, ClusterInfo, ConnectionState, short_context_name};
pub use command::{AppCommand, PortForwardSpec};
pub use format::{
    age_since, format_age, format_cpu, format_memory, parse_cpu_cores, parse_memory_bytes,
    parse_quantity,
};
pub use navigation::{SidebarEntry, SidebarSection, ViewId, kind_title, section_index, sidebar};
pub use prefs::{
    Clamp, DataPrefs, Density, LogPrefs, TerminalPrefs, UiPrefs, UpdatePrefs, UserPrefs,
};
pub use resource::{ResourceKind, ResourceRef, kinds};
pub use settings::{
    ClusterPrefs, LoadReport, MemorySettingsStore, NamespaceFilter, Settings, SettingsError,
    SettingsStore,
};
