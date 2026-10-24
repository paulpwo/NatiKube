use natikube_clusters::FileSettingsStore;
use natikube_core::{ClusterId, Settings, SettingsError, SettingsStore};

#[test]
fn missing_file_loads_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileSettingsStore::new(dir.path().join("settings.json"));
    assert_eq!(store.load().unwrap(), Settings::default());
}

#[test]
fn save_creates_parent_folders_and_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileSettingsStore::new(dir.path().join("a/b/settings.json"));
    let settings = Settings {
        last_cluster: Some(ClusterId::new("dev-cluster")),
        ..Settings::default()
    };
    store.save(&settings).unwrap();
    assert_eq!(store.load().unwrap(), settings);
}

#[test]
fn corrupt_file_is_a_format_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, "{ nope").unwrap();
    let err = FileSettingsStore::new(path).load().unwrap_err();
    assert!(matches!(err, SettingsError::Format(_)));
}

#[test]
fn persists_extra_kubeconfigs_and_per_cluster_prefs() {
    use natikube_core::{ClusterPrefs, NamespaceFilter, ViewId, kinds};
    let dir = tempfile::tempdir().unwrap();
    let store = FileSettingsStore::new(dir.path().join("settings.json"));
    let mut settings = Settings {
        extra_kubeconfigs: vec!["/tmp/extra.yaml".into()],
        ..Settings::default()
    };
    settings.clusters.insert(
        "dev-cluster".to_string(),
        ClusterPrefs {
            namespaces: NamespaceFilter::Only(["kube-system".to_string()].into()),
            last_view: Some(ViewId::Resource(kinds::POD)),
        },
    );
    store.save(&settings).unwrap();
    assert_eq!(store.load().unwrap(), settings);
}

fn corrupt_store() -> (tempfile::TempDir, FileSettingsStore, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, "{ \"appearance\": \"native\", oops").unwrap();
    (dir, FileSettingsStore::new(&path), path)
}

#[test]
fn syntax_error_keeps_a_backup_and_never_gets_overwritten() {
    let (dir, store, path) = corrupt_store();
    let original = std::fs::read_to_string(&path).unwrap();

    assert!(matches!(store.load(), Err(SettingsError::Format(_))));
    let backup = dir.path().join("settings.json.bak");
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), original);

    // Saving while the file is broken must refuse and leave it untouched.
    let err = store.save(&Settings::default()).unwrap_err();
    assert!(matches!(err, SettingsError::Format(_)));
    assert!(err.to_string().contains("settings.json"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);

    // Loading again does not pile up identical backups.
    let _ = store.load();
    assert!(!dir.path().join("settings.json.bak1").exists());
}

#[test]
fn saving_works_again_once_the_file_is_fixed() {
    let (_dir, store, path) = corrupt_store();
    let _ = store.load();
    std::fs::write(&path, "{}").unwrap();
    let settings = Settings {
        last_cluster: Some(ClusterId::new("c1")),
        ..Settings::default()
    };
    store.save(&settings).unwrap();
    assert_eq!(store.load().unwrap(), settings);
}

#[test]
fn a_different_broken_file_gets_its_own_backup() {
    let (dir, store, path) = corrupt_store();
    let _ = store.load();
    std::fs::write(&path, "[ also broken").unwrap();
    let _ = store.load();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("settings.json.bak")).unwrap(),
        "{ \"appearance\": \"native\", oops"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("settings.json.bak1")).unwrap(),
        "[ also broken"
    );
}

#[test]
fn load_reporting_surfaces_the_syntax_error_with_defaults() {
    let (_dir, store, _path) = corrupt_store();
    let report = store.load_reporting();
    assert_eq!(report.settings, Settings::default());
    assert!(report.warning.is_some());
}

#[test]
fn background_store_coalesces_writes_and_flushes() {
    use natikube_clusters::BackgroundSettingsStore;
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let inner = Arc::new(FileSettingsStore::new(dir.path().join("settings.json")));
    let store = BackgroundSettingsStore::new(inner.clone());
    for i in 0..50 {
        let settings = Settings {
            last_cluster: Some(ClusterId::new(format!("c{i}"))),
            ..Settings::default()
        };
        store.save(&settings).unwrap();
    }
    store.flush();
    assert_eq!(
        inner.load().unwrap().last_cluster,
        Some(ClusterId::new("c49"))
    );
    // load reads through to the inner store.
    assert_eq!(
        store.load().unwrap().last_cluster,
        Some(ClusterId::new("c49"))
    );
}

#[test]
fn background_store_flushes_on_drop() {
    use natikube_clusters::BackgroundSettingsStore;
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let inner = Arc::new(FileSettingsStore::new(dir.path().join("settings.json")));
    {
        let store = BackgroundSettingsStore::new(inner.clone());
        store
            .save(&Settings {
                last_cluster: Some(ClusterId::new("bye")),
                ..Settings::default()
            })
            .unwrap();
    }
    assert_eq!(
        inner.load().unwrap().last_cluster,
        Some(ClusterId::new("bye"))
    );
}

#[test]
fn config_dir_override_replaces_the_os_location() {
    use std::ffi::OsString;
    use std::path::PathBuf;
    let os = Some(PathBuf::from("/os-config"));
    assert_eq!(
        FileSettingsStore::resolve_app_config_dir(Some(OsString::from("/tmp/qa")), os.clone()),
        Some(PathBuf::from("/tmp/qa")),
        "the override is the NatiKube folder itself"
    );
    assert_eq!(
        FileSettingsStore::resolve_app_config_dir(None, os.clone()),
        Some(PathBuf::from("/os-config/NatiKube"))
    );
    assert_eq!(
        FileSettingsStore::resolve_app_config_dir(Some(OsString::new()), os),
        Some(PathBuf::from("/os-config/NatiKube")),
        "an empty override is ignored"
    );
    assert_eq!(
        FileSettingsStore::resolve_app_config_dir(Some(OsString::from("/tmp/qa")), None),
        Some(PathBuf::from("/tmp/qa")),
        "the override works even without an OS config dir"
    );
    assert_eq!(FileSettingsStore::resolve_app_config_dir(None, None), None);
}
