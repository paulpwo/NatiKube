//! F9 — Alerts (vertical slice): user-chosen monitoring rules, a local
//! notification history and optional OS notifications.
//!
//! * `domain`: pure rule evaluation ([`Evaluator`]), the [`Alert`] value, the
//!   ports ([`AlertStore`], [`OsNotifier`]) and the [`AlertDispatcher`] that
//!   persists alerts and honors the OS-notification switch.
//! * `adapters::store`: SQLite history ([`SqliteAlertStore`]).
//! * `adapters::monitor`: kube watchers feeding the evaluator (stub).
//! * `adapters::notify`: OS notification sender.
//! * `adapters::sound`: system alert sounds.
//! * `ui::bell` / `ui::toast` / `ui::settings`: header bell with history panel,
//!   in-app toasts and the Settings section.
//!
//! The alert configuration model lives in `natikube-core` (it is part of
//! `Settings`); it is re-exported here for convenience. See the Contracts
//! section of `odd/tasks/alerts-notifications.md` for the wiring.

pub mod adapters;
pub mod domain;
pub mod ui;

pub use adapters::{DB_FILE_NAME, LATEST_SCHEMA_VERSION, SqliteAlertStore, StoreSchemaVersion};
pub use domain::{
    Alert, AlertDispatcher, AlertStore, BatchPlan, ConnectionHealth, ContainerState,
    DeploymentSnapshot, Evaluator, MAX_TOASTS, MonitorEvent, NodeReady, NodeSnapshot, NotifyError,
    ObjectEvent, OsNotifier, PermissionState, PodSnapshot, SOUND_MIN_INTERVAL_MS, Severity,
    SoundError, SoundPlayer, SoundThrottle, StoreError, StoredAlert, TOAST_TTL_MS, Timestamp,
    ToastEntry, ToastQueue, WarningEvent, WatchKind, plan_batch, rule_severity,
};
pub use natikube_core::{
    AlertTarget, AlertsConfig, ClusterAlerts, ParamKind, Retention, RuleConfig, RuleKind,
    RuleParams,
};
