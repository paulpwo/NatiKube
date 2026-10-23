use std::pin::Pin;

use futures::Stream;
use natikube_core::Appearance;

use super::source::SourceError;
use super::status::StatusTone;
use super::text::{Interner, Text};

/// What a column shows. Data columns point into [`ObjectRow::cells`]; the
/// others are derived from row metadata (or live metrics) at render time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Name,
    /// Link-styled namespace.
    Namespace,
    /// Age since creation, recomputed every second.
    Age,
    /// Age since the last update (events).
    LastSeen,
    /// Live CPU usage from `metrics.k8s.io` (empty without metrics-server).
    Cpu,
    /// Live memory usage from `metrics.k8s.io`.
    Memory,
    /// Controlling owner kind, link-styled (`Controlled By`).
    Owner,
    /// `Kind/name` of the referenced object, link-styled (`Involved Object`).
    Object,
    /// A warning mark for unhealthy rows.
    Warning,
    /// `cells[index]`.
    Cell(usize),
}

/// One column of a resource table.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnSpec {
    pub key: String,
    pub title: String,
    pub kind: ColumnKind,
    pub width: f32,
    pub align_right: bool,
    pub sortable: bool,
}

impl ColumnSpec {
    pub fn new(
        key: impl Into<String>,
        title: impl Into<String>,
        kind: ColumnKind,
        width: f32,
    ) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            kind,
            width,
            align_right: false,
            sortable: true,
        }
    }

    pub fn right(mut self) -> Self {
        self.align_right = true;
        self
    }

    pub fn unsortable(mut self) -> Self {
        self.sortable = false;
        self
    }
}

/// The value of one data cell, with optional presentation hints.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cell {
    pub text: Text,
    /// Colors the text (`Running` green, `Failed` red).
    pub tone: Option<StatusTone>,
    /// Small colored squares (the Containers column) instead of text.
    pub squares: Vec<StatusTone>,
    /// Numeric value used for sorting.
    pub number: Option<f64>,
}

impl Cell {
    pub fn text(text: impl Into<Text>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    pub fn toned(text: impl Into<Text>, tone: StatusTone) -> Self {
        Self {
            text: text.into(),
            tone: Some(tone),
            ..Self::default()
        }
    }

    pub fn number(number: f64, text: impl Into<Text>) -> Self {
        Self {
            text: text.into(),
            number: Some(number),
            ..Self::default()
        }
    }

    pub fn squares(squares: Vec<StatusTone>, text: impl Into<Text>) -> Self {
        Self {
            text: text.into(),
            squares,
            ..Self::default()
        }
    }
}

/// A referenced object (controller owner, or the involved object of an event).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerRef {
    pub kind: Text,
    pub name: Text,
}

/// Allocatable resources of a node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Capacity {
    pub cpu_cores: f64,
    pub memory_bytes: u64,
}

/// One table row, already formatted for display. Rows never hold raw object
/// content (so Secret values cannot leak through them).
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectRow {
    /// `None` for cluster-scoped kinds.
    pub namespace: Option<Text>,
    pub name: String,
    /// Creation timestamp in unix seconds.
    pub created_at: Option<i64>,
    /// Last update in unix seconds (events: last seen).
    pub updated_at: Option<i64>,
    pub cells: Vec<Cell>,
    pub owner: Option<OwnerRef>,
    /// Health used for status counts and the warning mark.
    pub health: StatusTone,
    pub capacity: Option<Capacity>,
    /// Free text shown by overviews (event message).
    pub note: Option<Box<str>>,
}

impl ObjectRow {
    pub fn new(namespace: Option<String>, name: impl Into<String>) -> Self {
        Self {
            namespace: namespace.map(Text::from),
            name: name.into(),
            created_at: None,
            updated_at: None,
            cells: Vec::new(),
            owner: None,
            health: StatusTone::Ok,
            capacity: None,
            note: None,
        }
    }

    /// Makes the repeated strings of this row (namespace, cell texts, owner)
    /// share storage with the other rows seen by `pool`, and drops spare
    /// vector capacity. Equality is unaffected.
    pub fn compact(&mut self, pool: &mut Interner) {
        if let Some(namespace) = &mut self.namespace {
            *namespace = pool.intern(namespace);
        }
        for cell in &mut self.cells {
            cell.text = pool.intern(&cell.text);
            cell.squares.shrink_to_fit();
        }
        self.cells.shrink_to_fit();
        if let Some(owner) = &mut self.owner {
            owner.kind = pool.intern(&owner.kind);
            owner.name = pool.intern(&owner.name);
        }
        self.name.shrink_to_fit();
    }
}

/// A change in the set of objects a source observes.
#[derive(Debug, Clone, PartialEq)]
pub enum RowEvent {
    Applied(ObjectRow),
    Deleted {
        namespace: Option<String>,
        name: String,
    },
    /// The complete current set after a (re)listing.
    Restarted(Vec<ObjectRow>),
}

pub type RowEventStream = Pin<Box<dyn Stream<Item = Result<RowEvent, SourceError>> + Send>>;

/// Port: watches one kind across all namespaces. The first successful item is
/// a [`RowEvent::Restarted`]; errors are recoverable and the source retries.
pub trait ObjectSource: Send + Sync {
    fn watch(&self) -> RowEventStream;
}

pub type UsageStream = Pin<Box<dyn Stream<Item = super::usage::UsageMap> + Send>>;

