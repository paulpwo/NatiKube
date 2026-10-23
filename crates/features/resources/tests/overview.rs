use natikube_core::kinds;
use natikube_resources::{
    ObjectRow, StatusTone, Usage, UsageMap, count_by_health, parse_metrics, printer_for,
    summarize_cluster, usage_key,
};
use serde_json::json;

fn with_health(name: &str, health: StatusTone) -> ObjectRow {
    let mut row = ObjectRow::new(Some("d".into()), name);
    row.health = health;
    row
}

#[test]
fn counts_group_rows_by_health() {
    let rows = vec![
        with_health("a", StatusTone::Ok),
        with_health("b", StatusTone::Ok),
        with_health("c", StatusTone::Warning),
        with_health("d", StatusTone::Error),
        with_health("e", StatusTone::Done),
    ];
    let counts = count_by_health(&rows);
    assert_eq!(
        (counts.ok, counts.warning, counts.error, counts.done),
        (2, 1, 1, 1)
    );
    assert_eq!(counts.total(), 5);
}

#[test]
fn pod_and_node_metrics_parse_into_usage() {
    let pods = json!({"items": [
        {"metadata": {"name": "api-1", "namespace": "default"},
         "containers": [
            {"name": "a", "usage": {"cpu": "250m", "memory": "64Mi"}},
            {"name": "b", "usage": {"cpu": "1500000n", "memory": "1024Ki"}}
         ]}
    ]});
    let usage = parse_metrics(&pods);
    let u = usage.get(&usage_key(Some("default"), "api-1")).unwrap();
    assert!((u.cpu_cores - 0.2515).abs() < 1e-9, "{}", u.cpu_cores);
    assert_eq!(u.memory_bytes, 65 * 1024 * 1024);

    let nodes = json!({"items": [
        {"metadata": {"name": "n1"}, "usage": {"cpu": "2", "memory": "4Gi"}}
    ]});
    let usage = parse_metrics(&nodes);
    let u = usage.get(&usage_key(None, "n1")).unwrap();
    assert_eq!(u.cpu_cores, 2.0);
    assert_eq!(u.memory_bytes, 4 * 1024 * 1024 * 1024);
}

#[test]
fn malformed_metrics_yield_an_empty_map() {
    assert!(parse_metrics(&json!({"kind": "Status", "code": 404})).is_empty());
}

#[test]
fn cluster_summary_totals_capacity_usage_and_warnings() {
    let node_printer = printer_for(&kinds::NODE);
    let mk = |name: &str, ready: &str| {
        node_printer.row(&json!({
            "metadata": {"name": name, "creationTimestamp": "2026-01-01T00:00:00Z"},
            "status": {
                "allocatable": {"cpu": "4", "memory": "8Gi"},
                "conditions": [{"type": "Ready", "status": ready}]
            }
        }))
    };
    let nodes = vec![mk("n1", "True"), mk("n2", "False")];
    let mut usage = UsageMap::default();
    usage.insert(
        usage_key(None, "n1"),
        Usage {
            cpu_cores: 1.0,
            memory_bytes: 1 << 30,
        },
    );

    let event_printer = printer_for(&kinds::EVENT);
    let event = |name: &str, kind: &str, last: &str| {
        event_printer.row(&json!({
            "metadata": {"name": name, "namespace": "d", "creationTimestamp": last},
            "type": kind, "reason": "R", "message": format!("msg {name}"), "count": 1,
            "involvedObject": {"kind": "Pod", "name": "p"},
            "lastTimestamp": last
        }))
    };
    let events = vec![
        event("old", "Warning", "2026-01-01T00:00:00Z"),
        event("fine", "Normal", "2026-01-03T00:00:00Z"),
        event("new", "Warning", "2026-01-02T00:00:00Z"),
    ];

    let summary = summarize_cluster(&nodes, Some(&usage), &events, 5);
    assert_eq!((summary.nodes_total, summary.nodes_ready), (2, 1));
    assert_eq!(summary.cpu_capacity, 8.0);
    assert_eq!(summary.memory_capacity, 16 * (1 << 30));
    assert_eq!(summary.cpu_usage, Some(1.0));
    assert_eq!(summary.memory_usage, Some(1 << 30));
    let warnings: Vec<_> = summary
        .warnings
        .iter()
        .map(|w| w.message.as_str())
        .collect();
    assert_eq!(warnings, ["msg new", "msg old"]);

    let no_metrics = summarize_cluster(&nodes, None, &events, 1);
    assert_eq!(no_metrics.cpu_usage, None);
    assert_eq!(no_metrics.warnings.len(), 1);
}
