use natikube_resources::{PodEvent, PodRow, PodTable, synthetic_rows};

fn row(namespace: &str, name: &str, status: &str) -> PodRow {
    PodRow {
        name: name.to_string(),
        namespace: namespace.to_string(),
        containers: Vec::new(),
        restarts: 0,
        controlled_by: None,
        created_at: None,
        status: status.to_string(),
    }
}

fn keys(table: &PodTable) -> Vec<String> {
    table
        .rows()
        .iter()
        .map(|r| format!("{}/{}", r.namespace, r.name))
        .collect()
}

#[test]
fn restart_replaces_everything_sorted_by_namespace_then_name() {
    let mut table = PodTable::default();
    table.apply(PodEvent::Applied(row("zzz", "old", "Running")));
    let changed = table.apply(PodEvent::Restarted(vec![
        row("kube-system", "coredns", "Running"),
        row("default", "web-b", "Running"),
        row("default", "web-a", "Pending"),
    ]));
    assert!(changed);
    assert_eq!(
        keys(&table),
        vec!["default/web-a", "default/web-b", "kube-system/coredns"]
    );
}

#[test]
fn applied_inserts_in_order_and_updates_in_place() {
    let mut table = PodTable::default();
    table.apply(PodEvent::Applied(row("b", "x", "Pending")));
    table.apply(PodEvent::Applied(row("a", "y", "Running")));
    table.apply(PodEvent::Applied(row("b", "x", "Running")));
    assert_eq!(table.len(), 2);
    assert_eq!(keys(&table), vec!["a/y", "b/x"]);
    assert_eq!(table.rows()[1].status, "Running");
}

#[test]
fn deleted_removes_only_the_matching_pod() {
    let mut table = PodTable::default();
    table.apply(PodEvent::Applied(row("a", "x", "Running")));
    table.apply(PodEvent::Applied(row("a", "y", "Running")));
    assert!(table.apply(PodEvent::Deleted {
        namespace: "a".to_string(),
        name: "x".to_string(),
    }));
    assert!(!table.apply(PodEvent::Deleted {
        namespace: "a".to_string(),
        name: "missing".to_string(),
    }));
    assert_eq!(keys(&table), vec!["a/y"]);
}

#[test]
fn identical_apply_reports_no_change() {
    let mut table = PodTable::default();
    assert!(table.apply(PodEvent::Applied(row("a", "x", "Running"))));
    assert!(!table.apply(PodEvent::Applied(row("a", "x", "Running"))));
}

#[test]
fn synthetic_rows_are_unique_and_deterministic() {
    let rows = synthetic_rows(10_000, 1_700_000_000);
    assert_eq!(rows.len(), 10_000);
    assert_eq!(rows, synthetic_rows(10_000, 1_700_000_000));
    let mut table = PodTable::default();
    table.apply(PodEvent::Restarted(rows));
    assert_eq!(table.len(), 10_000);
}
