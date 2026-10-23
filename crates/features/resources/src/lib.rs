//! F2 — Resource browser (vertical slice).
//!
//! * `domain`: pure pod/row models and kubectl-compatible derivations.
//! * `adapters`: kube-runtime watchers and `k8s_openapi` mappings.
//! * `ui`: GPUI views. [`ResourcesView`] is the public entry point.

pub mod adapters;
pub mod domain;
pub mod ui;

pub use adapters::{
    EventAssembler, KubeObjectSource, KubePodSource, KubeUsageSource, MetricsTarget, RowAssembler,
    fetch_crd, list_names, pod_to_facts, pod_to_row, strip_ingest_noise, watch_crds,
};
pub use domain::{
    Capacity, Cell, ClusterKpis, ClusterSummary, ColumnKind, ColumnSpec, ContainerFacts,
    ContainerState, ContainerStateKind, ContainerStatusFacts, CrdInfo, DEMO_NAMESPACES, DemoSource,
    DemoUsage, FakePodSource, GroupItem, INTERN_MAX_ENTRIES, INTERN_MAX_LEN, InitContainerFacts,
    Interner, NamespaceGroup, ObjectRow, ObjectSource, ObjectTable, OwnerRef, PodEvent,
    PodEventStream, PodFacts, PodRow, PodRows, PodSource, PodSummary, PodTable, Printer,
    PrinterColumn, RowAction, RowEvent, RowEventStream, ShownColumn, SortDirection, SourceError,
    StatusCounts, StatusFilter, StatusFilterCounts, StatusTone, Text, Usage, UsageMap, UsageSource,
    UsageStream, WarningEvent, age_tick_interval, appearance_columns, bar_fill, cluster_kpis,
    count_by_health, crd_from_value, crds_from_list_json, demo_has, demo_namespaces, demo_objects,
    demo_rows, demo_usage, fit_widths, fit_widths_with_mins, flatten_groups, group_by_namespace,
    header_min_width, parse_metrics, pod_facts_from_value, pod_object_row, printer_for,
    printer_for_custom, row_actions, row_ref, status_tone, summarize, summarize_cluster,
    synthetic_rows, usage_key,
};
pub use ui::{LoadState, ResourcesView};
