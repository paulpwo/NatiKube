use super::status::summarize;
use natikube_core::age_since;

/// Observed state of one container, independent of any client library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerStateKind {
    Waiting {
        reason: Option<String>,
    },
    Running,
    Terminated {
        reason: Option<String>,
        exit_code: i32,
        signal: i32,
    },
}

/// The status fields the derivations need from a container status.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerStatusFacts {
    pub ready: bool,
    pub started: bool,
    pub restart_count: u32,
    pub state: Option<ContainerStateKind>,
}

/// A regular container from the pod spec with its status, if reported yet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerFacts {
    pub name: String,
    pub status: Option<ContainerStatusFacts>,
}

/// An init container from the pod spec. `restartable` marks native sidecars
/// (`restartPolicy: Always`), which count as regular containers once started.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InitContainerFacts {
    pub name: String,
    pub restartable: bool,
    pub status: Option<ContainerStatusFacts>,
}

/// Everything the domain needs to know about a pod to build a [`PodRow`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PodFacts {
    pub name: String,
    pub namespace: String,
    /// Kind of the controlling owner reference, e.g. `ReplicaSet`.
    pub controlled_by: Option<String>,
    /// Creation timestamp in unix seconds.
    pub created_at: Option<i64>,
    pub phase: Option<String>,
    pub reason: Option<String>,
    /// `metadata.deletionTimestamp` is set.
    pub deleting: bool,
    /// The `PodScheduled` condition carries reason `SchedulingGated`.
    pub scheduling_gated: bool,
    /// The `Initialized` condition is `True`.
    pub initialized: bool,
    /// The `Ready` condition is `True`.
    pub ready_condition: bool,
    pub init_containers: Vec<InitContainerFacts>,
    pub containers: Vec<ContainerFacts>,
}

/// One container as displayed in the pods table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerState {
    pub name: String,
    /// Counts as ready in the `ready/total` column (kubectl semantics).
    pub ready: bool,
    pub restart_count: u32,
    pub state: Option<ContainerStateKind>,
    /// Native sidecar declared as a restartable init container.
    pub sidecar: bool,
}

/// A pod row as shown in the pods table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodRow {
    pub name: String,
    pub namespace: String,
    pub containers: Vec<ContainerState>,
    pub restarts: u32,
    pub controlled_by: Option<String>,
    /// Creation timestamp in unix seconds.
    pub created_at: Option<i64>,
    pub status: String,
}

impl PodRow {
    /// Builds a row applying the kubectl derivations to raw pod facts.
    pub fn from_facts(facts: PodFacts) -> Self {
        let summary = summarize(&facts);
        Self {
            name: facts.name,
            namespace: facts.namespace,
            containers: summary.containers,
            restarts: summary.restarts,
            controlled_by: facts.controlled_by,
            created_at: facts.created_at,
            status: summary.status,
        }
    }

    pub fn ready_containers(&self) -> usize {
        self.containers.iter().filter(|c| c.ready).count()
    }

    pub fn total_containers(&self) -> usize {
        self.containers.len()
    }

    /// The `READY` column, e.g. `1/2`.
    pub fn containers_label(&self) -> String {
        format!("{}/{}", self.ready_containers(), self.total_containers())
    }

    /// The `AGE` column at `now` (unix seconds).
    pub fn age(&self, now: i64) -> String {
        age_since(self.created_at, now)
    }
}
