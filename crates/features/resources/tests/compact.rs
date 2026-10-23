//! Compact row storage, ingestion trimming and bounded caches (O2, O4).

use std::sync::Arc;
use std::time::Duration;

use kube::api::DynamicObject;
use kube::runtime::watcher::Event;
use natikube_core::kinds;
use natikube_resources::{
    INTERN_MAX_ENTRIES, INTERN_MAX_LEN, Interner, ObjectRow, ObjectTable, RowAssembler, RowEvent,
    Text, age_tick_interval, printer_for, strip_ingest_noise,
};

fn pod(name: &str, namespace: &str) -> DynamicObject {
    serde_json::from_value(serde_json::json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": name,
            "namespace": namespace,
            "annotations": {
                "kubectl.kubernetes.io/last-applied-configuration": "{\"huge\":true}",
                "keep": "me"
            },
            "managedFields": [{"manager": "kubectl", "fieldsV1": {"f:spec": {}}}],
            "ownerReferences": [{"kind": "ReplicaSet", "name": "web-1", "controller": true}]
        },
        "spec": {"containers": [{"name": "app"}]},
        "status": {"phase": "Running",
            "containerStatuses": [{"name": "app", "ready": true, "restartCount": 0,
                "state": {"running": {}}}]}
    }))
    .unwrap()
}

#[test]
fn ingest_trimming_drops_managed_fields_and_last_applied_only() {
    let mut object = pod("a", "default");
    strip_ingest_noise(&mut object);
    assert!(object.metadata.managed_fields.is_none());
    let annotations = object.metadata.annotations.as_ref().unwrap();
    assert!(!annotations.contains_key("kubectl.kubernetes.io/last-applied-configuration"));
    assert_eq!(annotations.get("keep").map(String::as_str), Some("me"));
    assert_eq!(object.metadata.name.as_deref(), Some("a"));
    assert_eq!(object.data["spec"]["containers"][0]["name"], "app");
}

#[test]
fn trimming_removes_an_annotation_map_left_empty() {
    let mut object: DynamicObject = serde_json::from_value(serde_json::json!({
        "metadata": {"name": "a", "annotations": {
            "kubectl.kubernetes.io/last-applied-configuration": "{}"}}
    }))
    .unwrap();
    strip_ingest_noise(&mut object);
    assert!(object.metadata.annotations.is_none());
}

#[test]
fn trimming_does_not_change_the_row() {
    let printer = Arc::new(printer_for(&kinds::POD));
    let mut a = RowAssembler::new(printer.clone());
    let mut b = RowAssembler::new(printer);
    let full = a.push(Event::Apply(pod("p", "default")));
    let mut trimmed = pod("p", "default");
    strip_ingest_noise(&mut trimmed);
    assert_eq!(full, b.push(Event::Apply(trimmed)));
}

#[test]
fn assembler_shares_repeated_strings_across_rows() {
    let mut assembler = RowAssembler::new(Arc::new(printer_for(&kinds::POD)));
    assembler.push(Event::Init);
    for i in 0..50 {
        assembler.push(Event::InitApply(pod(&format!("p{i}"), "default")));
    }
    let Some(RowEvent::Restarted(rows)) = assembler.push(Event::InitDone) else {
        panic!("expected Restarted");
    };
    let first = &rows[0];
    for row in &rows[1..] {
        assert!(
            row.namespace
                .as_ref()
                .unwrap()
                .ptr_eq(first.namespace.as_ref().unwrap())
        );
        assert!(
            row.owner
                .as_ref()
                .unwrap()
                .kind
                .ptr_eq(&first.owner.as_ref().unwrap().kind)
        );
        assert!(
            row.owner
                .as_ref()
                .unwrap()
                .name
                .ptr_eq(&first.owner.as_ref().unwrap().name)
        );
        for (a, b) in row.cells.iter().zip(&first.cells) {
            if a.text == b.text {
                assert!(a.text.ptr_eq(&b.text), "cell text {:?} not shared", a.text);
            }
        }
    }
}

#[test]
fn compacting_keeps_rows_equal() {
    let mut row = ObjectRow::new(Some("ns".into()), "n");
    row.cells = vec![natikube_resources::Cell::text("Running")];
    let before = row.clone();
    row.compact(&mut Interner::new());
    assert_eq!(row, before);
}

#[test]
fn interner_shares_equal_short_strings() {
    let mut pool = Interner::new();
    let a = pool.intern_str("Running");
    let b = pool.intern(&Text::from("Running"));
    assert!(a.ptr_eq(&b));
    assert_eq!(pool.len(), 1);
}

#[test]
fn interner_does_not_pool_long_strings() {
    let mut pool = Interner::new();
    let long = "x".repeat(INTERN_MAX_LEN + 1);
    let a = pool.intern_str(&long);
    let b = pool.intern_str(&long);
    assert_eq!(a, b);
    assert!(!a.ptr_eq(&b));
    assert!(pool.is_empty());
}

#[test]
fn interner_is_bounded() {
    let mut pool = Interner::new();
    for i in 0..INTERN_MAX_ENTRIES * 3 {
        pool.intern_str(&format!("unique-{i}"));
        assert!(pool.len() <= INTERN_MAX_ENTRIES);
    }
}

fn aged(name: &str, created_at: i64) -> ObjectRow {
    let mut row = ObjectRow::new(Some("d".into()), name);
    row.created_at = Some(created_at);
    row
}

#[test]
fn table_reports_the_newest_timestamp() {
    let mut table = ObjectTable::new(Vec::new());
    assert_eq!(table.newest_timestamp(), None);
    table.apply(RowEvent::Restarted(vec![
        aged("a", 100),
        aged("b", 900),
        aged("c", 500),
    ]));
    assert_eq!(table.newest_timestamp(), Some(900));
}

#[test]
fn age_tick_follows_the_finest_visible_unit() {
    let s = Duration::from_secs;
    // Seconds are shown up to 10 minutes of age.
    assert_eq!(age_tick_interval(Some(5)), s(1));
    assert_eq!(age_tick_interval(Some(599)), s(1));
    // Minutes up to 8 hours, hours after.
    assert_eq!(age_tick_interval(Some(600)), s(60));
    assert_eq!(age_tick_interval(Some(7 * 3600)), s(60));
    assert_eq!(age_tick_interval(Some(8 * 3600)), s(3600));
    assert_eq!(age_tick_interval(Some(10 * 86_400)), s(3600));
    // Nothing to age: sleep long, never spin.
    assert_eq!(age_tick_interval(None), s(60));
    // Clock skew (future timestamps) behaves like a fresh object.
    assert_eq!(age_tick_interval(Some(-30)), s(1));
}
