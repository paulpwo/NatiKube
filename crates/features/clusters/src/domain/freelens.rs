//! Parsing of Freelens/Lens preference files to find kubeconfig paths.
//! Only paths are read, never credentials.

use std::path::PathBuf;

/// `kubeConfigPath` of every cluster in `lens-cluster-store.json`.
pub fn cluster_store_paths(json: &str) -> Vec<PathBuf> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    value
        .get("clusters")
        .and_then(|c| c.as_array())
        .map(|clusters| {
            clusters
                .iter()
                .filter_map(|c| c.get("kubeConfigPath")?.as_str())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

/// `preferences.syncKubeconfigEntries[].path` of `lens-user-store.json`.
pub fn sync_entry_paths(json: &str) -> Vec<PathBuf> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    value
        .pointer("/preferences/syncKubeconfigEntries")
        .and_then(|e| e.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|e| e.get("path")?.as_str())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Keeps candidates that exist, are not already `known` and are not repeated.
pub fn importable(candidates: Vec<PathBuf>, known: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for path in candidates {
        if known.contains(&path) || out.contains(&path) || !path.exists() {
            continue;
        }
        out.push(path);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn reads_cluster_store_paths() {
        let json = r#"{"clusters":[
            {"id":"1","contextName":"a","kubeConfigPath":"/home/u/.kube/config"},
            {"id":"2","contextName":"b","kubeConfigPath":"/home/u/other.yaml"},
            {"id":"3","contextName":"c"}]}"#;
        assert_eq!(
            cluster_store_paths(json),
            vec![
                PathBuf::from("/home/u/.kube/config"),
                PathBuf::from("/home/u/other.yaml")
            ]
        );
    }

    #[test]
    fn reads_sync_entries_from_user_store() {
        let json = r#"{"preferences":{"syncKubeconfigEntries":[
            {"path":"/home/u/kubes"},{"path":"/home/u/x.yaml"},{"nope":1}]}}"#;
        assert_eq!(
            sync_entry_paths(json),
            vec![
                PathBuf::from("/home/u/kubes"),
                PathBuf::from("/home/u/x.yaml")
            ]
        );
    }

    #[test]
    fn invalid_json_yields_nothing() {
        assert!(cluster_store_paths("{ nope").is_empty());
        assert!(sync_entry_paths("").is_empty());
        assert!(sync_entry_paths("{}").is_empty());
    }

    #[test]
    fn merge_import_dedupes_and_skips_known_and_missing() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.yaml");
        std::fs::write(&real, "x").unwrap();
        let known = dir.path().join("known.yaml");
        std::fs::write(&known, "x").unwrap();
        let candidates = vec![
            real.clone(),
            real.clone(),
            known.clone(),
            dir.path().join("missing.yaml"),
        ];
        let found = importable(candidates, &[known]);
        assert_eq!(found, vec![real]);
    }
}
