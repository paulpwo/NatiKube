use k8s_openapi::api::core::v1::Pod;
use kube::runtime::watcher::Event;
use natikube_resources::{EventAssembler, pod_to_row};
use natikube_resources::{PodEvent, PodRow};

fn fixture(name: &str) -> Pod {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).expect("fixture exists");
    serde_json::from_str(&text).expect("fixture is a valid Pod")
}

/// 2026-10-23T12:00:00Z
const NOW: i64 = 1_792_756_800;

#[test]
fn running_replicaset_pod() {
    let row = pod_to_row(&fixture("running_replicaset"));
    assert_eq!(row.name, "api-7d9f8c6b5-x2k4p");
    assert_eq!(row.namespace, "default");
    assert_eq!(row.status, "Running");
    assert_eq!(row.containers_label(), "2/2");
    assert_eq!(row.restarts, 3);
    assert_eq!(row.controlled_by.as_deref(), Some("ReplicaSet"));
    assert_eq!(row.age(NOW), "120m");
}

#[test]
fn pending_pod_without_container_statuses() {
    let row = pod_to_row(&fixture("pending_unschedulable"));
    assert_eq!(row.status, "Pending");
    assert_eq!(row.containers_label(), "0/1");
    assert_eq!(row.restarts, 0);
    assert_eq!(row.controlled_by.as_deref(), Some("StatefulSet"));
    assert_eq!(row.age(NOW), "60s");
}

#[test]
fn crash_looping_pod() {
    let row = pod_to_row(&fixture("crashloop"));
    assert_eq!(row.status, "CrashLoopBackOff");
    assert_eq!(row.containers_label(), "0/1");
    assert_eq!(row.restarts, 14);
    assert_eq!(row.age(NOW), "3d4h");
}

#[test]
fn daemonset_pod_uses_controller_owner_and_counts_sidecar() {
    let row = pod_to_row(&fixture("daemonset_with_sidecar"));
    assert_eq!(row.controlled_by.as_deref(), Some("DaemonSet"));
    assert_eq!(row.status, "Running");
    assert_eq!(row.containers_label(), "2/2");
    assert_eq!(row.restarts, 3);
    assert!(
        row.containers
            .iter()
            .any(|c| c.sidecar && c.name == "log-shipper")
    );
    assert_eq!(row.age(NOW), "22d");
}

#[test]
fn pod_with_deletion_timestamp_is_terminating() {
    let row = pod_to_row(&fixture("terminating"));
    assert_eq!(row.status, "Terminating");
    assert_eq!(row.containers_label(), "1/1");
}

fn names(rows: &[PodRow]) -> Vec<&str> {
    rows.iter().map(|r| r.name.as_str()).collect()
}

#[test]
fn assembler_batches_initial_listing_into_one_restart() {
    let mut assembler = EventAssembler::default();
    assert_eq!(assembler.push(Event::Init), None);
    assert_eq!(assembler.push(Event::InitApply(fixture("crashloop"))), None);
    assert_eq!(
        assembler.push(Event::InitApply(fixture("terminating"))),
        None
    );
    match assembler.push(Event::InitDone) {
        Some(PodEvent::Restarted(rows)) => assert_eq!(
            names(&rows),
            vec!["broken-6c8d5f7b9-q8w2e", "web-5b7c9d8f6-zz9yy"]
        ),
        other => panic!("expected Restarted, got {other:?}"),
    }
}

#[test]
fn assembler_forwards_live_changes() {
    let mut assembler = EventAssembler::default();
    match assembler.push(Event::Apply(fixture("running_replicaset"))) {
        Some(PodEvent::Applied(row)) => assert_eq!(row.name, "api-7d9f8c6b5-x2k4p"),
        other => panic!("expected Applied, got {other:?}"),
    }
    assert_eq!(
        assembler.push(Event::Delete(fixture("terminating"))),
        Some(PodEvent::Deleted {
            namespace: "default".to_string(),
            name: "web-5b7c9d8f6-zz9yy".to_string(),
        })
    );
}

#[test]
fn assembler_relist_drops_previous_partial_buffer() {
    let mut assembler = EventAssembler::default();
    assembler.push(Event::Init);
    assembler.push(Event::InitApply(fixture("crashloop")));
    assembler.push(Event::Init);
    assembler.push(Event::InitApply(fixture("terminating")));
    match assembler.push(Event::InitDone) {
        Some(PodEvent::Restarted(rows)) => assert_eq!(names(&rows), vec!["web-5b7c9d8f6-zz9yy"]),
        other => panic!("expected Restarted, got {other:?}"),
    }
}
