//! F9 — Alerts (vertical slice): user-chosen monitoring rules, a local
//! notification history and optional OS notifications.
//!
//! * `domain`: pure rule evaluation ([`Evaluator`]), the [`Alert`] value, the
//!   ports ([`AlertStore`], [`OsNotifier`]) and the [`AlertDispatcher`] that
//!   persists alerts and honors the OS-notification switch.
//! * `adapters::store`: SQLite history ([`SqliteAlertStore`]).
//! * `adapters::monitor`: kube watchers feeding the evaluator (stub).
//! * `adapters::notify`: OS notification sender (stub).
//! * `ui::bell` / `ui::settings`: header bell with history panel and the
//!   Settings section (stubs).
//!
//! The alert configuration model lives in `natikube-core` (it is part of
//! `Settings`); it is re-exported here for convenience. See the Contracts
//! section of `odd/tasks/alerts-notifications.md` for the wiring.

pub mod adapters;
pub mod domain;
pub mod ui;

pub use adapters::{DB_FILE_NAME, LATEST_SCHEMA_VERSION, SqliteAlertStore, StoreSchemaVersion};
pub use domain::{
    Alert, AlertDispatcher, AlertStore, ConnectionHealth, ContainerState, DeploymentSnapshot,
    Evaluator, MonitorEvent, NodeReady, NodeSnapshot, NotifyError, ObjectEvent, OsNotifier,
    PodSnapshot, Severity, StoreError, StoredAlert, Timestamp, WarningEvent, WatchKind,
    rule_severity,
};
pub use natikube_core::{
    AlertTarget, AlertsConfig, ClusterAlerts, ParamKind, Retention, RuleConfig, RuleKind,
    RuleParams,
};
