use natikube_core::{ViewId, kinds, sidebar};

#[test]
fn sections_follow_the_freelens_order() {
    let labels: Vec<_> = sidebar().iter().map(|s| s.label).collect();
    assert_eq!(
        labels,
        [
            "Cluster",
            "Nodes",
            "Workloads",
            "Config",
            "Network",
            "Storage",
            "Namespaces",
            "Events",
            "Access Control",
            "Custom Resources",
        ]
    );
}

#[test]
fn workloads_section_has_the_expected_tabs() {
    let sections = sidebar();
    let workloads = sections.iter().find(|s| s.label == "Workloads").unwrap();
    let labels: Vec<_> = workloads.entries.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Overview",
            "Pods",
            "Deployments",
            "Daemon Sets",
            "Stateful Sets",
            "Replica Sets",
            "Replication Controllers",
            "Jobs",
            "Cron Jobs",
        ]
    );
    assert_eq!(workloads.entries[0].view, ViewId::WorkloadsOverview);
    assert_eq!(workloads.entries[1].view, ViewId::Resource(kinds::POD));
    assert!(workloads.is_group());
}

#[test]
fn single_link_sections_are_not_groups() {
    let sections = sidebar();
    let nodes = sections.iter().find(|s| s.label == "Nodes").unwrap();
    assert!(!nodes.is_group());
    assert_eq!(nodes.landing(), Some(ViewId::Resource(kinds::NODE)));
    let cluster = sections.iter().find(|s| s.label == "Cluster").unwrap();
    assert_eq!(cluster.landing(), Some(ViewId::ClusterOverview));
}

#[test]
fn custom_resources_section_is_dynamic_and_starts_with_definitions() {
    let sections = sidebar();
    let crs = sections.last().unwrap();
    assert_eq!(crs.label, "Custom Resources");
    assert!(crs.dynamic);
    assert_eq!(crs.entries.len(), 1);
    assert_eq!(crs.entries[0].label, "Definitions");
    assert_eq!(
        crs.landing(),
        Some(ViewId::Resource(kinds::CUSTOM_RESOURCE_DEFINITION))
    );
}

#[test]
fn every_builtin_kind_but_crd_support_kinds_is_reachable() {
    let mut reachable = Vec::new();
    for section in sidebar() {
        for entry in section.entries {
            if let ViewId::Resource(kind) = entry.view {
                reachable.push(kind);
            }
        }
    }
    for kind in [
        kinds::POD,
        kinds::NODE,
        kinds::SECRET,
        kinds::INGRESS,
        kinds::PERSISTENT_VOLUME_CLAIM,
        kinds::NAMESPACE,
        kinds::EVENT,
        kinds::CLUSTER_ROLE_BINDING,
        kinds::VALIDATING_WEBHOOK_CONFIGURATION,
    ] {
        assert!(reachable.contains(&kind), "{} missing", kind.kind);
    }
}

#[test]
fn section_lookup_by_view() {
    let sections = sidebar();
    let pods = ViewId::Resource(kinds::POD);
    let ix = natikube_core::section_index(&sections, &pods).unwrap();
    assert_eq!(sections[ix].label, "Workloads");
    let custom = ViewId::Resource(natikube_core::ResourceKind::custom(
        "example.com",
        "v1",
        "Widget",
        "widgets",
        true,
    ));
    assert_eq!(
        natikube_core::section_index(&sections, &custom),
        Some(sections.len() - 1)
    );
}

#[test]
fn titles() {
    assert_eq!(ViewId::ClusterOverview.title(), "Cluster");
    assert_eq!(ViewId::WorkloadsOverview.title(), "Overview");
    assert_eq!(ViewId::Resource(kinds::POD).title(), "Pods");
    assert_eq!(ViewId::Resource(kinds::DAEMON_SET).title(), "Daemon Sets");
}
