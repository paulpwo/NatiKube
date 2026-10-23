//! Pure logic behind the appearance-specific resource views: the status
//! segmented filter (Native), namespace grouping with totals (Observatory),
//! cluster KPI cards (Observatory) and mini usage bars (Command).

use std::collections::BTreeSet;

use super::object::ObjectRow;
use super::object_table::ObjectTable;
use super::status::StatusTone;
use super::usage::UsageMap;

/// The All / Running / Problems segmented filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusFilter {
    #[default]
    All,
    /// Healthy rows (`Running` pods, ready workloads).
    Running,
    /// Warnings and errors.
    Problems,
}

impl StatusFilter {
    /// In segmented-control order.
    pub fn all() -> [Self; 3] {
        [Self::All, Self::Running, Self::Problems]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Running => "Running",
            Self::Problems => "Problems",
        }
    }

    pub fn matches(self, health: StatusTone) -> bool {
        match self {
            Self::All => true,
            Self::Running => health == StatusTone::Ok,
            Self::Problems => matches!(health, StatusTone::Warning | StatusTone::Error),
        }
    }
}

/// How many rows each [`StatusFilter`] segment would show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatusFilterCounts {
    pub all: usize,
    pub running: usize,
    pub problems: usize,
}

impl StatusFilterCounts {
    pub fn of<'a>(rows: impl IntoIterator<Item = &'a ObjectRow>) -> Self {
        let mut counts = Self::default();
        for row in rows {
            counts.all += 1;
            counts.running += usize::from(StatusFilter::Running.matches(row.health));
            counts.problems += usize::from(StatusFilter::Problems.matches(row.health));
        }
        counts
    }

    pub fn get(&self, filter: StatusFilter) -> usize {
        match filter {
            StatusFilter::All => self.all,
            StatusFilter::Running => self.running,
            StatusFilter::Problems => self.problems,
        }
    }
}

/// The visible rows of one namespace with their summed metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct NamespaceGroup {
    pub namespace: String,
    /// Indices into the table's visible rows (`ObjectTable::row`).
    pub rows: Vec<usize>,
    pub cpu_cores: f64,
    pub memory_bytes: u64,
    /// False when none of the rows has metrics (no metrics-server, or all new).
    pub has_usage: bool,
}

/// Groups the table's visible rows by namespace (alphabetical). Rows keep the
/// table's display order inside their group, so sorting still applies.
pub fn group_by_namespace(table: &ObjectTable) -> Vec<NamespaceGroup> {
    let mut groups: Vec<NamespaceGroup> = Vec::new();
    for ix in 0..table.len() {
        let Some(row) = table.row(ix) else { continue };
        let namespace = row.namespace.as_deref().unwrap_or_default().to_string();
        let group = match groups.iter().position(|g| g.namespace == namespace) {
            Some(pos) => &mut groups[pos],
            None => {
                groups.push(NamespaceGroup {
                    namespace,
                    rows: Vec::new(),
                    cpu_cores: 0.0,
                    memory_bytes: 0,
                    has_usage: false,
                });
                groups.last_mut().expect("just pushed")
            }
        };
        group.rows.push(ix);
        if let Some(usage) = table
            .usage()
            .for_object(row.namespace.as_deref(), &row.name)
        {
            group.cpu_cores += usage.cpu_cores;
            group.memory_bytes += usage.memory_bytes;
            group.has_usage = true;
        }
    }
    groups.sort_by(|a, b| a.namespace.cmp(&b.namespace));
    groups
}

/// One line of the grouped list: a section header or a pod row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupItem {
    /// Index into the groups slice.
    Header(usize),
    /// Visible-row index in the table.
    Row(usize),
}

/// Flattens groups into list lines; collapsed namespaces keep only their header.
pub fn flatten_groups(groups: &[NamespaceGroup], collapsed: &BTreeSet<String>) -> Vec<GroupItem> {
    let mut items = Vec::new();
    for (ix, group) in groups.iter().enumerate() {
        items.push(GroupItem::Header(ix));
        if !collapsed.contains(&group.namespace) {
            items.extend(group.rows.iter().map(|&row| GroupItem::Row(row)));
        }
    }
    items
}

/// Numbers of the Observatory KPI cards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClusterKpis {
    pub pods_total: usize,
    pub pods_running: usize,
    pub pods_problems: usize,
    pub nodes_total: usize,
    pub nodes_ready: usize,
    /// `None` without metrics.
    pub cpu_used: Option<f64>,
    pub memory_used: Option<u64>,
    /// Summed node allocatable; `None` when no node reported capacity.
    pub cpu_capacity: Option<f64>,
    pub memory_capacity: Option<u64>,
}

impl ClusterKpis {
    pub fn cpu_ratio(&self) -> Option<f32> {
        ratio(self.cpu_used?, self.cpu_capacity?)
    }

    pub fn memory_ratio(&self) -> Option<f32> {
        ratio(self.memory_used? as f64, self.memory_capacity? as f64)
    }
}

fn ratio(used: f64, capacity: f64) -> Option<f32> {
    (capacity > 0.0).then(|| (used / capacity).clamp(0.0, 1.0) as f32)
}

/// KPIs from every known pod, the pod metrics and the nodes (a node counts as
/// ready unless its health is an error, like the cluster overview).
pub fn cluster_kpis(pods: &[ObjectRow], usage: &UsageMap, nodes: &[ObjectRow]) -> ClusterKpis {
    let counts = StatusFilterCounts::of(pods);
    let capacities: Vec<_> = nodes.iter().filter_map(|n| n.capacity).collect();
    let metrics = !usage.is_empty();
    ClusterKpis {
        pods_total: counts.all,
        pods_running: counts.running,
        pods_problems: counts.problems,
        nodes_total: nodes.len(),
        nodes_ready: nodes
            .iter()
            .filter(|n| matches!(n.health, StatusTone::Ok | StatusTone::Warning))
            .count(),
        cpu_used: metrics.then(|| usage.values().map(|u| u.cpu_cores).sum()),
        memory_used: metrics.then(|| usage.values().map(|u| u.memory_bytes).sum()),
        cpu_capacity: (!capacities.is_empty())
            .then(|| capacities.iter().map(|c| c.cpu_cores).sum()),
        memory_capacity: (!capacities.is_empty())
            .then(|| capacities.iter().map(|c| c.memory_bytes).sum()),
    }
}

/// Fill of a mini usage bar in `0.0..=1.0`: `value` relative to `max`. Any
/// non-zero value stays visible (at least 4%).
pub fn bar_fill(value: f64, max: f64) -> f32 {
    if value <= 0.0 || max <= 0.0 {
        return 0.0;
    }
    ((value / max) as f32).clamp(0.04, 1.0)
}
