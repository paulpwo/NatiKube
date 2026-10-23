use std::collections::HashMap;

use crate::domain::{
    ContainerFacts, ContainerStateKind, ContainerStatusFacts, InitContainerFacts, PodEvent,
    PodFacts, PodRow,
};
use k8s_openapi::api::core::v1::{ContainerState, ContainerStatus, Pod, PodCondition};
use kube::runtime::watcher::Event;

fn state_kind(state: &ContainerState) -> Option<ContainerStateKind> {
    if let Some(waiting) = &state.waiting {
        return Some(ContainerStateKind::Waiting {
            reason: waiting.reason.clone(),
        });
    }
    if let Some(terminated) = &state.terminated {
        return Some(ContainerStateKind::Terminated {
            reason: terminated.reason.clone(),
            exit_code: terminated.exit_code,
            signal: terminated.signal.unwrap_or(0),
        });
    }
    state.running.as_ref().map(|_| ContainerStateKind::Running)
}

fn status_facts(status: &ContainerStatus) -> ContainerStatusFacts {
    ContainerStatusFacts {
        ready: status.ready,
        started: status.started.unwrap_or(false),
        restart_count: u32::try_from(status.restart_count).unwrap_or(0),
        state: status.state.as_ref().and_then(state_kind),
    }
}

fn condition_true(conditions: &[PodCondition], kind: &str) -> bool {
    conditions
        .iter()
        .any(|c| c.type_ == kind && c.status == "True")
}

/// Extracts the facts the domain needs from a Kubernetes pod.
pub fn pod_to_facts(pod: &Pod) -> PodFacts {
    let meta = &pod.metadata;
    let status = pod.status.as_ref();
    let conditions = status
        .and_then(|s| s.conditions.as_deref())
        .unwrap_or_default();

    let statuses_by_name =
        |list: Option<&Vec<ContainerStatus>>| -> HashMap<String, ContainerStatusFacts> {
            list.map(|l| {
                l.iter()
                    .map(|s| (s.name.clone(), status_facts(s)))
                    .collect()
            })
            .unwrap_or_default()
        };
    let mut init_statuses =
        statuses_by_name(status.and_then(|s| s.init_container_statuses.as_ref()));
    let mut statuses = statuses_by_name(status.and_then(|s| s.container_statuses.as_ref()));

    let spec = pod.spec.as_ref();
    let init_containers = spec
        .and_then(|s| s.init_containers.as_ref())
        .map(|list| {
            list.iter()
                .map(|c| InitContainerFacts {
                    name: c.name.clone(),
                    restartable: c.restart_policy.as_deref() == Some("Always"),
                    status: init_statuses.remove(&c.name),
                })
                .collect()
        })
        .unwrap_or_default();
    let containers = spec
        .map(|s| {
            s.containers
                .iter()
                .map(|c| ContainerFacts {
                    name: c.name.clone(),
                    status: statuses.remove(&c.name),
                })
                .collect()
        })
        .unwrap_or_default();

    let owners = meta.owner_references.as_deref().unwrap_or_default();
    let controlled_by = owners
        .iter()
        .find(|o| o.controller == Some(true))
        .or_else(|| owners.first())
        .map(|o| o.kind.clone());

    PodFacts {
        name: meta.name.clone().unwrap_or_default(),
        namespace: meta.namespace.clone().unwrap_or_default(),
        controlled_by,
        created_at: meta.creation_timestamp.as_ref().map(|t| t.0.as_second()),
        phase: status.and_then(|s| s.phase.clone()),
        reason: status.and_then(|s| s.reason.clone()),
        deleting: meta.deletion_timestamp.is_some(),
        scheduling_gated: conditions
            .iter()
            .any(|c| c.type_ == "PodScheduled" && c.reason.as_deref() == Some("SchedulingGated")),
        initialized: condition_true(conditions, "Initialized"),
        ready_condition: condition_true(conditions, "Ready"),
        init_containers,
        containers,
    }
}

/// Maps a Kubernetes pod to a table row.
pub fn pod_to_row(pod: &Pod) -> PodRow {
    PodRow::from_facts(pod_to_facts(pod))
}

/// Turns raw watcher events into domain events, collapsing each initial
/// (re)listing into a single [`PodEvent::Restarted`].
#[derive(Debug, Default)]
pub struct EventAssembler {
    buffer: Vec<PodRow>,
}

impl EventAssembler {
    pub fn push(&mut self, event: Event<Pod>) -> Option<PodEvent> {
        match event {
            Event::Init => {
                self.buffer.clear();
                None
            }
            Event::InitApply(pod) => {
                self.buffer.push(pod_to_row(&pod));
                None
            }
            Event::InitDone => Some(PodEvent::Restarted(std::mem::take(&mut self.buffer))),
            Event::Apply(pod) => Some(PodEvent::Applied(pod_to_row(&pod))),
            Event::Delete(pod) => Some(PodEvent::Deleted {
                namespace: pod.metadata.namespace.unwrap_or_default(),
                name: pod.metadata.name.unwrap_or_default(),
            }),
        }
    }
}
