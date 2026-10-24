//! Pure alert logic: no I/O, no kube, no GPUI.

mod dispatch;
mod evaluator;
mod events;
mod model;
mod ports;

pub use dispatch::AlertDispatcher;
pub use evaluator::Evaluator;
pub use events::{
    ConnectionHealth, ContainerState, DeploymentSnapshot, MonitorEvent, NodeReady, NodeSnapshot,
    ObjectEvent, PodSnapshot, WarningEvent, WatchKind,
};
pub use model::{Alert, Severity, StoredAlert, Timestamp, rule_severity};
pub use ports::{AlertStore, NotifyError, OsNotifier, StoreError};
