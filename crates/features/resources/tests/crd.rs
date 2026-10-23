use natikube_resources::{crd_from_value, crds_from_list_json};
use serde_json::json;

#[test]
fn crd_yields_kind_scope_and_printer_columns_of_the_storage_version() {
    let crd = json!({
        "metadata": {"name": "widgets.example.io"},
        "spec": {
            "group": "example.io",
            "scope": "Namespaced",
            "names": {"kind": "Widget", "plural": "widgets"},
            "versions": [
                {"name": "v1alpha1", "served": true, "storage": false,
                 "additionalPrinterColumns": [{"name": "Old", "type": "string", "jsonPath": ".spec.old"}]},
                {"name": "v1", "served": true, "storage": true,
                 "additionalPrinterColumns": [
                    {"name": "Replicas", "type": "integer", "jsonPath": ".spec.replicas"},
                    {"name": "Age", "type": "date", "jsonPath": ".metadata.creationTimestamp"}
                 ]}
            ]
        }
    });
    let info = crd_from_value(&crd).expect("valid crd");
    assert_eq!(info.kind.group, "example.io");
    assert_eq!(info.kind.version, "v1");
    assert_eq!(info.kind.kind, "Widget");
    assert_eq!(info.kind.plural, "widgets");
    assert!(info.kind.namespaced);
    assert_eq!(info.columns.len(), 2);
    assert_eq!(info.columns[0].name, "Replicas");
    assert_eq!(info.columns[0].json_path, ".spec.replicas");
}

#[test]
fn cluster_scoped_and_incomplete_crds() {
    let cluster = json!({
        "spec": {"group": "g.io", "scope": "Cluster",
                 "names": {"kind": "Thing", "plural": "things"},
                 "versions": [{"name": "v1beta1", "served": true, "storage": true}]}
    });
    let info = crd_from_value(&cluster).unwrap();
    assert!(!info.kind.namespaced);
    assert!(info.columns.is_empty());
    assert!(crd_from_value(&json!({"spec": {"group": "g.io"}})).is_none());
}

fn widget(name: &str, schema: bool) -> serde_json::Value {
    let mut version = json!({"name": "v1", "served": true, "storage": true,
        "additionalPrinterColumns": [{"name": "Size", "type": "integer", "jsonPath": ".spec.size"}]});
    if schema {
        version["schema"] = json!({"openAPIV3Schema": {"type": "object",
            "properties": {"spec": {"type": "object", "description": "x".repeat(2_000)}}}});
    }
    json!({
        "metadata": {"name": format!("{name}s.example.io"), "managedFields": [{"manager": "a"}]},
        "spec": {"group": "example.io", "scope": "Cluster",
                 "names": {"kind": name, "plural": format!("{name}s")},
                 "versions": [version]}
    })
}

#[test]
fn crd_list_parses_like_the_value_reader_without_building_the_schema() {
    let items = [widget("Alpha", true), widget("Beta", true)];
    let list = json!({"kind": "CustomResourceDefinitionList", "items": items});
    let bytes = serde_json::to_vec(&list).unwrap();
    let parsed = crds_from_list_json(&bytes).expect("valid list");
    let expected: Vec<_> = items.iter().filter_map(crd_from_value).collect();
    assert_eq!(parsed, expected);
    assert_eq!(parsed.len(), 2);
}

#[test]
fn crd_list_skips_incomplete_entries_and_rejects_garbage() {
    let list = json!({"items": [widget("Ok", false), {"spec": {"group": "g.io"}}, {}]});
    let parsed = crds_from_list_json(&serde_json::to_vec(&list).unwrap()).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].kind.kind, "Ok");
    assert!(crds_from_list_json(b"not json").is_err());
    assert!(crds_from_list_json(b"{}").unwrap().is_empty());
}
