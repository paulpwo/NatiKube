use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use natikube_core::Retention;

use super::model::{Alert, StoredAlert, Timestamp};
use super::permission::PermissionState;
use super::ports::{AlertStore, OsNotifier};

/// Where fired alerts go: always the history, and the OS notifier only while
/// the user has that switch on. Thread-safe; share it in an `Arc`.
pub struct AlertDispatcher {
    store: Arc<dyn AlertStore>,
    notifier: Arc<dyn OsNotifier>,
    os_enabled: AtomicBool,
    retention: Mutex<Retention>,
    on_change: Arc<dyn Fn() + Send + Sync>,
    on_fired: Mutex<Option<FiredHook>>,
}

/// Receives each batch of newly stored alerts (toasts and sound).
pub type FiredHook = Arc<dyn Fn(&[StoredAlert]) + Send + Sync>;

impl AlertDispatcher {
    /// `on_change` runs (on the caller's thread) after new rows were stored;
    /// the integrator uses it to wake the UI. OS notifications start off.
    pub fn new(
        store: Arc<dyn AlertStore>,
        notifier: Arc<dyn OsNotifier>,
        on_change: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            store,
            notifier,
            os_enabled: AtomicBool::new(false),
            retention: Mutex::new(Retention::default()),
            on_change,
            on_fired: Mutex::new(None),
        }
    }

    /// Registers the hook that gets every batch of newly stored alerts, with
    /// the history row of each, on the caller's thread (a monitor thread):
    /// post to a channel, never do UI work in it.
    pub fn set_on_fired(&self, hook: FiredHook) {
        if let Ok(mut guard) = self.on_fired.lock() {
            *guard = Some(hook);
        }
    }

    pub fn set_os_enabled(&self, enabled: bool) {
        self.os_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn os_enabled(&self) -> bool {
        self.os_enabled.load(Ordering::Relaxed)
    }

    pub fn set_retention(&self, retention: Retention) {
        if let Ok(mut guard) = self.retention.lock() {
            *guard = retention.clamped();
        }
    }

    /// Stores every alert, sends OS notifications if enabled, prunes the
    /// history and signals `on_change`. Failures are logged, never raised:
    /// monitoring must not stop because a disk write failed. Returns how many
    /// alerts were stored.
    pub fn dispatch(&self, alerts: &[Alert], now: Timestamp) -> usize {
        let mut stored = 0;
        let mut fired = Vec::new();
        for alert in alerts {
            match self.store.insert(alert) {
                Ok(row_id) => {
                    stored += 1;
                    fired.push(StoredAlert {
                        row_id,
                        alert: alert.clone(),
                        read: false,
                    });
                }
                Err(error) => tracing::warn!(%error, "could not store alert"),
            }
            if self.os_enabled()
                && let Err(error) = self.notifier.notify(alert)
            {
                tracing::warn!(%error, "could not send OS notification");
            }
        }
        if stored > 0 {
            let retention = self.retention.lock().map(|r| r.clone()).unwrap_or_default();
            if let Err(error) = self.store.prune(now, &retention) {
                tracing::warn!(%error, "could not prune alert history");
            }
            (self.on_change)();
            let hook = self.on_fired.lock().ok().and_then(|guard| guard.clone());
            if let Some(hook) = hook {
                hook(&fired);
            }
        }
        stored
    }

    pub fn store(&self) -> &Arc<dyn AlertStore> {
        &self.store
    }

    pub fn permission(&self) -> PermissionState {
        self.notifier.permission()
    }

    pub fn request_permission(&self) -> PermissionState {
        self.notifier.request_permission()
    }

    /// Sends the test notification regardless of the switch (the user asked
    /// for it explicitly).
    pub fn send_test_notification(&self) -> Result<(), super::ports::NotifyError> {
        self.notifier.send_test()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use natikube_core::{ClusterId, RuleKind};

    use super::*;
    use crate::domain::model::{Severity, StoredAlert};
    use crate::domain::ports::{NotifyError, StoreError};

    #[derive(Default)]
    struct FakeStore {
        rows: Mutex<Vec<Alert>>,
        pruned: Mutex<Vec<(Timestamp, Retention)>>,
        fail_insert: AtomicBool,
    }

    impl AlertStore for FakeStore {
        fn insert(&self, alert: &Alert) -> Result<i64, StoreError> {
            if self.fail_insert.load(Ordering::Relaxed) {
                return Err(StoreError("disk full".into()));
            }
            let mut rows = self.rows.lock().unwrap();
            rows.push(alert.clone());
            Ok(rows.len() as i64)
        }
        fn list(&self, _: u32, _: u32) -> Result<Vec<StoredAlert>, StoreError> {
            Ok(Vec::new())
        }
        fn total_count(&self) -> Result<u32, StoreError> {
            Ok(self.rows.lock().unwrap().len() as u32)
        }
        fn unread_count(&self) -> Result<u32, StoreError> {
            Ok(0)
        }
        fn mark_read(&self, _: i64) -> Result<(), StoreError> {
            Ok(())
        }
        fn mark_all_read(&self) -> Result<(), StoreError> {
            Ok(())
        }
        fn clear(&self) -> Result<(), StoreError> {
            Ok(())
        }
        fn prune(&self, now: Timestamp, retention: &Retention) -> Result<u32, StoreError> {
            self.pruned.lock().unwrap().push((now, retention.clone()));
            Ok(0)
        }
    }

    #[derive(Default)]
    struct CountingNotifier {
        sent: AtomicUsize,
        tests: AtomicUsize,
    }

    impl OsNotifier for CountingNotifier {
        fn notify(&self, _: &Alert) -> Result<(), NotifyError> {
            self.sent.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        fn send_test(&self) -> Result<(), NotifyError> {
            self.tests.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    fn alert(n: i64) -> Alert {
        Alert {
            id: format!("id-{n}"),
            cluster: ClusterId::new("c"),
            rule: RuleKind::NewPod,
            severity: Severity::Info,
            namespace: None,
            object_kind: None,
            object_name: None,
            message: "m".into(),
            timestamp: n,
            dedup_key: format!("k{n}"),
        }
    }

    struct Fixture {
        store: Arc<FakeStore>,
        notifier: Arc<CountingNotifier>,
        changes: Arc<AtomicUsize>,
        dispatcher: AlertDispatcher,
    }

    fn fixture() -> Fixture {
        let store = Arc::new(FakeStore::default());
        let notifier = Arc::new(CountingNotifier::default());
        let changes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&changes);
        let dispatcher = AlertDispatcher::new(
            store.clone(),
            notifier.clone(),
            Arc::new(move || {
                counter.fetch_add(1, Ordering::Relaxed);
            }),
        );
        Fixture {
            store,
            notifier,
            changes,
            dispatcher,
        }
    }

    #[test]
    fn os_notifications_are_off_by_default() {
        let f = fixture();
        assert!(!f.dispatcher.os_enabled());
        assert_eq!(f.dispatcher.dispatch(&[alert(1), alert(2)], 10), 2);
        assert_eq!(f.store.total_count().unwrap(), 2, "history always records");
        assert_eq!(f.notifier.sent.load(Ordering::Relaxed), 0);
        assert_eq!(
            f.changes.load(Ordering::Relaxed),
            1,
            "one wake-up per batch"
        );
    }

    #[test]
    fn os_notifications_follow_the_switch() {
        let f = fixture();
        f.dispatcher.set_os_enabled(true);
        f.dispatcher.dispatch(&[alert(1), alert(2)], 10);
        assert_eq!(f.notifier.sent.load(Ordering::Relaxed), 2);
        f.dispatcher.set_os_enabled(false);
        f.dispatcher.dispatch(&[alert(3)], 11);
        assert_eq!(f.notifier.sent.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn empty_batches_do_nothing() {
        let f = fixture();
        assert_eq!(f.dispatcher.dispatch(&[], 10), 0);
        assert_eq!(f.changes.load(Ordering::Relaxed), 0);
        assert!(f.store.pruned.lock().unwrap().is_empty());
    }

    #[test]
    fn prunes_with_the_configured_clamped_retention() {
        let f = fixture();
        f.dispatcher.set_retention(Retention {
            max_age_days: 0,
            max_rows: 1,
        });
        f.dispatcher.dispatch(&[alert(1)], 77);
        let pruned = f.store.pruned.lock().unwrap();
        assert_eq!(
            pruned[0],
            (
                77,
                Retention {
                    max_age_days: 1,
                    max_rows: 100
                }
            )
        );
    }

    #[test]
    fn a_failing_store_never_panics_and_does_not_wake_the_ui() {
        let f = fixture();
        f.store.fail_insert.store(true, Ordering::Relaxed);
        assert_eq!(f.dispatcher.dispatch(&[alert(1)], 1), 0);
        assert_eq!(f.changes.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn the_fired_hook_gets_the_stored_rows_of_each_batch() {
        let f = fixture();
        type Seen = Vec<Vec<(i64, String)>>;
        let seen: Arc<Mutex<Seen>> = Arc::default();
        let sink = Arc::clone(&seen);
        f.dispatcher
            .set_on_fired(Arc::new(move |rows: &[StoredAlert]| {
                sink.lock().unwrap().push(
                    rows.iter()
                        .map(|r| (r.row_id, r.alert.id.clone()))
                        .collect(),
                );
            }));
        f.dispatcher.dispatch(&[alert(1), alert(2)], 10);
        f.dispatcher.dispatch(&[alert(3)], 11);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2, "one call per batch");
        assert_eq!(seen[0], vec![(1, "id-1".into()), (2, "id-2".into())]);
        assert_eq!(seen[1], vec![(3, "id-3".into())]);
    }

    #[test]
    fn alerts_that_could_not_be_stored_are_not_announced() {
        let f = fixture();
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        f.dispatcher
            .set_on_fired(Arc::new(move |_: &[StoredAlert]| {
                counter.fetch_add(1, Ordering::Relaxed);
            }));
        f.store.fail_insert.store(true, Ordering::Relaxed);
        f.dispatcher.dispatch(&[alert(1)], 1);
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_notification_ignores_the_switch() {
        let f = fixture();
        f.dispatcher.send_test_notification().unwrap();
        assert_eq!(f.notifier.tests.load(Ordering::Relaxed), 1);
    }
}
