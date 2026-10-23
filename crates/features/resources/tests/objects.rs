use natikube_core::NamespaceFilter;
use natikube_resources::{
    Cell, ColumnKind, ColumnSpec, ObjectRow, ObjectTable, RowEvent, SortDirection, StatusTone,
    Usage, UsageMap, pod_object_row, usage_key,
};
use natikube_resources::{PodRow, synthetic_rows};

fn columns() -> Vec<ColumnSpec> {
    vec![
        ColumnSpec::new("name", "Name", ColumnKind::Name, 200.0),
        ColumnSpec::new("namespace", "Namespace", ColumnKind::Namespace, 120.0),
        ColumnSpec::new("size", "Size", ColumnKind::Cell(0), 80.0),
        ColumnSpec::new("age", "Age", ColumnKind::Age, 80.0),
        ColumnSpec::new("cpu", "CPU", ColumnKind::Cpu, 80.0),
    ]
}

fn row(ns: &str, name: &str, size: f64, created: i64) -> ObjectRow {
    let mut row = ObjectRow::new(Some(ns.to_string()), name);
    row.created_at = Some(created);
    row.cells = vec![Cell::number(size, format!("{size}"))];
    row
}

fn names(table: &ObjectTable) -> Vec<String> {
    (0..table.len())
        .map(|i| {
            let r = table.row(i).unwrap();
            format!("{}/{}", r.namespace.clone().unwrap_or_default(), r.name)
        })
        .collect()
}

fn table() -> ObjectTable {
    let mut t = ObjectTable::new(columns());
    t.apply(RowEvent::Restarted(vec![
        row("b", "two", 10.0, 300),
        row("a", "zed", 2.0, 100),
        row("a", "alpha", 30.0, 200),
    ]));
    t
}

#[test]
fn default_order_is_namespace_then_name() {
    assert_eq!(names(&table()), ["a/alpha", "a/zed", "b/two"]);
}

#[test]
fn applied_and_deleted_events_update_rows() {
    let mut t = table();
    assert!(t.apply(RowEvent::Applied(row("a", "beta", 1.0, 50))));
    assert!(!t.apply(RowEvent::Applied(row("a", "beta", 1.0, 50))));
    assert_eq!(names(&t), ["a/alpha", "a/beta", "a/zed", "b/two"]);
    assert!(t.apply(RowEvent::Deleted {
        namespace: Some("a".into()),
        name: "alpha".into()
    }));
    assert!(!t.apply(RowEvent::Deleted {
        namespace: Some("a".into()),
        name: "alpha".into()
    }));
    assert_eq!(t.total(), 3);
}

#[test]
fn namespace_filter_and_search_narrow_the_visible_rows() {
    let mut t = table();
    t.set_namespaces(NamespaceFilter::Only(["a".to_string()].into()));
    assert_eq!(names(&t), ["a/alpha", "a/zed"]);
    assert_eq!(t.total(), 3);
    t.set_search("ZE");
    assert_eq!(names(&t), ["a/zed"]);
    t.set_namespaces(NamespaceFilter::All);
    t.set_search("two");
    assert_eq!(names(&t), ["b/two"]);
    t.set_search("");
    assert_eq!(t.len(), 3);
}

#[test]
fn sorting_by_text_number_and_age() {
    let mut t = table();
    // The Name column sorts by name only (zed, two, alpha), not namespace.
    t.set_sort(Some((0, SortDirection::Descending)));
    assert_eq!(names(&t), ["a/zed", "b/two", "a/alpha"]);
    t.set_sort(Some((2, SortDirection::Ascending)));
    assert_eq!(names(&t), ["a/zed", "b/two", "a/alpha"]);
    t.set_sort(Some((2, SortDirection::Descending)));
    assert_eq!(names(&t), ["a/alpha", "b/two", "a/zed"]);
    // Age ascending means youngest first (largest creation time).
    t.set_sort(Some((3, SortDirection::Ascending)));
    assert_eq!(names(&t), ["b/two", "a/alpha", "a/zed"]);
    t.set_sort(None);
    assert_eq!(names(&t), ["a/alpha", "a/zed", "b/two"]);
}

#[test]
fn sorting_by_usage_puts_rows_without_metrics_last() {
    let mut t = table();
    let mut usage = UsageMap::default();
    usage.insert(
        usage_key(Some("a"), "zed"),
        Usage {
            cpu_cores: 0.5,
            memory_bytes: 10,
        },
    );
    usage.insert(
        usage_key(Some("b"), "two"),
        Usage {
            cpu_cores: 0.1,
            memory_bytes: 20,
        },
    );
    t.set_usage(usage);
    t.set_sort(Some((4, SortDirection::Descending)));
    assert_eq!(names(&t), ["a/zed", "b/two", "a/alpha"]);
    t.set_sort(Some((4, SortDirection::Ascending)));
    assert_eq!(names(&t), ["b/two", "a/zed", "a/alpha"]);
}

#[test]
fn pod_rows_convert_with_containers_status_and_owner() {
    let mut pods = synthetic_rows(4, 1_000_000);
    let pod: PodRow = pods.remove(1);
    let row = pod_object_row(&pod);
    assert_eq!(row.name, pod.name);
    assert_eq!(row.namespace.as_deref(), Some(pod.namespace.as_str()));
    assert_eq!(row.created_at, pod.created_at);
    // cells: containers squares, restarts, status
    assert_eq!(row.cells.len(), 3);
    assert_eq!(row.cells[0].squares.len(), pod.containers.len());
    assert_eq!(row.cells[1].text, pod.restarts.to_string());
    assert_eq!(row.cells[2].text, pod.status);
    assert_eq!(row.cells[2].tone, Some(StatusTone::Ok));
    assert_eq!(
        row.owner.as_ref().map(|o| o.kind.as_str()),
        pod.controlled_by.as_deref()
    );
}
