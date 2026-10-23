//! Kubeconfig discovery and the file-backed settings store.

mod background;
mod background_store;
mod environment;
mod freelens;
mod kubeconfig;
mod picker;
mod settings_file;

pub use background_store::BackgroundSettingsStore;
pub use environment::init_environment;
#[cfg(not(windows))]
pub use environment::resolve_login_path;
pub use freelens::{
    freelens_data_dir, freelens_data_dir_in, freelens_kubeconfig_paths, import_freelens,
    import_freelens_async, import_from_freelens, importable_from_freelens, kubeconfig_add_summary,
};
pub use kubeconfig::{
    contexts_in_file, current_context, default_kube_dir, discover_clusters, kube_dir_in,
    load_catalog, load_catalog_async,
};
pub use picker::pick_kubeconfig_paths;
pub use settings_file::FileSettingsStore;
