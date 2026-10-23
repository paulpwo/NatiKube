use natikube_core::{ResourceKind, kinds};
use natikube_resources::{ColumnKind, PrinterColumn, StatusTone, printer_for, printer_for_custom};
use serde_json::{Value, json};

const NOW: i64 = 1_800_000_000;

fn meta(name: &str, namespace: Option<&str>) -> Value {
    let mut m = json!({"name": name, "creationTimestamp": "2026-01-01T00:00:00Z"});
    if let Some(ns) = namespace {
        m["namespace"] = json!(ns);
    }
    m
}

fn titles(kind: &ResourceKind) -> Vec<String> {
    printer_for(kind)
        .columns()
        .iter()
        .map(|c| c.title.clone())
        .collect()
}

fn cell_text(kind: &ResourceKind, object: &Value, title: &str) -> String {
    let printer = printer_for(kind);
    let row = printer.row(object);
    let col = printer
        .columns()
        .iter()
        .find(|c| c.title == title)
        .unwrap_or_else(|| panic!("no column {title}"));
    match col.kind {
        ColumnKind::Cell(ix) => row.cells[ix].text.to_string(),
        ColumnKind::Name => row.name.clone(),
        ColumnKind::Namespace => row.namespace.as_deref().unwrap_or_default().to_string(),
        _ => panic!("column {title} is not a data column"),
    }
}

#[test]
fn deployment_columns_and_row() {
    assert_eq!(
        titles(&kinds::DEPLOYMENT),
        ["Name", "Namespace", "Pods", "Replicas", "Age", "Conditions"]
    );
    let object = json!({
        "metadata": meta("api", Some("default")),
        "spec": {"replicas": 3},
        "status": {
            "readyReplicas": 2,
            "conditions": [
                {"type": "Available", "status": "True"},
                {"type": "Progressing", "status": "True"},
                {"type": "ReplicaFailure", "status": "False"}
            ]
        }
    });
    assert_eq!(cell_text(&kinds::DEPLOYMENT, &object, "Pods"), "2/3");
    assert_eq!(cell_text(&kinds::DEPLOYMENT, &object, "Replicas"), "3");
    assert_eq!(
        cell_text(&kinds::DEPLOYMENT, &object, "Conditions"),
        "Available Progressing"
    );
    let row = printer_for(&kinds::DEPLOYMENT).row(&object);
    assert_eq!(row.health, StatusTone::Warning);
    assert_eq!(row.created_at, Some(1_767_225_600));
}

#[test]
fn deployment_fully_ready_is_ok_and_scaled_to_zero_is_done() {
    let ok = json!({"metadata": meta("a", Some("d")), "spec": {"replicas": 2}, "status": {"readyReplicas": 2}});
    let zero = json!({"metadata": meta("a", Some("d")), "spec": {"replicas": 0}, "status": {}});
    let printer = printer_for(&kinds::DEPLOYMENT);
    assert_eq!(printer.row(&ok).health, StatusTone::Ok);
    assert_eq!(printer.row(&zero).health, StatusTone::Done);
}

#[test]
fn service_ports_external_ip_and_status() {
    assert_eq!(
        titles(&kinds::SERVICE),
        [
            "Name",
            "Namespace",
            "Type",
            "Cluster IP",
            "Ports",
            "External IP",
            "Selector",
            "Age",
            "Status"
        ]
    );
    let lb = json!({
        "metadata": meta("web", Some("default")),
        "spec": {
            "type": "LoadBalancer",
            "clusterIP": "10.0.0.5",
            "ports": [
                {"port": 80, "protocol": "TCP"},
                {"port": 443, "nodePort": 30443, "protocol": "TCP"}
            ],
            "selector": {"app": "web", "tier": "fe"}
        },
        "status": {"loadBalancer": {}}
    });
    assert_eq!(
        cell_text(&kinds::SERVICE, &lb, "Ports"),
        "80/TCP, 443:30443/TCP"
    );
    assert_eq!(
        cell_text(&kinds::SERVICE, &lb, "Selector"),
        "app=web, tier=fe"
    );
    assert_eq!(cell_text(&kinds::SERVICE, &lb, "External IP"), "-");
    assert_eq!(cell_text(&kinds::SERVICE, &lb, "Status"), "Pending");
    assert_eq!(
        printer_for(&kinds::SERVICE).row(&lb).health,
        StatusTone::Warning
    );

    let with_ip = json!({
        "metadata": meta("web", Some("default")),
        "spec": {"type": "LoadBalancer", "clusterIP": "10.0.0.5"},
        "status": {"loadBalancer": {"ingress": [{"ip": "1.2.3.4"}]}}
    });
    assert_eq!(
        cell_text(&kinds::SERVICE, &with_ip, "External IP"),
        "1.2.3.4"
    );
    assert_eq!(cell_text(&kinds::SERVICE, &with_ip, "Status"), "Active");
}

