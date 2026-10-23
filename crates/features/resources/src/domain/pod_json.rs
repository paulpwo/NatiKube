//! Pods from raw JSON (as the dynamic watcher delivers them) to table rows,
//! with the same kubectl derivations as the typed F0 mapping.

use std::collections::HashMap;

use serde_json::Value;

use super::json::{array_at, at, bool_at, i64_at, str_at, time_at};
use super::object::{Cell, ObjectRow, OwnerRef};
use super::pod::{
    ContainerFacts, ContainerStateKind, ContainerStatusFacts, InitContainerFacts, PodFacts, PodRow,
};
use super::status::{StatusTone, status_tone};
use super::text::{Interner, Text};

fn state_kind(state: &Value) -> Option<ContainerStateKind> {
    if let Some(waiting) = at(state, "waiting") {
        return Some(ContainerStateKind::Waiting {
            reason: str_at(waiting, "reason").map(str::to_string),
        });
    }
    if let Some(terminated) = at(state, "terminated") {
        return Some(ContainerStateKind::Terminated {
            reason: str_at(terminated, "reason").map(str::to_string),
            exit_code: i64_at(terminated, "exitCode").unwrap_or(0) as i32,
            signal: i64_at(terminated, "signal").unwrap_or(0) as i32,
        });
    }
    at(state, "running").map(|_| ContainerStateKind::Running)
}

fn status_facts(status: &Value) -> ContainerStatusFacts {
    ContainerStatusFacts {
        ready: bool_at(status, "ready").unwrap_or(false),
        started: bool_at(status, "started").unwrap_or(false),
        restart_count: i64_at(status, "restartCount").unwrap_or(0).max(0) as u32,
        state: at(status, "state").and_then(state_kind),
    }
}

fn statuses(pod: &Value, path: &str) -> HashMap<String, ContainerStatusFacts> {
    array_at(pod, path)
        .iter()
        .filter_map(|s| Some((str_at(s, "name")?.to_string(), status_facts(s))))
        .collect()
}

fn condition_true(pod: &Value, kind: &str) -> bool {
    array_at(pod, "status.conditions")
        .iter()
        .any(|c| str_at(c, "type") == Some(kind) && str_at(c, "status") == Some("True"))
}

/// Extracts the facts the pod derivations need from a raw `Pod` object.
pub fn pod_facts_from_value(pod: &Value) -> PodFacts {
    let mut init_statuses = statuses(pod, "status.initContainerStatuses");
    let mut container_statuses = statuses(pod, "status.containerStatuses");

    let init_containers = array_at(pod, "spec.initContainers")
        .iter()
        .map(|c| {
            let name = str_at(c, "name").unwrap_or_default().to_string();
            InitContainerFacts {
                restartable: str_at(c, "restartPolicy") == Some("Always"),
                status: init_statuses.remove(&name),
                name,
            }
        })
        .collect();
    let containers = array_at(pod, "spec.containers")
        .iter()
        .map(|c| {
            let name = str_at(c, "name").unwrap_or_default().to_string();
            ContainerFacts {
                status: container_statuses.remove(&name),
                name,
            }
        })
        .collect();

    PodFacts {
        name: str_at(pod, "metadata.name").unwrap_or_default().to_string(),
        namespace: str_at(pod, "metadata.namespace")
            .unwrap_or_default()
            .to_string(),
        controlled_by: controller(pod).map(|o| o.kind.to_string()),
        created_at: time_at(pod, "metadata.creationTimestamp"),
        phase: str_at(pod, "status.phase").map(str::to_string),
        reason: str_at(pod, "status.reason").map(str::to_string),
        deleting: at(pod, "metadata.deletionTimestamp").is_some(),
        scheduling_gated: array_at(pod, "status.conditions").iter().any(|c| {
            str_at(c, "type") == Some("PodScheduled")
                && str_at(c, "reason") == Some("SchedulingGated")
        }),
        initialized: condition_true(pod, "Initialized"),
        ready_condition: condition_true(pod, "Ready"),
        init_containers,
        containers,
    }
}

/// The controlling owner (or the first owner) of any object.
pub fn controller(object: &Value) -> Option<OwnerRef> {
    let owners = array_at(object, "metadata.ownerReferences");
    owners
        .iter()
        .find(|o| bool_at(o, "controller") == Some(true))
        .or_else(|| owners.first())
        .and_then(|o| {
            Some(OwnerRef {
                kind: str_at(o, "kind")?.into(),
                name: str_at(o, "name").unwrap_or_default().into(),
            })
        })
}

fn container_tone(container: &super::pod::ContainerState) -> StatusTone {
    match &container.state {
        Some(ContainerStateKind::Running) if container.ready => StatusTone::Ok,
        Some(ContainerStateKind::Running) => StatusTone::Warning,
        Some(ContainerStateKind::Waiting { reason }) => match reason.as_deref() {
            Some("ContainerCreating" | "PodInitializing") | None => StatusTone::Warning,
            Some(_) => StatusTone::Error,
        },
        Some(ContainerStateKind::Terminated { exit_code, .. }) => {
            if *exit_code == 0 {
                StatusTone::Done
            } else {
                StatusTone::Error
            }
        }
        None => StatusTone::Warning,
    }
}

/// Converts a pod row into a table row. Data cells are, in order: containers
/// (squares), restarts, status.
pub fn pod_object_row(pod: &PodRow) -> ObjectRow {
    let tone = status_tone(&pod.status);
    let mut row = ObjectRow::new(Some(pod.namespace.clone()), pod.name.clone());
    row.created_at = pod.created_at;
    row.health = tone;
    row.cells = vec![
        Cell::squares(
            pod.containers.iter().map(container_tone).collect(),
            pod.containers_label(),
        ),
        Cell::number(f64::from(pod.restarts), pod.restarts.to_string()),
        Cell::toned(pod.status.clone(), tone),
    ];
    row.owner = pod.controlled_by.as_ref().map(|kind| OwnerRef {
        kind: kind.as_str().into(),
        name: Text::default(),
    });
    row
}

fn compact_row(pod: &PodRow, pool: &mut Interner) -> ObjectRow {
    let mut row = pod_object_row(pod);
    row.compact(pool);
    row
}

/// Adapts a [`PodSource`] (the F0 pod port) to the generic [`ObjectSource`],
/// e.g. for `--fake-rows`.
pub struct PodRows<S>(pub S);

impl<S: super::source::PodSource> super::object::ObjectSource for PodRows<S> {
    fn watch(&self) -> super::object::RowEventStream {
        use futures::StreamExt as _;

        use super::object::RowEvent;
        use super::source::PodEvent;
        let mut pool = Interner::new();
        Box::pin(self.0.watch().map(move |item| {
            item.map(|event| match event {
                PodEvent::Applied(row) => RowEvent::Applied(compact_row(&row, &mut pool)),
                PodEvent::Deleted { namespace, name } => RowEvent::Deleted {
                    namespace: Some(namespace),
                    name,
                },
                PodEvent::Restarted(rows) => {
                    RowEvent::Restarted(rows.iter().map(|r| compact_row(r, &mut pool)).collect())
                }
            })
        }))
    }
}
