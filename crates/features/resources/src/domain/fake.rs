use super::pod::{ContainerState, ContainerStateKind, PodRow};

const NAMESPACES: [&str; 6] = [
    "default",
    "kube-system",
    "monitoring",
    "payments",
    "platform",
    "staging",
];
const OWNERS: [Option<&str>; 4] = [
    Some("ReplicaSet"),
    Some("DaemonSet"),
    Some("StatefulSet"),
    None,
];
const STATUSES: [&str; 8] = [
    "Running",
    "Running",
    "Running",
    "Running",
    "Pending",
    "Completed",
    "CrashLoopBackOff",
    "Terminating",
];

/// Deterministic synthetic pods for scroll and render testing without a
/// cluster.
pub fn synthetic_rows(count: usize, now: i64) -> Vec<PodRow> {
    (0..count)
        .map(|i| {
            let status = STATUSES[i % STATUSES.len()];
            let total = 1 + i % 3;
            let containers = (0..total)
                .map(|c| ContainerState {
                    name: format!("c{c}"),
                    ready: status == "Running",
                    restart_count: ((i + c) % 5) as u32,
                    state: Some(ContainerStateKind::Running),
                    sidecar: false,
                })
                .collect::<Vec<_>>();
            let restarts = containers.iter().map(|c| c.restart_count).sum();
            PodRow {
                name: format!("synthetic-pod-{i:06}"),
                namespace: NAMESPACES[i % NAMESPACES.len()].to_string(),
                containers,
                restarts,
                controlled_by: OWNERS[i % OWNERS.len()].map(str::to_string),
                created_at: Some(now - (i as i64 * 97) % (30 * 86_400)),
                status: status.to_string(),
            }
        })
        .collect()
}

/// A [`PodSource`](super::PodSource) that yields one listing of synthetic
/// pods and then stays open. Used by `--fake-rows` and previews.
#[derive(Debug, Clone)]
pub struct FakePodSource {
    count: usize,
    now: i64,
}

impl FakePodSource {
    pub fn new(count: usize, now: i64) -> Self {
        Self { count, now }
    }
}

impl super::PodSource for FakePodSource {
    fn watch(&self) -> super::PodEventStream {
        use futures::StreamExt as _;
        // Generated per watch instead of retained, so the source itself holds
        // no rows next to the table's.
        let first = super::PodEvent::Restarted(synthetic_rows(self.count, self.now));
        Box::pin(futures::stream::once(async move { Ok(first) }).chain(futures::stream::pending()))
    }
}
