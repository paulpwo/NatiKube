//! F1 — Clusters (vertical slice).
//!
//! * `domain`: pure cluster-list state.
//! * `adapters`: kubeconfig discovery, file-backed settings store.
//! * `ui`: [`ClusterRail`] (left avatars) and [`ClusterHeader`] (sidebar top).

pub mod adapters;
pub mod domain;
pub mod ui;

#[cfg(not(windows))]
pub use adapters::resolve_login_path;
pub use adapters::{
    BackgroundSettingsStore, FileSettingsStore, contexts_in_file, current_context,
    default_kube_dir, discover_clusters, freelens_data_dir, freelens_data_dir_in,
    freelens_kubeconfig_paths, import_freelens, import_freelens_async, import_from_freelens,
    importable_from_freelens, init_environment, kube_dir_in, kubeconfig_add_summary, load_catalog,
    load_catalog_async, pick_kubeconfig_paths,
};
pub use domain::{
    ClusterList, ImportSummary, avatar_hue, find_cluster, merge_paths, needs_login_path,
    parse_login_path,
};
pub use ui::{ClusterHeader, ClusterRail, RAIL_WIDTH};
