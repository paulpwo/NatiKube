//! OS notification adapter (task A6): an
//! [`OsNotifier`](crate::domain::OsNotifier) for macOS and Windows. Nothing is
//! sent unless the user enabled OS notifications (the dispatcher checks), and
//! [`NoopNotifier`] is the silent implementation for tests and other platforms.
//!
//! macOS goes through `UNUserNotificationCenter` (`macos.rs`), which needs the
//! user's permission and a `.app` bundle; Windows keeps `notify-rust` toasts.

use std::sync::Arc;

use crate::domain::{Alert, NotifyError, OsNotifier, PermissionState};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::open_notification_settings;

#[cfg(target_os = "windows")]
const APP_NAME: &str = "NatiKube";

/// Opens the system notification settings. Only macOS has such a pane to
/// point at.
#[cfg(not(target_os = "macos"))]
pub fn open_notification_settings() -> Result<(), NotifyError> {
    Err(NotifyError(
        "System notification settings are only opened on macOS".into(),
    ))
}

/// Title and body of one notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationContent {
    pub title: String,
    pub body: String,
}

/// Builds the text of an alert notification: the rule and cluster in the
/// title, the object and message in the body.
pub fn content_for(alert: &Alert) -> NotificationContent {
    let title = format!(
        "{} \u{2014} {}",
        alert.rule.title(),
        alert.cluster.context_name()
    );
    let object = match (&alert.namespace, &alert.object_kind, &alert.object_name) {
        (Some(ns), Some(kind), Some(name)) => Some(format!("{kind} {ns}/{name}")),
        (None, Some(kind), Some(name)) => Some(format!("{kind} {name}")),
        (Some(ns), None, Some(name)) => Some(format!("{ns}/{name}")),
        (None, None, Some(name)) => Some(name.clone()),
        _ => None,
    };
    let body = match object {
        Some(object) => format!("{object}\n{}", alert.message),
        None => alert.message.clone(),
    };
    NotificationContent { title, body }
}

/// Content of the "Send test notification" button.
pub fn test_content() -> NotificationContent {
    NotificationContent {
        title: "NatiKube notifications are on".to_string(),
        body: "This is a test. Alerts you enabled will appear like this.".to_string(),
    }
}

/// The part of the platform that actually shows a notification.
pub trait NotificationSink: Send + Sync {
    fn show(&self, content: &NotificationContent) -> Result<(), NotifyError>;
    /// Current permission; see [`OsNotifier::permission`].
    fn permission(&self) -> PermissionState {
        PermissionState::NotApplicable
    }
    /// See [`OsNotifier::request_permission`].
    fn request_permission(&self) -> PermissionState {
        self.permission()
    }
}

/// [`OsNotifier`] that formats alerts and hands them to a sink (the platform
/// sink by default).
pub struct NativeNotifier {
    sink: Arc<dyn NotificationSink>,
}

impl NativeNotifier {
    pub fn new() -> Self {
        Self::with_sink(Arc::new(PlatformSink::new()))
    }

    pub fn with_sink(sink: Arc<dyn NotificationSink>) -> Self {
        Self { sink }
    }
}

impl Default for NativeNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl OsNotifier for NativeNotifier {
    fn notify(&self, alert: &Alert) -> Result<(), NotifyError> {
        self.sink.show(&content_for(alert))
    }

    fn send_test(&self) -> Result<(), NotifyError> {
        self.sink.show(&test_content())
    }

    fn permission(&self) -> PermissionState {
        self.sink.permission()
    }

    fn request_permission(&self) -> PermissionState {
        self.sink.request_permission()
    }
}

/// Never shows anything.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopNotifier;

impl OsNotifier for NoopNotifier {
    fn notify(&self, _alert: &Alert) -> Result<(), NotifyError> {
        Ok(())
    }