#[test]
fn secret_row_never_contains_values() {
    assert_eq!(
        titles(&kinds::SECRET),
        ["Name", "Namespace", "Type", "Keys", "Age"]
    );
    let object = json!({
        "metadata": meta("creds", Some("default")),
        "type": "Opaque",
        "data": {"password": "c3VwZXItc2VjcmV0LXZhbHVl", "user": "YWRtaW4="},
        "stringData": {"x": "plain-text-value"}
    });
    let row = printer_for(&kinds::SECRET).row(&object);
    let dump = format!("{row:?}");
    assert!(!dump.contains("c3VwZXI"), "base64 value leaked: {dump}");
    assert!(!dump.contains("YWRtaW4"), "base64 value leaked: {dump}");
    assert!(!dump.contains("plain-text-value"), "value leaked: {dump}");
    assert_eq!(cell_text(&kinds::SECRET, &object, "Type"), "Opaque");
    assert_eq!(cell_text(&kinds::SECRET, &object, "Keys"), "2");
}

#[test]
fn configmap_lists_keys() {
    let object = json!({"metadata": meta("cfg", Some("d")), "data": {"b": "1", "a": "2"}, "binaryData": {"c": "x"}});
    assert_eq!(cell_text(&kinds::CONFIG_MAP, &object, "Keys"), "a, b, c");
}

#[test]
fn node_roles_taints_version_conditions_and_capacity() {
    assert_eq!(
        titles(&kinds::NODE),
        [
            "Name",
            "CPU",
            "Memory",
            "Taints",
            "Roles",
            "Version",
            "Age",
            "Conditions"
        ]
    );
    let node = json!({
        "metadata": {
            "name": "n1",
            "creationTimestamp": "2026-01-01T00:00:00Z",
            "labels": {"node-role.kubernetes.io/control-plane": "", "node-role.kubernetes.io/master": "true", "kubernetes.io/os": "linux"}
        },
        "spec": {"taints": [{"key": "a", "effect": "NoSchedule"}]},
        "status": {
            "nodeInfo": {"kubeletVersion": "v1.33.1+k3s1"},
            "allocatable": {"cpu": "4", "memory": "8Gi"},
            "conditions": [
                {"type": "MemoryPressure", "status": "False"},
                {"type": "Ready", "status": "True"}
            ]
        }
    });
    assert_eq!(
        cell_text(&kinds::NODE, &node, "Roles"),
        "control-plane, master"
    );
    assert_eq!(cell_text(&kinds::NODE, &node, "Taints"), "1");
    assert_eq!(cell_text(&kinds::NODE, &node, "Version"), "v1.33.1+k3s1");
    assert_eq!(cell_text(&kinds::NODE, &node, "Conditions"), "Ready");
    let row = printer_for(&kinds::NODE).row(&node);
    assert_eq!(row.health, StatusTone::Ok);
    let capacity = row.capacity.expect("node capacity");
    assert_eq!(capacity.cpu_cores, 4.0);
    assert_eq!(capacity.memory_bytes, 8 * 1024 * 1024 * 1024);

    let not_ready = json!({"metadata": meta("n2", None), "status": {"conditions": [{"type": "Ready", "status": "False"}]}});
    let row = printer_for(&kinds::NODE).row(&not_ready);
    assert_eq!(row.health, StatusTone::Error);
    assert_eq!(
        cell_text(&kinds::NODE, &not_ready, "Conditions"),
        "NotReady"
    );
}

