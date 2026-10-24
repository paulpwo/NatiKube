//! Dev-only sample history so the bell can be reviewed without a cluster
//! (`--fake-alerts`). Never touches the real notifications database.

use std::sync::Arc;

use natikube_core::{ClusterId, RuleKind};

use crate::adapters::SqliteAlertStore;
use crate::domain::{Alert, AlertStore, Timestamp, rule_severity};

/// (rule, namespace, kind, name, message, age in seconds)
type Sample = (
    RuleKind,
    Option<&'static str>,
    Option<&'static str>,
    &'static str,
    &'static str,
    i64,
);

const SAMPLES: &[Sample] = &[
    (
        RuleKind::PodFailing,
        Some("shop"),
        Some("Pod"),
        "checkout-7d9f8-x2k4p",
        "Container app is in CrashLoopBackOff",
        40,
    ),
    (
        RuleKind::NewPod,
        Some("shop"),
        Some("Pod"),
        "worker-5c6b7-q9z1m",
        "Pod worker-5c6b7-q9z1m appeared",
        6 * 60,
    ),
    (
        RuleKind::WarningEvents,
        Some("shop"),
        Some("Pod"),
        "checkout-7d9f8-x2k4p",
        "BackOff: Back-off restarting failed container",
        14 * 60,
    ),
    (
        RuleKind::PodRestarts,
        Some("auth"),
        Some("Pod"),
        "auth-redis-0",
        "Restarted 4 times in the last 10 minutes",
        55 * 60,
    ),
    (
        RuleKind::DeploymentUnavailable,
        Some("web"),
        Some("Deployment"),
        "frontend",
        "2 of 3 replicas unavailable for over 2 minutes",
        2 * 3600,
    ),
    (
        RuleKind::NodeNotReady,
        None,
        Some("Node"),
        "node-b",
        "Node node-b is not Ready",
        5 * 3600,
    ),
    (
        RuleKind::PodPendingTooLong,
        Some("batch"),
        Some("Pod"),
        "report-job-8s7d6",
        "Pending for over 5 minutes",
        9 * 3600,
    ),
    (
        RuleKind::ClusterUnreachable,
        None,
        None,
        "",
        "The cluster API stopped answering",
        26 * 3600,
    ),
    (
        RuleKind::PodFailing,
        Some("web"),
        Some("Pod"),
        "api-6f5d4-lm3nb",
        "Container api was OOMKilled",
        30 * 3600,
    ),
    (
        RuleKind::WarningEvents,
        Some("data"),
        Some("Pod"),
        "postgres-0",
        "FailedMount: Unable to attach or mount volumes",
        3 * 86_400,
    ),
    (
        RuleKind::NewPod,
        Some("web"),
        Some("Pod"),
        "frontend-84c6d-zz91k",
        "Pod frontend-84c6d-zz91k appeared",
        5 * 86_400,
    ),
];

/// `count` sample alerts for `cluster`, newest first by construction (the
/// templates repeat, each lap older than the previous one).
pub fn fake_alerts(now: Timestamp, cluster: &ClusterId, count: usize) -> Vec<Alert> {
    (0..count)
        .map(|n| {
            let (rule, ns, kind, name, message, age) = SAMPLES[n % SAMPLES.len()];
            let lap = (n / SAMPLES.len()) as i64;
            let timestamp = now - age - lap * 6 * 86_400;
            Alert {
                id: format!("fake-{n}"),
                cluster: cluster.clone(),
                rule,
                severity: rule_severity(rule),
                namespace: ns.map(String::from),
                object_kind: kind.map(String::from),
                object_name: (!name.is_empty()).then(|| name.to_string()),
                message: message.to_string(),
                timestamp,
                dedup_key: format!("fake-{n}"),
            }
        })
        .collect()
}

/// Fills `store` with `count` sample alerts for `cluster`; the oldest third is
/// already read. Returns how many were stored.
pub fn seed_store(
    store: &dyn AlertStore,
    now: Timestamp,
    cluster: &ClusterId,
    count: usize,
) -> usize {
    let mut ids = Vec::new();
    for alert in fake_alerts(now, cluster, count) {
        if let Ok(id) = store.insert(&alert) {
            ids.push(id);
        }
    }
    for id in ids.iter().skip(ids.len() - ids.len() / 3) {
        let _ = store.mark_read(*id);
    }
    ids.len()
}

/// An in-memory store holding `count` sample alerts; the oldest third is
/// already read. Falls back to an empty store if SQLite cannot open memory.
pub fn fake_store(now: Timestamp, cluster: &ClusterId, count: usize) -> Arc<dyn AlertStore> {
    let store = SqliteAlertStore::open_in_memory().expect("in-memory sqlite store");
    seed_store(&store, now, cluster, count);
    Arc::new(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_store_has_mixed_read_state() {
        let cluster = ClusterId::new("/k#dev");
        let store = fake_store(1_700_000_000, &cluster, 30);
        assert_eq!(store.total_count().unwrap(), 30);
        let unread = store.unread_count().unwrap();
        assert!(unread > 0 && unread < 30, "unread = {unread}");
    }

    #[test]
    fn fake_alerts_can_exceed_the_badge_cap() {
        let cluster = ClusterId::new("/k#dev");
        let alerts = fake_alerts(1_700_000_000, &cluster, 120);
        assert_eq!(alerts.len(), 120);
        assert_eq!(fake_alerts(0, &cluster, 0).len(), 0);
    }
}
