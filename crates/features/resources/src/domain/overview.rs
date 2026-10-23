use super::object::ObjectRow;
use super::status::StatusTone;
use super::usage::UsageMap;

/// Rows by health, for the Workloads overview.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatusCounts {
    pub ok: usize,
    pub warning: usize,
    pub error: usize,
    /// Completed or scaled to zero.
    pub done: usize,
}

impl StatusCounts {
    pub fn total(&self) -> usize {
        self.ok + self.warning + self.error + self.done
    }
}

pub fn count_by_health<'a>(rows: impl IntoIterator<Item = &'a ObjectRow>) -> StatusCounts {
    let mut counts = StatusCounts::default();
    for row in rows {
        match row.health {
            StatusTone::Ok => counts.ok += 1,
            StatusTone::Warning => counts.warning += 1,
            StatusTone::Error => counts.error += 1,
            StatusTone::Done => counts.done += 1,
        }
    }
    counts
}

/// A warning event shown on the cluster overview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarningEvent {
    pub namespace: Option<String>,
    /// `Kind/name` of the involved object.
    pub object: String,
    pub message: String,
    pub updated_at: Option<i64>,
}

/// Node totals, capacity versus usage and recent warnings.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterSummary {
    pub nodes_total: usize,
    pub nodes_ready: usize,
    pub cpu_capacity: f64,
    pub memory_capacity: u64,
    /// `None` when metrics-server is not available.
    pub cpu_usage: Option<f64>,
    pub memory_usage: Option<u64>,
    /// Newest first, at most `max_warnings`.
    pub warnings: Vec<WarningEvent>,
}

pub fn summarize_cluster(
    nodes: &[ObjectRow],
    usage: Option<&UsageMap>,
    events: &[ObjectRow],
    max_warnings: usize,
) -> ClusterSummary {
    let nodes_ready = nodes
        .iter()
        .filter(|n| matches!(n.health, StatusTone::Ok | StatusTone::Warning))
        .count();
    let cpu_capacity = nodes
        .iter()
        .filter_map(|n| n.capacity)
        .map(|c| c.cpu_cores)
        .sum();
    let memory_capacity = nodes
        .iter()
        .filter_map(|n| n.capacity)
        .map(|c| c.memory_bytes)
        .sum();
    let (cpu_usage, memory_usage) = match usage.filter(|u| !u.is_empty()) {
        Some(map) => (
            Some(map.values().map(|u| u.cpu_cores).sum()),
            Some(map.values().map(|u| u.memory_bytes).sum()),
        ),
        None => (None, None),
    };

    let mut warnings: Vec<&ObjectRow> = events
        .iter()
        .filter(|e| e.health == StatusTone::Warning)
        .collect();
    warnings.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.name.cmp(&b.name))
    });
    let warnings = warnings
        .into_iter()
        .take(max_warnings)
        .map(|e| WarningEvent {
            namespace: e.namespace.as_deref().map(str::to_string),
            object: e
                .owner
                .as_ref()
                .map(|o| format!("{}/{}", o.kind, o.name))
                .unwrap_or_default(),
            message: e.note.as_deref().unwrap_or_default().to_string(),
            updated_at: e.updated_at,
        })
        .collect();

    ClusterSummary {
        nodes_total: nodes.len(),
        nodes_ready,
        cpu_capacity,
        memory_capacity,
        cpu_usage,
        memory_usage,
        warnings,
    }
}
