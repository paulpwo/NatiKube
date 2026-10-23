//! The raw-JSON pod path (used by the generic dynamic watcher) must derive
//! exactly what the typed F0 mapping derives.

use natikube_core::kinds;
use natikube_resources::{PodRow, StatusTone, pod_facts_from_value, pod_to_row, printer_for};
use serde_json::Value;

const FIXTURES: [&str; 5] = [
    "running_replicaset",
    "pending_unschedulable",
    "terminating",
    "crashloop",
    "daemonset_with_sidecar",
];

fn raw(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn json_path_matches_typed_mapping_for_every_fixture() {
    for name in FIXTURES {
        let value = raw(name);
        let typed = pod_to_row(&serde_json::from_value(value.clone()).unwrap());
        let from_json = PodRow::from_facts(pod_facts_from_value(&value));
        assert_eq!(from_json, typed, "fixture {name}");
    }
}

#[test]
fn pod_printer_builds_the_freelens_columns() {
    let printer = printer_for(&kinds::POD);
    let titles: Vec<_> = printer.columns().iter().map(|c| c.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "Name",
            "Warning",
            "Namespace",
            "Containers",
            "CPU",
            "Memory",
            "Restarts",
            "Controlled By",
            "Age",
            "Status"
        ]
    );
    let row = printer.row(&raw("running_replicaset"));
    assert_eq!(row.cells[2].text, "Running");
    assert_eq!(row.cells[2].tone, Some(StatusTone::Ok));
    assert_eq!(row.cells[0].squares.len(), 2);
    let owner = row.owner.expect("controller owner");
    assert_eq!(owner.kind, "ReplicaSet");
    assert!(
        !owner.name.is_empty(),
        "owner name comes from the reference"
    );

    let crash = printer.row(&raw("crashloop"));
    assert_eq!(crash.health, StatusTone::Error);
}
