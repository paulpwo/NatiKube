//! Pure normalization of kube objects and kube-runtime watcher events into
//! [`MonitorEvent`]s. Only the fields the rules need are copied; kube types
//! never reach the evaluator.

use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{ContainerStatus, Event as KubeEvent, Node, Pod};
use kube::runtime::watcher::Event;

use crate::domain::{
    ContainerState, DeploymentSnapshot, MonitorEvent, NodeReady, NodeSnapshot, ObjectEvent,
    PodSnapshot, WarningEvent, WatchKind,
};

fn container(status: &ContainerStatus) -> ContainerState {
    let state = status.state.as_ref();
    let waiting_reason = state
        .and_then(|s| s.waiting.as_ref())
        .and_then(|w| w.reason.clone());
    let terminated_reason = state
        .and_then(|s| s.terminated.as_ref())
        .and_then(|t| t.reason.clone());
    let last_terminated_reason = status
        .last_state
        .as_ref()
        .and_then(|s| s.terminated.as_ref())
        .and_then(|t| t.reason.clone());
    ContainerState {
        name: status.name.clone(),
        restart_count: u32::try_from(status.restart_count).unwrap_or(0),
        waiting_reason,
        terminated_reason,
        last_terminated_reason,
    }
}

pub(crate) fn pod_snapshot(pod: &Pod) -> PodSnapshot {
    let status = pod.status.as_ref();
    let containers = status
        .into_iter()
        .flat_map(|s| {
            s.init_container_statuses
                .iter()
                .flatten()
                .chain(s.container_statuses.iter().flatten())
        })
        .map(container)
        .collect();
    PodSnapshot {
        namespace: pod.metadata.namespace.clone().unwrap_or_default(),
        name: pod.metadata.name.clone().unwrap_or_default(),
        phase: status.and_then(|s| s.phase.clone()).unwrap_or_default(),
        created_at: pod
            .metadata
            .creation_timestamp
            .as_ref()
            .map(|t| t.0.as_second()),
        containers,
    }
}

pub(crate) fn deployment_snapshot(deployment: &Deployment) -> DeploymentSnapshot {
    DeploymentSnapshot {
        namespace: deployment.metadata.namespace.clone().unwrap_or_default(),
        name: deployment.metadata.name.clone().unwrap_or_default(),
        // An omitted `spec.replicas` means one.
        desired_replicas: deployment
            .spec
            .as_ref()
            .and_then(|s| s.replicas)
            .unwrap_or(1),
        unavailable_replicas: deployment
            .status
            .as_ref()
            .and_then(|s| s.unavailable_replicas)
            .unwrap_or(0),
    }
}

pub(crate) fn node_snapshot(node: &Node) -> NodeSnapshot {
    let ready = node
        .status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .and_then(|conditions| conditions.iter().find(|c| c.type_ == "Ready"))
        .map_or(NodeReady::Unknown, |c| match c.status.as_str() {
            "True" => NodeReady::True,
            "False" => NodeReady::False,
            _ => NodeReady::Unknown,
        });
    NodeSnapshot {
        name: node.metadata.name.clone().unwrap_or_default(),
        ready,
    }
}

/// `Some` only for events of type `Warning`.
pub(crate) fn warning_event(event: &KubeEvent) -> Option<WarningEvent> {
    if event.type_.as_deref() != Some("Warning") {
        return None;
    }
    Some(WarningEvent {
        namespace: event
            .metadata
            .namespace
            .clone()
            .or_else(|| event.involved_object.namespace.clone()),
        reason: event.reason.clone().unwrap_or_default(),
        message: event.message.clone().unwrap_or_default(),
        object_kind: event.involved_object.kind.clone().unwrap_or_default(),
        object_name: event.involved_object.name.clone().unwrap_or_default(),
    })
}

/// Maps one watcher event of any kind. `Init`/`InitApply`/`InitDone` become
/// the list markers and baseline `Added`s; `Apply` and `Delete` are live.
fn map_event<K>(
    kind: WatchKind,
    event: Event<K>,
    object: impl Fn(ObjectEvent<&K>) -> Option<MonitorEvent>,
) -> Vec<MonitorEvent> {
    match event {
        Event::Init => vec![MonitorEvent::ListStarted(kind)],
        Event::InitDone => vec![MonitorEvent::ListDone(kind)],
        Event::InitApply(o) => object(ObjectEvent::Added(&o)).into_iter().collect(),
        Event::Apply(o) => object(ObjectEvent::Modified(&o)).into_iter().collect(),
        Event::Delete(o) => object(ObjectEvent::Deleted(&o)).into_iter().collect(),
    }
}

