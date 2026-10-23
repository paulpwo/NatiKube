use super::*;
use natikube_core::{ClusterId, ResourceKind, ResourceRef, kinds};
use serde_json::json;

fn target(kind: ResourceKind, ns: Option<&str>, name: &str) -> ResourceRef {
    ResourceRef::new(ClusterId::new("c"), kind, ns.map(String::from), name)
}

fn apply(target: ResourceRef, dry_run: bool) -> Operation {
    Operation::Apply {
        target,
        body: json!({"kind": "Deployment"}),
        dry_run,
    }
}

#[test]
fn apply_is_a_forced_server_side_apply() {
    let p = preview(&apply(
        target(kinds::DEPLOYMENT, Some("default"), "web"),
        false,
    ))
    .unwrap();
    assert_eq!(p.method, "PATCH");
    assert_eq!(p.path, "/apis/apps/v1/namespaces/default/deployments/web");
    assert_eq!(
        p.content_type.as_deref(),
        Some("application/apply-patch+yaml")
    );
    assert!(p.query.contains("fieldManager=natikube"), "{}", p.query);
    assert!(p.query.contains("force=true"), "{}", p.query);
    assert!(!p.query.contains("dryRun"), "{}", p.query);
    assert_eq!(p.body, Some(json!({"kind": "Deployment"})));
}

#[test]
fn dry_run_apply_sets_dry_run_all() {
    let p = preview(&apply(
        target(kinds::DEPLOYMENT, Some("default"), "web"),
        true,
    ))
    .unwrap();
    assert!(p.query.contains("dryRun=All"), "{}", p.query);
    assert!(p.query.contains("fieldManager=natikube"), "{}", p.query);
}

#[test]
fn paths_cover_core_cluster_scoped_and_custom_kinds() {
    let pod = preview(&Operation::Delete {
        target: target(kinds::POD, Some("default"), "api-1"),
    })
    .unwrap();
    assert_eq!(pod.method, "DELETE");
    assert_eq!(pod.path, "/api/v1/namespaces/default/pods/api-1");

    let node = preview(&Operation::Delete {
        target: target(kinds::NODE, None, "n1"),
    })
    .unwrap();
    assert_eq!(node.path, "/api/v1/nodes/n1");

    let widget = ResourceKind::custom("example.com", "v1", "Widget", "widgets", true);
    let cr = preview(&Operation::Delete {
        target: target(widget, Some("ns"), "w"),
    })
    .unwrap();
    assert_eq!(cr.path, "/apis/example.com/v1/namespaces/ns/widgets/w");
}

#[test]
fn delete_sends_no_dry_run() {
    let p = preview(&Operation::Delete {
        target: target(kinds::POD, Some("default"), "api-1"),
    })
    .unwrap();
    assert!(!p.query.contains("dryRun"));
}

#[test]
fn merge_patch_uses_the_merge_content_type() {
    let p = preview(&Operation::MergePatch {
        target: target(kinds::DEPLOYMENT, Some("default"), "web"),
        body: json!({"spec": {"replicas": 2}}),
    })
    .unwrap();
    assert_eq!(p.method, "PATCH");
    assert_eq!(
        p.content_type.as_deref(),
        Some("application/merge-patch+json")
    );
    assert_eq!(p.body, Some(json!({"spec": {"replicas": 2}})));
    assert!(!p.query.contains("force"));
}

#[test]
fn events_are_selected_by_involved_object() {
    let p = preview(&Operation::ListEvents {
        target: target(kinds::POD, Some("default"), "api-1"),
    })
    .unwrap();
    assert_eq!(p.method, "GET");
    assert_eq!(p.path, "/api/v1/namespaces/default/events");
    let query = p.query.replace("%3D", "=").replace("%2C", ",");
    assert!(query.contains("involvedObject.name=api-1"), "{query}");
    assert!(query.contains("involvedObject.kind=Pod"), "{query}");

    let node = preview(&Operation::ListEvents {
        target: target(kinds::NODE, None, "n1"),
    })
    .unwrap();
    assert_eq!(node.path, "/api/v1/events");
}
