use natikube_core::{ClusterId, ResourceKind, ResourceRef, kinds};

#[test]
fn api_version_joins_group_and_version() {
    assert_eq!(kinds::POD.api_version(), "v1");
    assert_eq!(kinds::DEPLOYMENT.api_version(), "apps/v1");
    assert!(kinds::POD.is_core());
    assert!(!kinds::DEPLOYMENT.is_core());
}

#[test]
fn builtin_kinds_describe_scope() {
    let scope = |name: &str| kinds::find(name).unwrap().namespaced;
    assert!(scope("pods"));
    assert!(!scope("nodes"));
    assert!(!scope("namespaces"));
    assert!(!scope("clusterroles"));
    assert_eq!(kinds::CRON_JOB.plural, "cronjobs");
}

#[test]
fn custom_kinds_compare_equal_to_builtin_ones() {
    let custom = ResourceKind::custom("apps", "v1", "Deployment", "deployments", true);
    assert_eq!(custom, kinds::DEPLOYMENT);
}

#[test]
fn all_builtin_kinds_are_unique() {
    let all = kinds::all_builtin();
    assert!(all.len() >= 35);
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

#[test]
fn resource_ref_display() {
    let namespaced = ResourceRef::new(
        ClusterId::new("c"),
        kinds::POD,
        Some("default".into()),
        "api-1",
    );
    assert_eq!(namespaced.to_string(), "Pod default/api-1");
    let cluster_scoped = ResourceRef::new(ClusterId::new("c"), kinds::NODE, None, "node-a");
    assert_eq!(cluster_scoped.to_string(), "Node node-a");
}

#[test]
fn resource_ref_round_trips_through_json() {
    let r = ResourceRef::new(
        ClusterId::new("c"),
        kinds::DEPLOYMENT,
        Some("ns".into()),
        "web",
    );
    let json = serde_json::to_string(&r).unwrap();
    let back: ResourceRef = serde_json::from_str(&json).unwrap();
    assert_eq!(r, back);
}

#[test]
fn find_matches_plural_kind_and_lowercase_singular() {
    assert_eq!(kinds::find("pods"), Some(kinds::POD));
    assert_eq!(kinds::find("Pod"), Some(kinds::POD));
    assert_eq!(kinds::find("DEPLOYMENTS"), Some(kinds::DEPLOYMENT));
    assert_eq!(kinds::find("cronjob"), Some(kinds::CRON_JOB));
    assert_eq!(kinds::find("ingresses"), Some(kinds::INGRESS));
    assert_eq!(kinds::find("nope"), None);
}
