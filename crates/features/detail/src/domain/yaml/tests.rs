use super::*;
use natikube_core::{ClusterId, ResourceRef, kinds};
use serde_json::json;

fn pod_ref() -> ResourceRef {
    ResourceRef::new(
        ClusterId::new("c"),
        kinds::POD,
        Some("default".into()),
        "api-1",
    )
}

fn pod() -> serde_json::Value {
    json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "api-1",
            "namespace": "default",
            "managedFields": [{"manager": "kubelet"}]
        },
        "spec": {"containers": [{"name": "app", "image": "nginx"}]}
    })
}

#[test]
fn strip_removes_only_managed_fields() {
    let mut obj = pod();
    strip_managed_fields(&mut obj);
    assert!(obj["metadata"].get("managedFields").is_none());
    assert_eq!(obj["metadata"]["name"], "api-1");
}

#[test]
fn yaml_hides_managed_fields_by_default() {
    let text = to_yaml(&pod(), true);
    assert!(text.contains("name: api-1"));
    assert!(!text.contains("managedFields"));
    assert!(to_yaml(&pod(), false).contains("managedFields"));
}

#[test]
fn parse_round_trips() {
    let original = pod();
    let parsed = parse_yaml(&to_yaml(&original, false)).unwrap();
    assert_eq!(parsed, original);
}

#[test]
fn parse_reports_syntax_errors() {
    assert!(parse_yaml("a: [").is_err());
}

#[test]
fn parse_requires_a_mapping() {
    let err = parse_yaml("- a\n- b\n").unwrap_err();
    assert!(err.contains("mapping"), "{err}");
}

#[test]
fn identity_accepts_unchanged_object() {
    assert!(check_identity(&pod_ref(), &pod()).is_ok());
}

#[test]
fn identity_rejects_renames_and_moves() {
    let mut renamed = pod();
    renamed["metadata"]["name"] = json!("other");
    assert!(
        check_identity(&pod_ref(), &renamed)
            .unwrap_err()
            .contains("name")
    );

    let mut moved = pod();
    moved["metadata"]["namespace"] = json!("kube-system");
    assert!(
        check_identity(&pod_ref(), &moved)
            .unwrap_err()
            .contains("namespace")
    );

    let mut other_kind = pod();
    other_kind["kind"] = json!("Deployment");
    assert!(
        check_identity(&pod_ref(), &other_kind)
            .unwrap_err()
            .contains("kind")
    );

    let mut other_version = pod();
    other_version["apiVersion"] = json!("apps/v1");
    assert!(
        check_identity(&pod_ref(), &other_version)
            .unwrap_err()
            .contains("apiVersion")
    );
}

#[test]
fn type_meta_is_filled_in_when_the_list_response_omits_it() {
    let mut obj = json!({"metadata": {"name": "api-1"}});
    ensure_type_meta(&mut obj, &kinds::POD);
    assert_eq!(obj["apiVersion"], "v1");
    assert_eq!(obj["kind"], "Pod");

    let mut deploy = json!({"apiVersion": "apps/v1", "kind": "Deployment"});
    ensure_type_meta(&mut deploy, &kinds::DEPLOYMENT);
    assert_eq!(deploy["kind"], "Deployment");
}
