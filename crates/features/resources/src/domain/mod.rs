//! Pure resource-browser domain: row models, kubectl-compatible derivations,
//! per-kind column printers, the sorted/filtered table model, metrics parsing
//! and overview summaries. No I/O, no GPUI, no kube.

mod actions;
mod appearance;
mod crd;
mod demo;
mod fake;
mod json;
mod object;
mod object_table;
mod overview;
mod pod;
mod pod_json;
mod printers;
mod source;
mod status;
mod table;
mod text;
mod tick;
mod usage;

pub use actions::{RowAction, row_actions, row_ref};
pub use appearance::{
    ClusterKpis, GroupItem, NamespaceGroup, StatusFilter, StatusFilterCounts, bar_fill,
    cluster_kpis, flatten_groups, group_by_namespace,
};
pub use crd::{CrdInfo, crd_from_value, crds_from_list_json};
pub use demo::{
    DEMO_NAMESPACES, DemoSource, DemoUsage, demo_has, demo_namespaces, demo_objects, demo_rows,
    demo_usage,
};
pub use fake::{FakePodSource, synthetic_rows};
pub use object::{
    Capacity, Cell, ColumnKind, ColumnSpec, ObjectRow, ObjectSource, OwnerRef, RowEvent,
    RowEventStream, ShownColumn, UsageSource, UsageStream, appearance_columns, fit_widths,
    fit_widths_with_mins, header_min_width,
};
pub use object_table::{ObjectTable, SortDirection};
pub use overview::{
    ClusterSummary, StatusCounts, WarningEvent, count_by_health, summarize_cluster,
};
pub use pod::{
    ContainerFacts, ContainerState, ContainerStateKind, ContainerStatusFacts, InitContainerFacts,
    PodFacts, PodRow,
};
pub use pod_json::{PodRows, pod_facts_from_value, pod_object_row};
pub use printers::{Printer, PrinterColumn, printer_for, printer_for_custom};
pub use source::{PodEvent, PodEventStream, PodSource, SourceError};
pub use status::{PodSummary, StatusTone, status_tone, summarize};
pub use table::PodTable;
pub use text::{INTERN_MAX_ENTRIES, INTERN_MAX_LEN, Interner, Text};
pub use tick::age_tick_interval;
pub use usage::{Usage, UsageMap, parse_metrics, usage_key};
