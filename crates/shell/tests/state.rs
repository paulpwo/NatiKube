use natikube_core::{ViewId, kinds};
use natikube_shell::{DockState, NavState};

fn pods() -> ViewId {
    ViewId::Resource(kinds::POD)
}

#[test]
fn starts_on_the_cluster_overview() {
    let nav = NavState::default();
    assert_eq!(nav.view(), &ViewId::ClusterOverview);
    assert!(nav.tabs().is_empty());
}

#[test]
fn navigating_to_a_workload_exposes_group_tabs_and_expands_it() {
    let mut nav = NavState::default();
    nav.navigate(pods());
    assert_eq!(nav.view(), &pods());
    let labels: Vec<_> = nav.tabs().iter().map(|e| e.label.as_str()).collect();
    assert_eq!(labels.len(), 9);
    assert_eq!(labels[1], "Pods");
    let ix = nav.section_index().unwrap();
    assert!(nav.is_expanded(ix));
}

#[test]
fn single_link_sections_have_no_tabs() {
    let mut nav = NavState::default();
    nav.navigate(ViewId::Resource(kinds::NODE));
    assert!(nav.tabs().is_empty());
}

#[test]
fn toggling_a_section_collapses_and_expands_it() {
    let mut nav = NavState::default();
    nav.navigate(pods());
    let ix = nav.section_index().unwrap();
    nav.toggle_section(ix);
    assert!(!nav.is_expanded(ix));
    nav.toggle_section(ix);
    assert!(nav.is_expanded(ix));
}

#[test]
fn custom_resource_views_belong_to_the_dynamic_section() {
    let mut nav = NavState::default();
    nav.navigate(ViewId::Resource(natikube_core::ResourceKind::custom(
        "example.com",
        "v1",
        "Widget",
        "widgets",
        true,
    )));
    let ix = nav.section_index().unwrap();
    assert_eq!(nav.sections()[ix].label, "Custom Resources");
}

#[test]
fn dock_opens_and_activates_new_tabs() {
    let mut dock = DockState::default();
    assert!(!dock.is_open());
    assert_eq!(dock.active(), None);
    let a = dock.push();
    assert_eq!((a, dock.active(), dock.is_open()), (0, Some(0), true));
    let b = dock.push();
    assert_eq!((b, dock.active(), dock.len()), (1, Some(1), 2));
}

#[test]
fn closing_tabs_keeps_a_valid_active_tab() {
    let mut dock = DockState::default();
    dock.push();
    dock.push();
    dock.push();
    dock.activate(2);
    dock.close(2);
    assert_eq!(dock.active(), Some(1));
    dock.close(0);
    assert_eq!(dock.active(), Some(0));
    assert_eq!(dock.len(), 1);
    dock.close(0);
    assert_eq!(dock.active(), None);
    assert!(!dock.is_open());
}

#[test]
fn closing_a_tab_before_the_active_one_shifts_the_selection() {
    let mut dock = DockState::default();
    dock.push();
    dock.push();
    dock.push();
    dock.activate(2);
    dock.close(0);
    assert_eq!(dock.active(), Some(1));
}

#[test]
fn out_of_range_operations_are_ignored() {
    let mut dock = DockState::default();
    dock.push();
    dock.activate(5);
    dock.close(5);
    assert_eq!(dock.active(), Some(0));
    assert_eq!(dock.len(), 1);
}

#[test]
fn toggle_only_works_with_tabs() {
    let mut dock = DockState::default();
    dock.toggle();
    assert!(!dock.is_open());
    dock.push();
    dock.toggle();
    assert!(!dock.is_open());
    dock.toggle();
    assert!(dock.is_open());
}

fn widget(group: &str, kind: &str, plural: &str) -> natikube_core::ResourceKind {
    natikube_core::ResourceKind::custom(group, "v1", kind, plural, true)
}

fn with_crds() -> NavState {
    let mut nav = NavState::default();
    let ix = nav.sections().iter().position(|s| s.dynamic).unwrap();
    let kinds_found = [
        widget("b.example.com", "Gadget", "gadgets"),
        widget("a.example.com", "Widget", "widgets"),
        widget("a.example.com", "Sprocket", "sprockets"),
    ];
    for kind in kinds_found {
        nav.sections_mut()[ix]
            .entries
            .push(natikube_core::SidebarEntry {
                label: kind.kind.to_string(),
                view: ViewId::Resource(kind),
            });
    }
    nav
}

#[test]
fn custom_resources_start_with_a_reachable_definitions_entry() {
    let nav = NavState::default();
    let section = nav.sections().iter().find(|s| s.dynamic).unwrap();
    assert_eq!(section.entries[0].label, "Definitions");
    assert_eq!(
        section.entries[0].view,
        ViewId::Resource(kinds::CUSTOM_RESOURCE_DEFINITION)
    );
}

#[test]
fn custom_resources_are_grouped_by_api_group_without_definitions() {
    let nav = with_crds();
    let groups = nav.custom_groups();
    let names: Vec<_> = groups.iter().map(|(g, _)| g.as_str()).collect();
    assert_eq!(names, ["a.example.com", "b.example.com"]);
    assert_eq!(groups[0].1.len(), 2);
    assert_eq!(groups[1].1.len(), 1);
}

#[test]
fn custom_resources_never_become_top_tabs() {
    let mut nav = with_crds();
    nav.navigate(ViewId::Resource(widget(
        "a.example.com",
        "Widget",
        "widgets",
    )));
    assert!(nav.tabs().is_empty());
    assert!(nav.is_group_expanded("a.example.com"));
    assert!(!nav.is_group_expanded("b.example.com"));
    nav.toggle_group("b.example.com");
    assert!(nav.is_group_expanded("b.example.com"));
}