    fn send_test(&self) -> Result<(), NotifyError> {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
use macos::UserNotificationSink as PlatformSink;

#[cfg(target_os = "windows")]
struct PlatformSink;

#[cfg(target_os = "windows")]
impl PlatformSink {
    fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "windows")]
impl NotificationSink for PlatformSink {
    fn show(&self, content: &NotificationContent) -> Result<(), NotifyError> {
        // Fire and forget: `show` does not wait for the user to act on it.
        notify_rust::Notification::new()
            .appname(APP_NAME)
            .summary(&content.title)
            .body(&content.body)
            .show()
            .map(|_| ())
            .map_err(|err| NotifyError(err.to_string()))
    }
}

/// Linux has no backend yet (the crate is built without its D-Bus features).
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
struct PlatformSink;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl PlatformSink {
    fn new() -> Self {
        Self
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl NotificationSink for PlatformSink {
    fn show(&self, _content: &NotificationContent) -> Result<(), NotifyError> {
        Err(NotifyError(format!(
            "NatiKube: OS notifications are not supported on this platform yet"
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use natikube_core::{ClusterId, RuleKind};

    use super::*;
    use crate::domain::Severity;

    fn alert() -> Alert {
        Alert {
            id: "a1".into(),
            cluster: ClusterId::new("/home/u/.kube/config#k3s-dev"),
            rule: RuleKind::PodFailing,
            severity: Severity::Critical,
            namespace: Some("shop".into()),
            object_kind: Some("Pod".into()),
            object_name: Some("api-1".into()),
            message: "api restarted: CrashLoopBackOff".into(),
            timestamp: 0,
            dedup_key: "k".into(),
        }
    }

    #[derive(Default)]
    struct Recorder(Mutex<Vec<NotificationContent>>);

    impl NotificationSink for Recorder {
        fn show(&self, content: &NotificationContent) -> Result<(), NotifyError> {
            self.0.lock().unwrap().push(content.clone());
            Ok(())
        }
    }

    #[test]
    fn content_names_rule_cluster_object_and_message() {
        let content = content_for(&alert());
        assert_eq!(content.title, "Pod failing \u{2014} k3s-dev");
        assert_eq!(
            content.body,
            "Pod shop/api-1\napi restarted: CrashLoopBackOff"
        );
    }

    #[test]
    fn content_without_object_is_just_the_message() {
        let mut a = alert();
        a.namespace = None;
        a.object_kind = None;
        a.object_name = None;
        assert_eq!(content_for(&a).body, "api restarted: CrashLoopBackOff");
        a.object_kind = Some("Node".into());
        a.object_name = Some("n1".into());
        assert_eq!(
            content_for(&a).body,
            "Node n1\napi restarted: CrashLoopBackOff"
        );
    }

    #[test]
    fn notifier_forwards_alerts_and_tests_to_the_sink() {
        let sink = Arc::new(Recorder::default());
        let notifier = NativeNotifier::with_sink(sink.clone());
        notifier.notify(&alert()).unwrap();
        notifier.send_test().unwrap();
        let shown = sink.0.lock().unwrap();
        assert_eq!(shown.len(), 2);
        assert_eq!(shown[1], test_content());
    }

    #[test]
    fn noop_notifier_succeeds_silently() {
        assert!(NoopNotifier.notify(&alert()).is_ok());
        assert!(NoopNotifier.send_test().is_ok());
    }

    #[test]
    fn permission_defaults_to_not_applicable_and_follows_the_sink() {
        let plain = NativeNotifier::with_sink(Arc::new(Recorder::default()));
        assert_eq!(plain.permission(), PermissionState::NotApplicable);
        assert_eq!(plain.request_permission(), PermissionState::NotApplicable);
        assert_eq!(NoopNotifier.permission(), PermissionState::NotApplicable);

        struct Asking;
        impl NotificationSink for Asking {
            fn show(&self, _: &NotificationContent) -> Result<(), NotifyError> {
                Ok(())
            }
            fn permission(&self) -> PermissionState {
                PermissionState::NotDetermined
            }
            fn request_permission(&self) -> PermissionState {
                PermissionState::Allowed
            }
        }
        let notifier = NativeNotifier::with_sink(Arc::new(Asking));
        assert_eq!(notifier.permission(), PermissionState::NotDetermined);
        assert_eq!(notifier.request_permission(), PermissionState::Allowed);
    }

    /// Manual check against the real OS: `cargo test -p natikube-alerts
    /// real_os_test_notification -- --ignored`. Shows one notification.
    #[test]
    #[ignore = "sends a real OS notification"]
    fn real_os_test_notification() {
        NativeNotifier::new().send_test().unwrap();
    }
}
