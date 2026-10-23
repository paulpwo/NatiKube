use natikube_core::{ViewId, kinds, sidebar};
use natikube_shell::nav_layout::{
    chips_for, palette_context, rail_active, rail_items, sidebar_groups,
};

fn pods() -> ViewId {
    ViewId::Resource(kinds::POD)
}

#[test]
fn sidebar_groups_put_single_sections_under_cluster() {
    let groups = sidebar_groups(&sidebar());
    let captions: Vec<_> = groups.iter().map(|g| g.caption.as_str()).collect();
    assert_eq!(
        captions,
        [
            "Cluster",
            "Workloads",
            "Config",
            "Network",
            "Storage",
            "Access Control"
        ]
    );
    let cluster: Vec<_> = groups[0].links.iter().map(|l| l.label.as_str()).collect();
    assert_eq!(
        cluster,
        [
            "Cluster",
            "Nodes",
            "Namespaces",
            "Events",
            "Custom Resources"
        ]
    );
    let custom = groups[0].links.last().unwrap();
    assert_eq!(
        custom.view,
        ViewId::Resource(kinds::CUSTOM_RESOURCE_DEFINITION)
    );
    assert!(groups[1].links.iter().any(|l| l.view == pods()));
}

#[test]
fn rail_items_map_to_real_views() {
    let items = rail_items(&sidebar());
    let labels: Vec<_> = items.iter().map(|i| i.label).collect();
    assert_eq!(
        labels,
        [
            "Health",
            "Workloads",
            "Network",
            "Config",
            "Storage",
            "Events"
        ]
    );
    assert_eq!(items[0].view, ViewId::ClusterOverview);
    assert_eq!(items[1].view, pods());
    assert_eq!(items[2].view, ViewId::Resource(kinds::SERVICE));
    assert_eq!(items[3].view, ViewId::Resource(kinds::CONFIG_MAP));
    assert_eq!(
        items[4].view,
        ViewId::Resource(kinds::PERSISTENT_VOLUME_CLAIM)
    );
    assert_eq!(items[5].view, ViewId::Resource(kinds::EVENT));
}

#[test]
fn rail_highlights_the_section_owning_the_view() {
    let sections = sidebar();
    let items = rail_items(&sections);
    let active = |view: &ViewId| -> Vec<&str> {
        items
            .iter()
            .filter(|i| rail_active(i, view, &sections))
            .map(|i| i.label)
            .collect()
    };
    assert_eq!(active(&ViewId::ClusterOverview), ["Health"]);
    assert_eq!(active(&pods()), ["Workloads"]);
    assert_eq!(active(&ViewId::Resource(kinds::DEPLOYMENT)), ["Workloads"]);
    assert_eq!(active(&ViewId::Resource(kinds::SECRET)), ["Config"]);
    assert_eq!(active(&ViewId::Resource(kinds::EVENT)), ["Events"]);
    assert!(active(&ViewId::Resource(kinds::NODE)).is_empty());
}

#[test]
fn chips_are_the_kinds_of_the_current_group() {
    let sections = sidebar();
    let chips = chips_for(&sections, &pods());
    let labels: Vec<_> = chips.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels.len(), 8, "overview is not a kind chip");
    assert_eq!(labels[0], "Pods");
    assert!(chips_for(&sections, &ViewId::ClusterOverview).is_empty());
    assert!(chips_for(&sections, &ViewId::Resource(kinds::NODE)).is_empty());
}

#[test]
fn palette_context_lists_every_kind_once() {
    let clusters = vec![natikube_core::ClusterInfo::from_context("dev-cluster")];
    let active = clusters[0].id.clone();
    let ctx = palette_context(&clusters, Some(&active), &sidebar(), Vec::new());
    assert_eq!(ctx.clusters, clusters);
    assert_eq!(ctx.active_cluster, Some(active));
    assert!(ctx.kinds.contains(&kinds::POD));
    assert!(ctx.kinds.contains(&kinds::CUSTOM_RESOURCE_DEFINITION));
    let mut unique = ctx.kinds.clone();
    unique.dedup();
    assert_eq!(unique.len(), ctx.kinds.len());
}

#[test]
fn palette_context_carries_the_loaded_resources() {
    let cluster = natikube_core::ClusterInfo::from_context("dev-cluster");
    let loaded = vec![natikube_core::ResourceRef::new(
        cluster.id.clone(),
        kinds::POD,
        Some("default".to_string()),
        "web-0",
    )];
    let ctx = palette_context(&[cluster], None, &sidebar(), loaded.clone());
    assert_eq!(ctx.resources, loaded);
}

#[test]
fn cluster_tooltip_shows_source_and_duplicates() {
    use natikube_shell::nav_layout::cluster_tooltip;
    let mut info = natikube_core::ClusterInfo::new(
        "prod-cluster",
        Some(std::path::PathBuf::from("/h/.kube/config.bak")),
    );
    assert_eq!(cluster_tooltip(&info), "/h/.kube/config.bak");
    info.also_in = vec![std::path::PathBuf::from("/h/.kube/config.bak2")];
    assert_eq!(
        cluster_tooltip(&info),
        "/h/.kube/config.bak\nAlso in: config.bak2"
    );
    assert!(
        cluster_tooltip(&natikube_core::ClusterInfo::from_context("dev-cluster"))
            .contains("Default")
    );
}
