//! Connection health derived from the watchers' own error path: a transport
//! failure is "down", the next healthy item is "up". Authorization and
//! missing-resource errors say nothing about reachability.

use crate::domain::{ConnectionHealth, MonitorEvent};

/// What a watcher error means for the cluster's health.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorClass {
    /// The API server cannot be reached (or answers with a server failure).
    Unreachable,
    /// The API answered but refuses this watch (403) or does not serve it (404).
    Refused,
}

pub(crate) fn classify_error(text: &str) -> ErrorClass {
    let lower = text.to_lowercase();
    if text.contains("403")
        || text.contains("404")
        || lower.contains("forbidden")
        || lower.contains("not found")
    {
        ErrorClass::Refused
    } else {
        ErrorClass::Unreachable
    }
}

/// Edge detector: reports `Down` once per outage and `Up` once on recovery.
#[derive(Debug, Default)]
pub(crate) struct HealthTracker {
    down: bool,
}

impl HealthTracker {
    pub(crate) fn on_error(&mut self, text: &str) -> Option<MonitorEvent> {
        if classify_error(text) == ErrorClass::Refused || self.down {
            return None;
        }
        self.down = true;
        Some(MonitorEvent::Connection(ConnectionHealth::Down(
            text.to_string(),
        )))
    }

    pub(crate) fn on_ok(&mut self) -> Option<MonitorEvent> {
        if !self.down {
            return None;
        }
        self.down = false;
        Some(MonitorEvent::Connection(ConnectionHealth::Up))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_errors_are_unreachable_and_authz_errors_are_not() {
        assert_eq!(
            classify_error("failed to perform initial object list: connection refused"),
            ErrorClass::Unreachable
        );
        assert_eq!(
            classify_error("ApiError: pods is forbidden: User cannot list (403)"),
            ErrorClass::Refused
        );
        assert_eq!(
            classify_error("ApiError: the server could not find the requested resource (404)"),
            ErrorClass::Refused
        );
    }

    #[test]
    fn down_and_up_are_reported_once_per_outage() {
        let mut health = HealthTracker::default();
        assert_eq!(health.on_ok(), None);
        assert!(matches!(
            health.on_error("connection refused"),
            Some(MonitorEvent::Connection(ConnectionHealth::Down(_)))
        ));
        assert_eq!(health.on_error("connection refused"), None);
        assert_eq!(
            health.on_ok(),
            Some(MonitorEvent::Connection(ConnectionHealth::Up))
        );
        assert_eq!(health.on_ok(), None);
    }

    #[test]
    fn refused_errors_never_mark_the_cluster_down() {
        let mut health = HealthTracker::default();
        assert_eq!(health.on_error("pods is forbidden (403)"), None);
        assert_eq!(health.on_ok(), None);
    }
}
