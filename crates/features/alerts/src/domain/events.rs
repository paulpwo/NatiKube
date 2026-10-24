//! Normalized watch events: what the monitor adapter feeds the evaluator.
//! Kube types never cross this boundary.

use super::model::Timestamp;

/// Which watch a list marker refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WatchKind {
    Pods,
    Deployments,
    Nodes,
    /// Kubernetes events (only `Warning` ones are forwarded).
    Events,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerState {
    pub name: String,
    pub restart_count: u32,
    /// Current `state.waiting.reason`, if waiting.
    pub waiting_reason: Option<String>,
    /// Reason of the current `state.terminated` (`Error`, `OOMKilled`, ...).
    pub terminated_reason: Option<String>,
    /// Reason of `lastState.terminated`. Kubernetes keeps it indefinitely, so
    /// it only means a new failure when `restart_count` went up.
    pub last_terminated_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodSnapshot {
    pub namespace: String,
    pub name: String,
    /// `status.phase` (`Pending`, `Running`, ...).
    pub phase: String,
    pub created_at: Option<Timestamp>,
    /// Init and regular containers.
    pub containers: Vec<ContainerState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentSnapshot {
    pub namespace: String,
    pub name: String,
    pub desired_replicas: i32,
    pub unavailable_replicas: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeReady {
    True,
    False,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeSnapshot {
    pub name: String,
    pub ready: NodeReady,
}

/// A Kubernetes event of type `Warning`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarningEvent {
    pub namespace: Option<String>,
    pub reason: String,
    pub message: String,
    /// Involved object kind and name.
    pub object_kind: String,
    pub object_name: String,
}

/// A change to an object. `Added` and `Modified` are handled the same way
/// (the evaluator decides "new" from what it already knows), so the monitor
/// may map kube `Apply` to either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectEvent<T> {
    Added(T),
    Modified(T),
    Deleted(T),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionHealth {
    Up,
    /// The API stopped answering (reason is shown to the user).
    Down(String),
}

/// Everything the monitor can tell the evaluator about one cluster.
///
/// A (re)list is bracketed: `ListStarted(kind)`, one `Added` per existing
/// object, `ListDone(kind)`. Objects delivered between the markers are the
/// baseline and never fire alerts; events outside a completed list do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorEvent {
    ListStarted(WatchKind),
    ListDone(WatchKind),
    Pod(ObjectEvent<PodSnapshot>),
    Deployment(ObjectEvent<DeploymentSnapshot>),
    Node(ObjectEvent<NodeSnapshot>),
    Warning(WarningEvent),
    Connection(ConnectionHealth),
}
