//! Pure navigation shaping for the appearance layouts: which links each
//! sidebar, icon rail and chip row shows. No GPUI, unit-tested directly.

use natikube_core::{ClusterId, ClusterInfo, SidebarSection, ViewId, kinds, section_index};
use natikube_palette::PaletteContext;

/// One clickable link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavLink {
    pub label: String,
    pub view: ViewId,
}

/// A captioned block of links (Command and Native sidebars).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavGroup {
    pub caption: String,
    pub links: Vec<NavLink>,
}

/// One labeled button of the Observatory icon rail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RailItem {
    pub label: &'static str,
    pub view: ViewId,
}

/// Sidebar blocks: every single-entry section (Cluster, Nodes, Namespaces,
/// Events, Custom Resources) is folded under one `Cluster` caption, followed
/// by one block per multi-entry section in sidebar order. The dynamic Custom
/// Resources section is a single link to its landing view; discovered kinds
/// are reached through the palette.
pub fn sidebar_groups(sections: &[SidebarSection]) -> Vec<NavGroup> {
    let link = |label: &str, view: &ViewId| NavLink {
        label: label.to_string(),
        view: view.clone(),
    };
    let mut singles = NavGroup {
        caption: "Cluster".to_string(),
        links: Vec::new(),
    };
    let mut groups = Vec::new();
    for section in sections {
        if section.dynamic {
            if let Some(view) = section.landing() {
                singles.links.push(link(section.label, &view));
            }
        } else if section.is_group() {
            groups.push(NavGroup {
                caption: section.label.to_string(),
                links: section
                    .entries
                    .iter()
                    .map(|e| link(&e.label, &e.view))
                    .collect(),
            });
        } else if let Some(entry) = section.entries.first() {
            singles.links.push(link(&entry.label, &entry.view));
        }
    }
    let mut out = Vec::with_capacity(groups.len() + 1);
    if !singles.links.is_empty() {
        out.push(singles);
    }
    out.extend(groups);
    out
}

/// The Observatory rail: Health plus the sections it labels. Workloads lands
/// on Pods (the grouped-by-namespace list); the others on their first entry.
pub fn rail_items(sections: &[SidebarSection]) -> Vec<RailItem> {
    let landing = |label: &str| {
        sections
            .iter()
            .find(|s| s.label == label)
            .and_then(|s| s.landing())
    };
    let mut items = vec![RailItem {
        label: "Health",
        view: ViewId::ClusterOverview,
    }];
    for label in ["Workloads", "Network", "Config", "Storage", "Events"] {
        let view = if label == "Workloads" {
            Some(ViewId::Resource(kinds::POD))
        } else {
            landing(label)
        };
        if let Some(view) = view {
            items.push(RailItem { label, view });
        }
    }
    items
}

/// Whether `item` is the rail button owning `current`.
pub fn rail_active(item: &RailItem, current: &ViewId, sections: &[SidebarSection]) -> bool {
    if item.label == "Health" {
        return *current == ViewId::ClusterOverview;
    }
    section_index(sections, current)
        .map(|ix| sections[ix].label == item.label)
        .unwrap_or(false)
}

/// Kind chips: the resource entries of the group owning `current` (the
/// workloads overview is not a kind). Empty outside multi-entry groups.
pub fn chips_for(sections: &[SidebarSection], current: &ViewId) -> Vec<NavLink> {
    let Some(ix) = section_index(sections, current) else {
        return Vec::new();
    };
    let section = &sections[ix];
    if !section.is_group() || section.dynamic {
        return Vec::new();
    }
    section
        .entries
        .iter()
        .filter(|e| matches!(e.view, ViewId::Resource(_)))
        .map(|e| NavLink {
            label: e.label.clone(),
            view: e.view.clone(),
        })
        .collect()
}

/// What the ⌘K palette can offer right now. `resources` are the objects the
/// active list has loaded (jump to a resource by name).
pub fn palette_context(
    clusters: &[ClusterInfo],
    active: Option<&ClusterId>,
    sections: &[SidebarSection],
    resources: Vec<natikube_core::ResourceRef>,
) -> PaletteContext {
    let mut kinds_list = Vec::new();
    for section in sections {
        for entry in &section.entries {
            if let ViewId::Resource(kind) = &entry.view
                && !kinds_list.contains(kind)
            {
                kinds_list.push(kind.clone());
            }
        }
    }
    PaletteContext {
        clusters: clusters.to_vec(),
        active_cluster: active.cloned(),
        kinds: kinds_list,
        resources,
    }
}

/// Tooltip of a cluster: its source kubeconfig path and the other files that
/// declare the identical context. Paths only, never contents.
pub fn cluster_tooltip(info: &ClusterInfo) -> String {
    let mut text = match &info.kubeconfig {
        Some(path) => path.display().to_string(),
        None => "Default kubeconfig ($KUBECONFIG or ~/.kube/config)".to_string(),
    };
    if !info.also_in.is_empty() {
        let names: Vec<String> = info
            .also_in
            .iter()
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        text.push_str(&format!("\nAlso in: {}", names.join(", ")));
    }
    if !info.aliases.is_empty() {
        text.push_str(&format!("\nAlso known as: {}", info.aliases.join(", ")));
    }
    text
}
