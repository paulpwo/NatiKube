use super::*;
use natikube_core::kinds;
use serde_json::json;

#[test]
fn scale_body_sets_spec_replicas() {
    assert_eq!(scale_body(3), json!({"spec": {"replicas": 3}}));
}

#[test]
fn restart_body_stamps_the_template_annotation() {
    assert_eq!(
        restart_body("2026-10-23T10:00:00Z"),
        json!({"spec": {"template": {"metadata": {"annotations": {
            "kubectl.kubernetes.io/restartedAt": "2026-10-23T10:00:00Z"
        }}}}})
    );
}

#[test]
fn scale_support_matches_kubectl() {
    assert!(supports_scale(&kinds::DEPLOYMENT));
    assert!(supports_scale(&kinds::STATEFUL_SET));
    assert!(supports_scale(&kinds::REPLICA_SET));
    assert!(!supports_scale(&kinds::POD));
    assert!(!supports_scale(&kinds::DAEMON_SET));
}

#[test]
fn restart_support_matches_kubectl() {
    assert!(supports_restart(&kinds::DEPLOYMENT));
    assert!(supports_restart(&kinds::DAEMON_SET));
    assert!(supports_restart(&kinds::STATEFUL_SET));
    assert!(!supports_restart(&kinds::REPLICA_SET));
    assert!(!supports_restart(&kinds::POD));
}

#[test]
fn replicas_input_is_validated() {
    assert_eq!(parse_replicas("3"), Ok(3));
    assert_eq!(parse_replicas(" 0 "), Ok(0));
    assert!(parse_replicas("").is_err());
    assert!(parse_replicas("-1").is_err());
    assert!(parse_replicas("2.5").is_err());
    assert!(parse_replicas("abc").is_err());
    assert!(parse_replicas("10001").is_err());
}
