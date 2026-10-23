//! Pure cluster-list state (no I/O, no GPUI).

mod avatar;
mod discovery;
mod freelens;
mod import;
mod login_path;

use std::collections::HashMap;

use natikube_core::{ClusterId, ClusterInfo, ConnectionState};

pub use avatar::avatar_hue;
pub use discovery::{
    Found, RANK_DEFAULT_CONFIG, RANK_ENV_BASE, RANK_EXTRA, RANK_KUBE_DIR, dedupe, env_entries,
    find_cluster,
};
pub use freelens::{cluster_store_paths, importable, sync_entry_paths};
pub use import::{ImportSummary, classify};
pub use login_path::{merge_paths, needs_login_path, parse_login_path};

/// The clusters shown in the rail with their connection state and the active
/// one.
#[derive(Debug, Clone, Default)]
pub struct ClusterList {
    clusters: Vec<ClusterInfo>,
    states: HashMap<ClusterId, ConnectionState>,
    active: Option<ClusterId>,
}

impl ClusterList {
    pub fn new(clusters: Vec<ClusterInfo>) -> Self {
        Self {
            clusters,
            ..Self::default()
        }
    }

    pub fn clusters(&self) -> &[ClusterInfo] {
        &self.clusters
    }

    pub fn set_clusters(&mut self, clusters: Vec<ClusterInfo>) {
        self.clusters = clusters;
    }

    pub fn active(&self) -> Option<&ClusterId> {
        self.active.as_ref()
    }

    pub fn active_info(&self) -> Option<&ClusterInfo> {
        let id = self.active.as_ref()?;
        self.clusters.iter().find(|c| &c.id == id)
    }

    pub fn set_active(&mut self, id: Option<ClusterId>) {
        self.active = id;
    }

    pub fn state(&self, id: &ClusterId) -> ConnectionState {
        self.states.get(id).cloned().unwrap_or_default()
    }

    pub fn set_state(&mut self, id: ClusterId, state: ConnectionState) {
        self.states.insert(id, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_active_and_states() {
        let a = ClusterInfo::from_context("a");
        let b = ClusterInfo::from_context("b");
        let mut list = ClusterList::new(vec![a.clone(), b.clone()]);
        assert!(list.active().is_none());
        assert_eq!(list.state(&a.id), ConnectionState::Disconnected);
        list.set_active(Some(b.id.clone()));
        list.set_state(b.id.clone(), ConnectionState::Connecting);
        assert_eq!(list.active_info().map(|c| c.name.as_str()), Some("b"));
        assert_eq!(list.state(&b.id), ConnectionState::Connecting);
        assert_eq!(list.state(&a.id), ConnectionState::Disconnected);
    }
}