/// Port: live CPU/memory usage, polled. Yields an (empty) map per poll;
/// empty means metrics are unavailable.
pub trait UsageSource: Send + Sync {
    fn poll(&self) -> UsageStream;
}

/// Narrow columns (warning marker, actions menu) that never shrink.
const FIXED_COLUMN_MAX: f32 = 60.0;
/// The other columns give up space first, down to this share of their width.
const OTHER_COLUMN_FLOOR: f32 = 0.75;
/// The first (name) column only shrinks once the others are at their floor,
/// and never below this share: names are what people read.
const NAME_COLUMN_FLOOR: f32 = 0.6;

/// Pixels per character of a header title (12 px semibold text).
const HEADER_CHAR_WIDTH: f32 = 7.0;
/// Cell padding on both sides of a header.
const HEADER_PADDING: f32 = 16.0;
/// Room of the sort icon of a sortable header.
const HEADER_SORT_ICON: f32 = 16.0;

/// The narrowest width at which a header title and its sort icon are not
/// cut. Empty titles (marker columns) need nothing.
pub fn header_min_width(title: &str, sortable: bool) -> f32 {
    let chars = title.chars().count();
    if chars == 0 {
        return 0.0;
    }
    let icon = if sortable { HEADER_SORT_ICON } else { 0.0 };
    (chars as f32 * HEADER_CHAR_WIDTH + HEADER_PADDING + icon).ceil()
}

/// Shrinks column widths so the table fits `available` px when it can.
/// Columns after the first scale down together, to 75 % at most; only if that
/// is not enough does the first column shrink, to 60 % at most. Narrow marker
/// columns keep their width. Tables that already fit are returned unchanged,
/// and a table that cannot fit even at the floors scrolls.
pub fn fit_widths(widths: &[f32], available: f32) -> Vec<f32> {
    fit_widths_with_mins(widths, &[], available)
}

/// Like [`fit_widths`], but no column shrinks below its entry in `mins` (the
/// width its header needs; a missing entry means no minimum), as long as the
/// column was not designed narrower than that.
pub fn fit_widths_with_mins(widths: &[f32], mins: &[f32], available: f32) -> Vec<f32> {
    let total: f32 = widths.iter().sum();
    if total <= available {
        return widths.to_vec();
    }
    let min_of = |ix: usize| mins.get(ix).copied().unwrap_or(0.0);
    let flexible = |ix: usize, w: f32| ix > 0 && w > FIXED_COLUMN_MAX;
    // How far each column may still shrink: others first, then the name.
    let slack = |ix: usize, w: f32, share: f32| (w - (w * share).max(min_of(ix)).min(w)).max(0.0);
    let other_slack: Vec<f32> = widths
        .iter()
        .enumerate()
        .map(|(ix, w)| {
            if flexible(ix, *w) {
                slack(ix, *w, OTHER_COLUMN_FLOOR)
            } else {
                0.0
            }
        })
        .collect();
    let other_total: f32 = other_slack.iter().sum();
    let mut deficit = total - available;
    let give = deficit.min(other_total);
    deficit -= give;
    let first = widths.first().copied().unwrap_or(0.0);
    let first_give = if first > FIXED_COLUMN_MAX && deficit > 0.0 {
        deficit.min(slack(0, first, NAME_COLUMN_FLOOR))
    } else {
        0.0
    };
    widths
        .iter()
        .enumerate()
        .map(|(ix, w)| {
            if ix == 0 {
                (w - first_give).floor()
            } else if other_total > 0.0 && other_slack[ix] > 0.0 {
                (w - other_slack[ix] * give / other_total).floor()
            } else {
                *w
            }
        })
        .collect()
}

/// A column to show in a given appearance: the index of its spec and an
/// optional title replacing the spec's.
pub type ShownColumn = (usize, Option<&'static str>);

/// Which columns a table shows, in which order, for `appearance`. Pod-like
/// tables (with Status and CPU columns) follow the design mockups: Command
/// reads Name, Status, CPU, Memory, Restarts, Owner, Age and Native reads
/// Name, Namespace, CPU, Memory, Age, Status. Every other table keeps its
/// own columns, only folding the namespace into the name when `stacked_names`.
pub fn appearance_columns(
    specs: &[ColumnSpec],
    appearance: Appearance,
    stacked_names: bool,
) -> Vec<ShownColumn> {
    let by_key = |key: &str| specs.iter().position(|s| s.key == key);
    let pod_like = by_key("status").is_some() && by_key("cpu").is_some();
    let order: &[(&str, Option<&'static str>)] = match appearance {
        Appearance::Command if pod_like => &[
            ("name", None),
            ("status", None),
            ("cpu", None),
            ("memory", None),
            ("restarts", None),
            ("controlled_by", Some("Owner")),
            ("age", None),
        ],
        Appearance::Native if pod_like => &[
            ("name", None),
            ("namespace", None),
            ("cpu", None),
            ("memory", None),
            ("age", None),
            ("status", None),
        ],
        _ => {
            return specs
                .iter()
                .enumerate()
                .filter(|(_, s)| !(stacked_names && s.kind == ColumnKind::Namespace))
                .map(|(ix, _)| (ix, None))
                .collect();
        }
    };
    order
        .iter()
        .filter_map(|(key, title)| by_key(key).map(|ix| (ix, *title)))
        .collect()
}
