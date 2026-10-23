use std::sync::Arc;

use kube::api::DynamicObject;
use kube::runtime::watcher::Event;
use natikube_core::kinds;
use natikube_resources::{RowAssembler, RowEvent, printer_for};

fn object(name: &str) -> DynamicObject {
    serde_json::from_value(serde_json::json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": {"name": name, "namespace": "default"},
        "spec": {"replicas": 1},
        "status": {"readyReplicas": 1}
    }))
    .unwrap()
}

#[test]
fn initial_listing_collapses_into_one_restart() {
    let mut assembler = RowAssembler::new(Arc::new(printer_for(&kinds::DEPLOYMENT)));
    assert!(assembler.push(Event::Init).is_none());
    assert!(assembler.push(Event::InitApply(object("a"))).is_none());
    assert!(assembler.push(Event::InitApply(object("b"))).is_none());
    match assembler.push(Event::InitDone) {
        Some(RowEvent::Restarted(rows)) => {
            let names: Vec<_> = rows.iter().map(|r| r.name.as_str()).collect();
            assert_eq!(names, ["a", "b"]);
            assert_eq!(rows[0].cells[0].text, "1/1");
        }
        other => panic!("expected Restarted, got {other:?}"),
    }
}

#[test]
fn live_events_map_to_applied_and_deleted() {
    let mut assembler = RowAssembler::new(Arc::new(printer_for(&kinds::DEPLOYMENT)));
    assert!(matches!(
        assembler.push(Event::Apply(object("a"))),
        Some(RowEvent::Applied(row)) if row.name == "a"
    ));
    assert_eq!(
        assembler.push(Event::Delete(object("a"))),
        Some(RowEvent::Deleted {
            namespace: Some("default".into()),
            name: "a".into()
        })
    );
}
