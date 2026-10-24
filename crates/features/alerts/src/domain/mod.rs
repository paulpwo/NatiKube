//! Pure alert logic: no I/O, no kube, no GPUI.

mod dispatch;
mod evaluator;
mod events;
mod model;
mod permission;
mod ports;
mod sound;
mod toast;

pub use dispatch::AlertDispatcher;
pub use evaluator::Evaluator;
pub use events::{
    ConnectionHealth, ContainerState, DeploymentSnapshot, MonitorEvent, NodeReady, NodeSnapshot,
    ObjectEvent, PodSnapshot, WarningEvent, WatchKind,
};
pub use model::{Alert, Severity, StoredAlert, Timestamp, rule_severity};
pub use permission::{
    PermissionCache, PermissionState, should_request_at_startup, should_request_on_enable,
};
pub use ports::{AlertStore, NotifyError, OsNotifier, SoundError, SoundPlayer, StoreError};
pub use sound::{BatchPlan, SOUND_MIN_INTERVAL_MS, SoundThrottle, plan_batch};
pub use toast::{MAX_TOASTS, TOAST_TTL_MS, ToastEntry, ToastQueue};
