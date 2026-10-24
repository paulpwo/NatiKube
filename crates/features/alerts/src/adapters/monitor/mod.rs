//! Kube monitor adapter (task A3): one set of watchers per cluster that has
//! at least one enabled rule, translating kube objects into
//! [`MonitorEvent`](crate::domain::MonitorEvent)s for the cluster's
//! [`Evaluator`](crate::domain::Evaluator) and handing the alerts to the
//! [`AlertDispatcher`](crate::domain::AlertDispatcher). Read-only: list and
//! watch only. See the Contracts section of the feature document.

mod alert_monitor;
mod health;
mod mapping;

pub use alert_monitor::AlertMonitor;