fn rewrap<A, B>(event: ObjectEvent<A>, f: impl FnOnce(A) -> B) -> ObjectEvent<B> {
    match event {
        ObjectEvent::Added(o) => ObjectEvent::Added(f(o)),
        ObjectEvent::Modified(o) => ObjectEvent::Modified(f(o)),
        ObjectEvent::Deleted(o) => ObjectEvent::Deleted(f(o)),
    }
}

pub(crate) fn pod_events(event: Event<Pod>) -> Vec<MonitorEvent> {
    map_event(WatchKind::Pods, event, |e| {
        Some(MonitorEvent::Pod(rewrap(e, pod_snapshot)))
    })
}

pub(crate) fn deployment_events(event: Event<Deployment>) -> Vec<MonitorEvent> {
    map_event(WatchKind::Deployments, event, |e| {
        Some(MonitorEvent::Deployment(rewrap(e, deployment_snapshot)))
    })
}

pub(crate) fn node_events(event: Event<Node>) -> Vec<MonitorEvent> {
    map_event(WatchKind::Nodes, event, |e| {
        Some(MonitorEvent::Node(rewrap(e, node_snapshot)))
    })
}

/// Warning events only. Deleted events are not alerts; events delivered
/// during the initial list are forwarded as warnings and the evaluator
/// ignores everything before `ListDone(Events)`.
pub(crate) fn kube_event_events(event: Event<KubeEvent>) -> Vec<MonitorEvent> {
    map_event(WatchKind::Events, event, |e| match e {
        ObjectEvent::Added(o) | ObjectEvent::Modified(o) => {
            warning_event(o).map(MonitorEvent::Warning)
        }
        ObjectEvent::Deleted(_) => None,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn pod(value: serde_json::Value) -> Pod {
        serde_json::from_value(value).expect("pod fixture")
    }

    fn crash_looping_pod() -> Pod {
        pod(json!({
            "metadata": {
                "name": "api-1", "namespace": "shop",
                "creationTimestamp": "2026-01-02T03:04:05Z"
            },
            "status": {
                "phase": "Running",
                "initContainerStatuses": [{
                    "name": "init", "image": "i", "imageID": "", "ready": true,
                    "restartCount": 1,
                    "state": {"terminated": {"exitCode": 0, "reason": "Completed"}}
                }],
                "containerStatuses": [{
                    "name": "app", "image": "a", "imageID": "", "ready": false,
                    "restartCount": 7,
                    "state": {"waiting": {"reason": "CrashLoopBackOff"}},
                    "lastState": {"terminated": {"exitCode": 137, "reason": "OOMKilled"}}
                }]
            }
        }))
    }

    #[test]
    fn pod_maps_containers_reasons_restarts_and_creation_time() {
        let snapshot = pod_snapshot(&crash_looping_pod());
        assert_eq!(snapshot.namespace, "shop");
        assert_eq!(snapshot.name, "api-1");
        assert_eq!(snapshot.phase, "Running");
        assert_eq!(snapshot.created_at, Some(1_767_323_045));
        assert_eq!(snapshot.containers.len(), 2);
        let init = &snapshot.containers[0];
        assert_eq!(init.name, "init");
        assert_eq!(init.terminated_reason.as_deref(), Some("Completed"));
        let app = &snapshot.containers[1];
        assert_eq!(app.restart_count, 7);
        assert_eq!(app.waiting_reason.as_deref(), Some("CrashLoopBackOff"));
        assert_eq!(app.terminated_reason, None);
        assert_eq!(app.last_terminated_reason.as_deref(), Some("OOMKilled"));
    }

    #[test]
    fn current_and_last_terminated_states_are_kept_apart() {
        let snapshot = pod_snapshot(&pod(json!({
            "metadata": {"name": "p", "namespace": "n"},
            "status": {"phase": "Failed", "containerStatuses": [{
                "name": "c", "image": "i", "imageID": "", "ready": false, "restartCount": 0,
                "state": {"terminated": {"exitCode": 1, "reason": "Error"}},
                "lastState": {"terminated": {"exitCode": 137, "reason": "OOMKilled"}}
            }]}
        })));
        assert_eq!(
            snapshot.containers[0].terminated_reason.as_deref(),
            Some("Error")
        );
        assert_eq!(
            snapshot.containers[0].last_terminated_reason.as_deref(),
            Some("OOMKilled")
        );
    }

    #[test]
    fn pod_without_status_maps_to_empty_phase_and_no_containers() {
        let snapshot = pod_snapshot(&pod(json!({"metadata": {"name": "p"}})));
        assert_eq!(snapshot.phase, "");
        assert!(snapshot.containers.is_empty());
        assert_eq!(snapshot.created_at, None);
        assert_eq!(snapshot.namespace, "");
    }

    #[test]
    fn deployment_defaults_to_one_replica_and_zero_unavailable() {
        let bare: Deployment = serde_json::from_value(json!({
            "metadata": {"name": "web", "namespace": "shop"},
            "spec": {"selector": {}, "template": {}}
        }))
        .unwrap();
        let snapshot = deployment_snapshot(&bare);
        assert_eq!(snapshot.desired_replicas, 1);
        assert_eq!(snapshot.unavailable_replicas, 0);

        let degraded: Deployment = serde_json::from_value(json!({
            "metadata": {"name": "web", "namespace": "shop"},
            "spec": {"replicas": 3, "selector": {}, "template": {}},
            "status": {"unavailableReplicas": 2}
        }))
        .unwrap();
        let snapshot = deployment_snapshot(&degraded);
        assert_eq!(snapshot.desired_replicas, 3);
        assert_eq!(snapshot.unavailable_replicas, 2);
    }

    #[test]
    fn node_readiness_comes_from_the_ready_condition() {
        let node = |status: Option<&str>| -> Node {
            let conditions: Vec<_> = status
                .map(|s| json!({"type": "Ready", "status": s}))
                .into_iter()
                .chain([json!({"type": "DiskPressure", "status": "True"})])
                .collect();
            serde_json::from_value(json!({
                "metadata": {"name": "n1"},
                "status": {"conditions": conditions}
            }))
            .unwrap()
        };
        assert_eq!(node_snapshot(&node(Some("True"))).ready, NodeReady::True);
        assert_eq!(node_snapshot(&node(Some("False"))).ready, NodeReady::False);
        assert_eq!(
            node_snapshot(&node(Some("Unknown"))).ready,
            NodeReady::Unknown
        );
        assert_eq!(node_snapshot(&node(None)).ready, NodeReady::Unknown);
    }

    fn kube_event(kind: &str) -> KubeEvent {
        serde_json::from_value(json!({
            "metadata": {"name": "e", "namespace": "shop"},
            "type": kind,
            "reason": "BackOff",
            "message": "Back-off restarting failed container",
            "involvedObject": {"kind": "Pod", "name": "api-1", "namespace": "shop"}
        }))
        .unwrap()
    }

    #[test]
    fn only_warning_events_are_forwarded() {
        assert_eq!(warning_event(&kube_event("Normal")), None);
        let warning = warning_event(&kube_event("Warning")).unwrap();
        assert_eq!(warning.namespace.as_deref(), Some("shop"));
        assert_eq!(warning.reason, "BackOff");
        assert_eq!(warning.object_kind, "Pod");
        assert_eq!(warning.object_name, "api-1");
    }

    #[test]
    fn warning_events_use_the_involved_namespace_when_metadata_has_none() {
        let mut event = kube_event("Warning");
        event.metadata.namespace = None;
        assert_eq!(
            warning_event(&event).unwrap().namespace.as_deref(),
            Some("shop")
        );
    }

    #[test]
    fn initial_list_is_bracketed_and_live_changes_follow() {
        let events: Vec<MonitorEvent> = [
            Event::Init,
            Event::InitApply(crash_looping_pod()),
            Event::InitDone,
            Event::Apply(crash_looping_pod()),
            Event::Delete(crash_looping_pod()),
        ]
        .into_iter()
        .flat_map(pod_events)
        .collect();
        assert!(matches!(
            events.as_slice(),
            [
                MonitorEvent::ListStarted(WatchKind::Pods),
                MonitorEvent::Pod(ObjectEvent::Added(_)),
                MonitorEvent::ListDone(WatchKind::Pods),
                MonitorEvent::Pod(ObjectEvent::Modified(_)),
                MonitorEvent::Pod(ObjectEvent::Deleted(_)),
            ]
        ));
    }

    #[test]
    fn kube_events_drop_normal_and_deleted_ones_but_keep_the_markers() {
        let events: Vec<MonitorEvent> = [
            Event::Init,
            Event::InitApply(kube_event("Normal")),
            Event::InitApply(kube_event("Warning")),
            Event::InitDone,
            Event::Apply(kube_event("Warning")),
            Event::Delete(kube_event("Warning")),
        ]
        .into_iter()
        .flat_map(kube_event_events)
        .collect();
        assert!(matches!(
            events.as_slice(),
            [
                MonitorEvent::ListStarted(WatchKind::Events),
                MonitorEvent::Warning(_),
                MonitorEvent::ListDone(WatchKind::Events),
                MonitorEvent::Warning(_),
            ]
        ));
    }

    #[test]
    fn deployments_and_nodes_use_their_own_list_markers() {
        let d = deployment_events(Event::Init);
        assert_eq!(d, vec![MonitorEvent::ListStarted(WatchKind::Deployments)]);
        let n = node_events(Event::InitDone);
        assert_eq!(n, vec![MonitorEvent::ListDone(WatchKind::Nodes)]);
    }
}