#[test]
fn event_columns_and_involved_object() {
    assert_eq!(
        titles(&kinds::EVENT),
        [
            "Type",
            "Message",
            "Namespace",
            "Involved Object",
            "Source",
            "Count",
            "Age",
            "Last Seen"
        ]
    );
    let event = json!({
        "metadata": meta("e1", Some("default")),
        "type": "Warning",
        "reason": "BackOff",
        "message": "Back-off restarting failed container",
        "count": 7,
        "involvedObject": {"kind": "Pod", "name": "api-1", "namespace": "default"},
        "source": {"component": "kubelet"},
        "lastTimestamp": "2026-01-02T00:00:00Z"
    });
    let printer = printer_for(&kinds::EVENT);
    let row = printer.row(&event);
    assert_eq!(row.health, StatusTone::Warning);
    let owner = row.owner.expect("involved object");
    assert_eq!((owner.kind.as_str(), owner.name.as_str()), ("Pod", "api-1"));
    assert_eq!(row.updated_at, Some(1_767_312_000));
    assert_eq!(cell_text(&kinds::EVENT, &event, "Type"), "Warning");
    assert_eq!(cell_text(&kinds::EVENT, &event, "Count"), "7");
    assert_eq!(cell_text(&kinds::EVENT, &event, "Source"), "kubelet");
}

#[test]
fn custom_resources_use_printer_columns() {
    let kind = ResourceKind::custom("example.io", "v1", "Widget", "widgets", true);
    let printer = printer_for_custom(
        &kind,
        &[
            PrinterColumn {
                name: "Replicas".into(),
                json_path: ".spec.replicas".into(),
                type_: "integer".into(),
            },
            PrinterColumn {
                name: "Phase".into(),
                json_path: ".status.phase".into(),
                type_: "string".into(),
            },
            PrinterColumn {
                name: "Age".into(),
                json_path: ".metadata.creationTimestamp".into(),
                type_: "date".into(),
            },
        ],
    );
    let names: Vec<_> = printer.columns().iter().map(|c| c.title.as_str()).collect();
    assert_eq!(names, ["Name", "Namespace", "Age", "Replicas", "Phase"]);
    let row = printer.row(&json!({
        "metadata": meta("w", Some("d")),
        "spec": {"replicas": 3},
        "status": {"phase": "Ready"}
    }));
    assert_eq!(row.cells.len(), 2);
    assert_eq!(row.cells[0].text, "3");
    assert_eq!(row.cells[1].text, "Ready");

    let cluster_scoped = ResourceKind::custom("example.io", "v1", "Thing", "things", false);
    let printer = printer_for_custom(&cluster_scoped, &[]);
    let names: Vec<_> = printer.columns().iter().map(|c| c.title.as_str()).collect();
    assert_eq!(names, ["Name", "Age"]);
}

#[test]
fn every_builtin_kind_builds_a_consistent_row_from_minimal_input() {
    for kind in kinds::all_builtin() {
        let printer = printer_for(&kind);
        let columns = printer.columns();
        assert!(
            columns.iter().any(|c| c.kind == ColumnKind::Name) || kind == kinds::EVENT,
            "{}",
            kind.kind
        );
        let object = json!({"metadata": meta("x", kind.namespaced.then_some("ns"))});
        let row = printer.row(&object);
        assert_eq!(row.name, "x");
        let data_columns = columns
            .iter()
            .filter(|c| matches!(c.kind, ColumnKind::Cell(_)))
            .count();
        assert_eq!(row.cells.len(), data_columns, "{}", kind.kind);
        for column in columns {
            if let ColumnKind::Cell(ix) = column.kind {
                assert!(ix < row.cells.len(), "{} {}", kind.kind, column.title);
            }
        }
        assert_eq!(
            columns.iter().any(|c| c.kind == ColumnKind::Namespace),
            kind.namespaced,
            "{}",
            kind.kind
        );
    }
    let _ = NOW;
}
