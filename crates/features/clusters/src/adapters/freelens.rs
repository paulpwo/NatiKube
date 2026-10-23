use std::path::{Path, PathBuf};

use natikube_core::Settings;

use crate::domain::{ImportSummary, classify, cluster_store_paths, importable, sync_entry_paths};

/// Freelens' data folder: `~/Library/Application Support/Freelens` on macOS,
/// `%APPDATA%\Freelens` on Windows.
pub fn freelens_data_dir() -> Option<PathBuf> {
    freelens_data_dir_in(dirs::config_dir().as_deref())
}

/// [`freelens_data_dir`] for an explicit config dir (a pure function, so path
/// resolution is testable on any OS).
pub fn freelens_data_dir_in(config_dir: Option<&Path>) -> Option<PathBuf> {
    config_dir.map(|dir| dir.join("Freelens"))
}

/// Kubeconfig files and folders Freelens knows about, read from its
/// preference files in `data_dir`. Only paths are read, never credentials.
/// Paths that do not exist or are already in `known` are left out.
pub fn freelens_kubeconfig_paths(data_dir: &Path, known: &[PathBuf]) -> Vec<PathBuf> {
    let read = |name: &str| std::fs::read_to_string(data_dir.join(name)).unwrap_or_default();
    let mut candidates = sync_entry_paths(&read("lens-user-store.json"));
    candidates.extend(cluster_store_paths(&read("lens-cluster-store.json")));
    importable(candidates, known)
}

/// Offers Freelens' kubeconfig paths for import: the ones not yet in
/// `extra_kubeconfigs` nor equal to the default `~/.kube/config`.
pub fn importable_from_freelens(data_dir: &Path, extra_kubeconfigs: &[PathBuf]) -> Vec<PathBuf> {
    let mut known = extra_kubeconfigs.to_vec();
    if let Some(kube_dir) = super::default_kube_dir() {
        known.push(kube_dir.join("config"));
    }
    freelens_kubeconfig_paths(data_dir, &known)
}

/// Plans an import from Freelens' data folder: the paths in its preference
/// files plus its `kubeconfigs/` folder (when it holds files). Only paths are
/// read, never credentials, and nothing is copied. `known` are the paths the
/// app already reads and `covered_dirs` the folders it scans on its own.
pub fn import_from_freelens(
    data_dir: &Path,
    known: &[PathBuf],
    covered_dirs: &[PathBuf],
) -> ImportSummary {
    let read = |name: &str| std::fs::read_to_string(data_dir.join(name)).unwrap_or_default();
    let mut candidates = sync_entry_paths(&read("lens-user-store.json"));
    candidates.extend(cluster_store_paths(&read("lens-cluster-store.json")));
    let folder = data_dir.join("kubeconfigs");
    let has_files = std::fs::read_dir(&folder)
        .map(|mut entries| entries.any(|e| e.is_ok_and(|e| e.path().is_file())))
        .unwrap_or(false);
    if has_files {
        candidates.push(folder);
    }
    classify(candidates, known, covered_dirs)
}

/// [`import_from_freelens`] against the real Freelens folder and `~/.kube`.
/// Call [`ImportSummary::apply`] and persist the settings to keep the result.
pub fn import_freelens(settings: &Settings) -> ImportSummary {
    let Some(data_dir) = freelens_data_dir() else {
        return ImportSummary::default();
    };
    let covered: Vec<PathBuf> = super::default_kube_dir().into_iter().collect();
    import_from_freelens(&data_dir, &settings.extra_kubeconfigs, &covered)
}

/// [`import_freelens`] on a worker thread; the caller's thread (the UI) never
/// touches the disk. Runtime-agnostic: any executor can poll the future.
pub async fn import_freelens_async(settings: Settings) -> ImportSummary {
    super::background::run(move || import_freelens(&settings)).await
}

/// Plans adding user-picked paths: new ones are imported, known ones are
/// reported as already present, vanished ones as missing.
pub fn kubeconfig_add_summary(known: &[PathBuf], picked: &[PathBuf]) -> ImportSummary {
    let covered: Vec<PathBuf> = super::default_kube_dir().into_iter().collect();
    classify(picked.to_vec(), known, &covered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_paths_from_both_stores() {
        let data = tempfile::tempdir().unwrap();
        let cfg = tempfile::tempdir().unwrap();
        let a = cfg.path().join("a.yaml");
        let b = cfg.path().join("b.yaml");
        std::fs::write(&a, "x").unwrap();
        std::fs::write(&b, "x").unwrap();
        std::fs::write(
            data.path().join("lens-cluster-store.json"),
            format!(r#"{{"clusters":[{{"kubeConfigPath":"{}"}}]}}"#, a.display()),
        )
        .unwrap();
        std::fs::write(
            data.path().join("lens-user-store.json"),
            format!(
                r#"{{"preferences":{{"syncKubeconfigEntries":[{{"path":"{}"}}]}}}}"#,
                b.display()
            ),
        )
        .unwrap();
        let found = freelens_kubeconfig_paths(data.path(), std::slice::from_ref(&a));
        assert_eq!(found, vec![b]);
    }

    #[test]
    fn freelens_dir_sits_in_the_config_dir() {
        assert_eq!(
            freelens_data_dir_in(Some(Path::new("/cfg"))),
            Some(PathBuf::from("/cfg").join("Freelens"))
        );
        assert_eq!(freelens_data_dir_in(None), None);
    }

    #[test]
    fn missing_data_dir_yields_nothing() {
        assert!(importable_from_freelens(Path::new("/nonexistent/freelens"), &[]).is_empty());
    }
}
