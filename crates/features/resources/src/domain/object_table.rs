use std::cmp::Ordering;

use natikube_core::NamespaceFilter;

use super::appearance::{StatusFilter, StatusFilterCounts};
use super::object::{ColumnKind, ColumnSpec, ObjectRow, RowEvent};
use super::usage::UsageMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// The objects currently known for one kind, with the namespace filter,
/// search text and sort order applied. Base order is namespace then name (what
/// `kubectl get -A` prints).
#[derive(Debug, Clone)]
pub struct ObjectTable {
    columns: Vec<ColumnSpec>,
    rows: Vec<ObjectRow>,
    visible: Vec<usize>,
    filter: NamespaceFilter,
    search: String,
    sort: Option<(usize, SortDirection)>,
    usage: UsageMap,
    /// Largest CPU / memory of any pod in `usage`, the scale of the mini bars.
    peak: (f64, u64),
    status: StatusFilter,
}

fn key_cmp(a: &ObjectRow, b: &ObjectRow) -> Ordering {
    a.namespace
        .as_deref()
        .unwrap_or("")
        .cmp(b.namespace.as_deref().unwrap_or(""))
        .then_with(|| a.name.cmp(&b.name))
}

fn health_rank(row: &ObjectRow) -> u8 {
    match row.health {
        super::status::StatusTone::Error => 3,
        super::status::StatusTone::Warning => 2,
        super::status::StatusTone::Done => 1,
        super::status::StatusTone::Ok => 0,
    }
}

impl ObjectTable {
    pub fn new(columns: Vec<ColumnSpec>) -> Self {
        Self {
            columns,
            rows: Vec::new(),
            visible: Vec::new(),
            filter: NamespaceFilter::All,
            search: String::new(),
            sort: None,
            usage: UsageMap::default(),
            peak: (0.0, 0),
            status: StatusFilter::All,
        }
    }

    pub fn columns(&self) -> &[ColumnSpec] {
        &self.columns
    }

    /// Rows passing the filters (after search and namespace).
    pub fn len(&self) -> usize {
        self.visible.len()
    }

    pub fn is_empty(&self) -> bool {
        self.visible.is_empty()
    }

    /// All rows known, regardless of filters.
    pub fn total(&self) -> usize {
        self.rows.len()
    }

    /// The `visible_ix`-th row in display order.
    pub fn row(&self, visible_ix: usize) -> Option<&ObjectRow> {
        self.visible.get(visible_ix).map(|&i| &self.rows[i])
    }

    /// All rows known, in base order, ignoring search but not the namespace
    /// filter. Used by overviews that count by status.
    pub fn namespace_filtered(&self) -> impl Iterator<Item = &ObjectRow> {
        self.rows.iter().filter(|r| self.in_namespaces(r))
    }

    pub fn all_rows(&self) -> &[ObjectRow] {
        &self.rows
    }

    /// The most recent creation or update time of any row (unix seconds).
    /// Drives how often relative ages need redrawing.
    pub fn newest_timestamp(&self) -> Option<i64> {
        self.rows
            .iter()
            .flat_map(|r| [r.created_at, r.updated_at])
            .flatten()
            .max()
    }

    pub fn usage(&self) -> &UsageMap {
        &self.usage
    }

    /// Largest CPU (cores) and memory (bytes) in the current metrics.
    pub fn usage_peak(&self) -> (f64, u64) {
        self.peak
    }

    pub fn status_filter(&self) -> StatusFilter {
        self.status
    }

    /// What each status segment would show, given the namespace filter and
    /// search but not the status filter itself.
    pub fn status_filter_counts(&self) -> StatusFilterCounts {
        StatusFilterCounts::of(
            self.rows
                .iter()
                .filter(|r| self.in_namespaces(r) && self.matches_search(r)),
        )
    }

    pub fn set_status_filter(&mut self, status: StatusFilter) {
        if self.status != status {
            self.status = status;
            self.recompute();
        }
    }

    pub fn sort(&self) -> Option<(usize, SortDirection)> {
        self.sort
    }

    fn in_namespaces(&self, row: &ObjectRow) -> bool {
        match &row.namespace {
            Some(ns) => self.filter.matches(ns),
            None => true,
        }
    }

    fn matches_search(&self, row: &ObjectRow) -> bool {
        if self.search.is_empty() {
            return true;
        }
        let needle = self.search.as_str();
        let hit = |text: &str| text.to_lowercase().contains(needle);
        hit(&row.name)
            || row.namespace.as_deref().is_some_and(hit)
            || row.cells.iter().any(|c| hit(&c.text))
            || row
                .owner
                .as_ref()
                .is_some_and(|o| hit(&o.kind) || hit(&o.name))
    }

    /// Applies one event; returns whether the stored rows changed.
    pub fn apply(&mut self, event: RowEvent) -> bool {
        let changed = match event {
            RowEvent::Restarted(mut rows) => {
                rows.sort_by(key_cmp);
                rows.dedup_by(|a, b| key_cmp(a, b) == Ordering::Equal);
                let changed = rows != self.rows;
                self.rows = rows;
                changed
            }
            RowEvent::Applied(row) => match self.rows.binary_search_by(|p| key_cmp(p, &row)) {
                Ok(ix) if self.rows[ix] == row => false,
                Ok(ix) => {
                    self.rows[ix] = row;
                    true
                }
                Err(ix) => {
                    self.rows.insert(ix, row);
                    true
                }
            },
            RowEvent::Deleted { namespace, name } => {
                let probe = ObjectRow::new(namespace, name);
                match self.rows.binary_search_by(|p| key_cmp(p, &probe)) {
                    Ok(ix) => {
                        self.rows.remove(ix);
                        true
                    }
                    Err(_) => false,
                }
            }
        };
        if changed {
            self.recompute();
        }
        changed
    }

