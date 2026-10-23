//! Pure palette model: item building, fuzzy scoring and ranking.

use natikube_core::{AppCommand, ClusterId, ClusterInfo, ResourceKind, ResourceRef};

/// What the palette can offer. The shell refreshes it with
/// [`PaletteView::set_context`](crate::PaletteView::set_context) whenever the
/// catalog, active cluster or custom resources change.
#[derive(Debug, Clone, Default)]
pub struct PaletteContext {
    pub clusters: Vec<ClusterInfo>,
    pub active_cluster: Option<ClusterId>,
    /// Built-in plus discovered custom kinds (jump to a list).
    pub kinds: Vec<ResourceKind>,
    /// Objects currently loaded in the active list (jump to a resource by name).
    pub resources: Vec<ResourceRef>,
}

/// One selectable row. Choosing it emits `command`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteItem {
    pub label: String,
    /// Right-aligned hint (`Cluster`, `Kind`, `Appearance`, source file...).
    pub hint: String,
    pub command: AppCommand,
}

mod matcher;

pub use matcher::{build_items, rank, score};
