use std::collections::HashSet;

use natikube_core::kinds;
use natikube_resources::{
    Capacity, ColumnKind, DEMO_NAMESPACES, StatusTone, demo_objects, demo_rows, demo_usage,
    printer_for,
};

const NOW: i64 = 1_800_000_000;

fn pod_status(row: &natikube_resources::ObjectRow) -> String {
    row.cells[2].text.to_string()
}

#[test]
fn pods_cover_the_demo_namespaces_with_a_realistic_mix() {
    let rows = demo_rows(&kinds::POD, NOW);
    assert!((26..=36).contains(&rows.len()), "got {}", rows.len());
    let namespaces: HashSet<String> = rows
        .iter()
        .map(|r| r.namespace.as_ref().unwrap().to_string())
        .collect();
    for ns in DEMO_NAMESPACES {
        assert!(namespaces.contains(ns), "no pods in {ns}");
    }
    let count = |status: &str| rows.iter().filter(|r| pod_status(r) == status).count();
    assert_eq!(count("CrashLoopBackOff"), 1);
    assert_eq!(count("Pending"), 1);
    assert_eq!(count("Running"), rows.len() - 2);
    let crash = rows
        .iter()
        .find(|r| pod_status(r) == "CrashLoopBackOff")
        .unwrap();
    assert!(crash.cells[1].number.unwrap() >= 10.0);
    assert_eq!(crash.health, StatusTone::Error);
}

#[test]
fn pod_names_are_unique_and_owners_are_set() {
    let rows = demo_rows(&kinds::POD, NOW);
    let names: HashSet<_> = rows
        .iter()
        .map(|r| (r.namespace.clone(), r.name.clone()))
        .collect();
    assert_eq!(names.len(), rows.len());
    let owners: HashSet<String> = rows
        .iter()
        .map(|r| {
            r.owner
                .as_ref()
                .expect("every demo pod has an owner")
                .kind
                .to_string()
        })
        .collect();
    for kind in ["ReplicaSet", "StatefulSet", "DaemonSet"] {
        assert!(owners.contains(kind), "no {kind}-owned pod");
    }
    assert!(
        rows.iter()
            .any(|r| r.name == "checkout-api-7d9f8c6b5-x2kqp")
    );
    assert!(rows.iter().any(|r| r.name == "prometheus-0"));
}

#[test]
fn every_running_pod_has_usage_and_pending_has_none() {
    let usage = demo_usage();
    for row in demo_rows(&kinds::POD, NOW) {
        let found = usage.for_object(row.namespace.as_deref(), &row.name);
        if pod_status(&row) == "Pending" {
            assert!(found.is_none());
        } else {
            let u = found.unwrap_or_else(|| panic!("no usage for {}", row.name));
            assert!(u.cpu_cores > 0.0 && u.memory_bytes > 0);
        }
    }
}

#[test]
fn nodes_have_capacity_and_usage_below_it() {
    let rows = demo_rows(&kinds::NODE, NOW);
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["demo-node-1", "demo-node-2", "demo-node-3"]);
    let usage = demo_usage();
    for row in &rows {
        let Capacity {
            cpu_cores,
            memory_bytes,
        } = row.capacity.expect("capacity");
        let used = usage.get(&row.name).expect("node usage");
        assert!(used.cpu_cores < cpu_cores && used.memory_bytes < memory_bytes);
    }
}

#[test]
fn rows_match_their_printer_columns() {
    for kind in kinds::all_builtin() {
        let printer = printer_for(&kind);
        let data_columns = printer
            .columns()
            .iter()
            .filter_map(|c| match c.kind {
                ColumnKind::Cell(ix) => Some(ix + 1),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        for row in demo_rows(&kind, NOW) {
            assert!(
                row.cells.len() >= data_columns,
                "{} {}",
                kind.plural,
                row.name
            );
        }
    }
}

#[test]
fn workloads_services_and_events_are_populated() {
    for kind in [
        kinds::DEPLOYMENT,
        kinds::STATEFUL_SET,
        kinds::DAEMON_SET,
        kinds::REPLICA_SET,
        kinds::SERVICE,
        kinds::INGRESS,
        kinds::CONFIG_MAP,
        kinds::NAMESPACE,
        kinds::EVENT,
    ] {
        assert!(
            !demo_objects(&kind, NOW).is_empty(),
            "{} empty",
            kind.plural
        );
    }
    let events = demo_rows(&kinds::EVENT, NOW);
    assert!(events.iter().any(|e| e.health == StatusTone::Warning));
    assert!(events.iter().all(|e| e.updated_at.is_some()));
    let deployments = demo_rows(&kinds::DEPLOYMENT, NOW);
    assert!(deployments.iter().any(|d| d.health == StatusTone::Warning));
}

#[test]
fn demo_data_names_nothing_real() {
    let mut text = String::new();
    for kind in kinds::all_builtin() {
        text.push_str(&serde_json::to_string(&demo_objects(&kind, NOW)).unwrap());
    }
    for forbidden in ["k3s", "amazonaws", "arn:", "ip-10-", "websocket"] {
        assert!(
            !text.to_lowercase().contains(forbidden),
            "found {forbidden}"
        );
    }
}