    pub fn set_namespaces(&mut self, filter: NamespaceFilter) {
        if self.filter != filter {
            self.filter = filter;
            self.recompute();
        }
    }

    pub fn set_search(&mut self, text: &str) {
        let search = text.trim().to_lowercase();
        if self.search != search {
            self.search = search;
            self.recompute();
        }
    }

    pub fn set_sort(&mut self, sort: Option<(usize, SortDirection)>) {
        if self.sort != sort {
            self.sort = sort;
            self.recompute();
        }
    }

    /// Replaces the metrics used by the CPU/Memory columns and their sorting.
    /// Returns whether anything changed.
    pub fn set_usage(&mut self, usage: UsageMap) -> bool {
        let changed = self.usage != usage;
        if changed {
            self.peak = usage.values().fold((0.0, 0), |(cpu, mem), u| {
                (cpu.max(u.cpu_cores), mem.max(u.memory_bytes))
            });
            self.usage = usage;
            if self.sort.is_some_and(|(ix, _)| {
                matches!(
                    self.columns.get(ix).map(|c| c.kind),
                    Some(ColumnKind::Cpu | ColumnKind::Memory)
                )
            }) {
                self.recompute();
            }
        }
        changed
    }

    fn usage_value(&self, row: &ObjectRow, kind: ColumnKind) -> Option<f64> {
        let usage = self.usage.for_object(row.namespace.as_deref(), &row.name)?;
        Some(match kind {
            ColumnKind::Cpu => usage.cpu_cores,
            _ => usage.memory_bytes as f64,
        })
    }

    /// Compares two rows by column `ix`. `None` means "no value": rows without
    /// a value sort after the others in both directions.
    fn compare(&self, ix: usize, dir: SortDirection, a: &ObjectRow, b: &ObjectRow) -> Ordering {
        let Some(column) = self.columns.get(ix) else {
            return Ordering::Equal;
        };
        let ordered = |o: Ordering| match dir {
            SortDirection::Ascending => o,
            SortDirection::Descending => o.reverse(),
        };
        // Optional numbers: missing always last.
        let optional = |x: Option<f64>, y: Option<f64>| match (x, y) {
            (Some(x), Some(y)) => ordered(x.partial_cmp(&y).unwrap_or(Ordering::Equal)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
        let text = |x: &str, y: &str| ordered(x.to_lowercase().cmp(&y.to_lowercase()));
        match column.kind {
            ColumnKind::Name => text(&a.name, &b.name),
            ColumnKind::Namespace => text(
                a.namespace.as_deref().unwrap_or(""),
                b.namespace.as_deref().unwrap_or(""),
            ),
            // Youngest first when ascending.
            ColumnKind::Age => optional(
                a.created_at.map(|t| -(t as f64)),
                b.created_at.map(|t| -(t as f64)),
            ),
            ColumnKind::LastSeen => optional(
                a.updated_at.map(|t| -(t as f64)),
                b.updated_at.map(|t| -(t as f64)),
            ),
            ColumnKind::Cpu | ColumnKind::Memory => optional(
                self.usage_value(a, column.kind),
                self.usage_value(b, column.kind),
            ),
            ColumnKind::Owner => text(
                a.owner.as_ref().map_or("", |o| o.kind.as_str()),
                b.owner.as_ref().map_or("", |o| o.kind.as_str()),
            ),
            ColumnKind::Object => text(
                &a.owner
                    .as_ref()
                    .map(|o| format!("{}/{}", o.kind, o.name))
                    .unwrap_or_default(),
                &b.owner
                    .as_ref()
                    .map(|o| format!("{}/{}", o.kind, o.name))
                    .unwrap_or_default(),
            ),
            ColumnKind::Warning => ordered(health_rank(a).cmp(&health_rank(b))),
            ColumnKind::Cell(i) => match (a.cells.get(i), b.cells.get(i)) {
                (Some(x), Some(y)) => match (x.number, y.number) {
                    (Some(nx), Some(ny)) => ordered(nx.partial_cmp(&ny).unwrap_or(Ordering::Equal)),
                    _ => text(&x.text, &y.text),
                },
                _ => Ordering::Equal,
            },
        }
    }

    fn recompute(&mut self) {
        let mut visible: Vec<usize> = (0..self.rows.len())
            .filter(|&i| {
                let row = &self.rows[i];
                self.in_namespaces(row)
                    && self.matches_search(row)
                    && self.status.matches(row.health)
            })
            .collect();
        if let Some((ix, dir)) = self.sort {
            visible.sort_by(|&a, &b| {
                self.compare(ix, dir, &self.rows[a], &self.rows[b])
                    .then_with(|| key_cmp(&self.rows[a], &self.rows[b]))
            });
        }
        self.visible = visible;
    }
}
