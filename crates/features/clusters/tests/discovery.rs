use std::ffi::OsString;
use std::path::{Path, PathBuf};

use natikube_clusters::{
    ImportSummary, discover_clusters, import_from_freelens, kubeconfig_add_summary,
};
use natikube_core::Settings;

fn kubeconfig(contexts: &[(&str, &str, &str)]) -> String {
    // (context name, server, user)
    let mut out = String::from("apiVersion: v1\nkind: Config\nclusters:\n");
    for (ctx, server, _) in contexts {
        out.push_str(&format!(
            "- name: cl-{ctx}\n  cluster:\n    server: {server}\n"
        ));
    }
    out.push_str("contexts:\n");
    for (ctx, _, user) in contexts {
        out.push_str(&format!(
            "- name: {ctx}\n  context:\n    cluster: cl-{ctx}\n    user: {user}\n"
        ));
    }
    out.push_str("users:\n");
    for (_, _, user) in contexts {
        out.push_str(&format!("- name: {user}\n  user: {{}}\n"));
    }
    out
}

fn put(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    path
}

/// A `~/.kube`-like folder: config, an EKS backup, junk and cache dirs.
fn kube_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    put(
        dir.path(),
        "config",
        &kubeconfig(&[("dev-cluster", "https://k3s:6443", "admin")]),
    );
    put(
        dir.path(),
        "config.bak",
        &kubeconfig(&[
            ("prod-cluster", "https://prod.eks.example", "aws-prod"),
            ("stage-cluster", "https://dev.eks.example", "aws-dev"),
        ]),
    );
    put(dir.path(), "notes.txt", "just some text");
    put(dir.path(), "empty.yaml", "apiVersion: v1\nkind: Config\n");
    put(
        dir.path(),
        ".hidden",
        &kubeconfig(&[("hidden", "https://h", "u")]),
    );
    for cache in ["cache", "http-cache"] {
        let sub = dir.path().join(cache);
        std::fs::create_dir(&sub).unwrap();
        put(&sub, "inside", &kubeconfig(&[("cached", "https://c", "u")]));
    }
    dir
}

fn names(found: &[natikube_core::ClusterInfo]) -> Vec<&str> {
    found.iter().map(|c| c.name.as_str()).collect()
}

#[test]
fn scans_top_level_files_only_and_skips_junk() {
    let dir = kube_dir();
    let found = discover_clusters(Some(dir.path()), None, &[]);
    let mut got = names(&found);
    got.sort();
    assert_eq!(got, vec!["dev-cluster", "prod-cluster", "stage-cluster"]);
    assert!(found.iter().all(|c| c.kubeconfig.is_some()));
    let eks = found.iter().find(|c| c.name == "prod-cluster").unwrap();
    assert_eq!(eks.source_file_name().as_deref(), Some("config.bak"));
    assert_eq!(eks.server.as_deref(), Some("https://prod.eks.example"));
}

#[test]
fn default_config_comes_first() {
    let dir = kube_dir();
    let found = discover_clusters(Some(dir.path()), None, &[]);
    assert_eq!(found[0].name, "dev-cluster");
}

#[test]
fn identical_contexts_across_files_are_deduped_preferring_default_config() {
    let dir = kube_dir();
    let copy = put(
        dir.path(),
        "config.old",
        &kubeconfig(&[("dev-cluster", "https://k3s:6443", "admin")]),
    );
    let found = discover_clusters(Some(dir.path()), None, &[]);
    let k3s: Vec<_> = found
        .iter()
        .filter(|c| c.context == "dev-cluster")
        .collect();
    assert_eq!(k3s.len(), 1);
    assert_eq!(k3s[0].source_file_name().as_deref(), Some("config"));
    assert_eq!(k3s[0].also_in, vec![copy]);
}

#[test]
fn env_order_beats_mtime_when_there_is_no_default_config() {
    let dir = tempfile::tempdir().unwrap();
    let body = kubeconfig(&[("shared", "https://s", "u")]);
    let first = put(dir.path(), "first.yaml", &body);
    let second = put(dir.path(), "second.yaml", &body);
    let env = std::env::join_paths([&second, &first]).unwrap();
    let found = discover_clusters(None, Some(&env), &[]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kubeconfig.as_deref(), Some(second.as_path()));
    assert_eq!(found[0].also_in, vec![first]);
}

#[test]
fn newest_file_wins_among_equals() {
    let dir = tempfile::tempdir().unwrap();
    let body = kubeconfig(&[("same", "https://s", "u")]);
    let old = put(dir.path(), "a.yaml", &body);
    let new = put(dir.path(), "b.yaml", &body);
    let past = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(past)
        .unwrap();
    let found = discover_clusters(None, None, &[dir.path().to_path_buf()]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kubeconfig.as_deref(), Some(new.as_path()));
    assert_eq!(found[0].also_in, vec![old]);
}

#[test]
fn same_name_with_another_server_is_kept_and_disambiguated() {
    let dir = tempfile::tempdir().unwrap();
    put(
        dir.path(),
        "a.yaml",
        &kubeconfig(&[("dev", "https://one", "u")]),
    );
    put(
        dir.path(),
        "b.yaml",
        &kubeconfig(&[("dev", "https://two", "u")]),
    );
    let found = discover_clusters(Some(dir.path()), None, &[]);
    assert_eq!(found.len(), 2);
    assert_ne!(found[0].id, found[1].id);
    assert_eq!(found[0].name, "dev");
    assert_eq!(found[1].name, "dev (b.yaml)");
    assert!(found.iter().all(|c| c.context == "dev"));
}

#[test]
fn same_name_with_another_user_is_a_different_context() {
    let dir = tempfile::tempdir().unwrap();
    put(
        dir.path(),
        "a.yaml",
        &kubeconfig(&[("dev", "https://s", "alice")]),
    );
    put(
        dir.path(),
        "b.yaml",
        &kubeconfig(&[("dev", "https://s", "bob")]),
    );
    assert_eq!(discover_clusters(Some(dir.path()), None, &[]).len(), 2);
}

#[test]
fn same_file_reached_twice_is_not_a_duplicate_of_itself() {
    let dir = kube_dir();
    let config = dir.path().join("config");
    let env = OsString::from(config.as_os_str());
    let found = discover_clusters(Some(dir.path()), Some(&env), &[config]);
    let k3s = found.iter().find(|c| c.name == "dev-cluster").unwrap();
    assert!(k3s.also_in.is_empty());
}

#[test]
fn extras_files_and_folders_are_scanned_one_level() {
    let kube = tempfile::tempdir().unwrap();
    let extra = tempfile::tempdir().unwrap();
    let file = put(
        extra.path(),
        "one.yaml",
        &kubeconfig(&[("one", "https://1", "u")]),
    );
    let folder = tempfile::tempdir().unwrap();
    put(
        folder.path(),
        "two.yaml",
        &kubeconfig(&[("two", "https://2", "u")]),
    );
    std::fs::create_dir(folder.path().join("nested")).unwrap();
    put(
        &folder.path().join("nested"),
        "deep.yaml",
        &kubeconfig(&[("deep", "https://3", "u")]),
    );
    let found = discover_clusters(
        Some(kube.path()),
        None,
        &[
            file,
            folder.path().to_path_buf(),
            PathBuf::from("/nonexistent/x"),
        ],
    );
    let mut got = names(&found);
    got.sort();
    assert_eq!(got, vec!["one", "two"]);
}

#[test]
fn missing_kube_dir_and_unreadable_files_yield_nothing() {
    assert!(discover_clusters(Some(Path::new("/nonexistent/kube")), None, &[]).is_empty());
    assert!(discover_clusters(None, None, &[]).is_empty());
}

const STORE_A: &str = "a.yaml";

fn freelens_dir(store: &str) -> tempfile::TempDir {
    let data = tempfile::tempdir().unwrap();
    put(data.path(), "lens-cluster-store.json", store);
    data
}

#[test]
fn freelens_import_classifies_imported_present_and_missing() {
    let files = tempfile::tempdir().unwrap();
    let new = put(files.path(), STORE_A, "x");
    let known = put(files.path(), "known.yaml", "x");
    let gone = files.path().join("gone.yaml");
    let store = format!(
        r#"{{"clusters":[
            {{"contextName":"a","kubeConfigPath":"{n}"}},
            {{"contextName":"b","kubeConfigPath":"{n}"}},
            {{"contextName":"c","kubeConfigPath":"{k}"}},
            {{"contextName":"d","kubeConfigPath":"{g}"}},
            {{"contextName":"e"}}]}}"#,
        n = new.display(),
        k = known.display(),
        g = gone.display()
    );
    let data = freelens_dir(&store);
    let summary = import_from_freelens(data.path(), std::slice::from_ref(&known), &[]);
    assert_eq!(summary.imported, vec![new.clone()]);
    assert_eq!(summary.already_present, vec![known.clone()]);
    assert_eq!(summary.missing, vec![gone]);

    let mut settings = Settings {
        extra_kubeconfigs: vec![known.clone()],
        ..Settings::default()
    };
    summary.apply(&mut settings);
    assert_eq!(settings.extra_kubeconfigs, vec![known, new]);
}

#[test]
fn freelens_import_treats_kube_dir_files_as_present() {
    let kube = tempfile::tempdir().unwrap();
    let inside = put(kube.path(), "config.bak", "x");
    let data = freelens_dir(&format!(
        r#"{{"clusters":[{{"kubeConfigPath":"{}"}}]}}"#,
        inside.display()
    ));
    let summary = import_from_freelens(data.path(), &[], &[kube.path().to_path_buf()]);
    assert!(summary.imported.is_empty());
    assert_eq!(summary.already_present, vec![inside]);
}

#[test]
fn freelens_import_offers_the_kubeconfigs_folder_once() {
    let data = tempfile::tempdir().unwrap();
    let folder = data.path().join("kubeconfigs");
    std::fs::create_dir(&folder).unwrap();
    put(&folder, "abc", "x");
    let summary = import_from_freelens(data.path(), &[], &[]);
    assert_eq!(summary.imported, vec![folder.clone()]);
    let again = import_from_freelens(data.path(), std::slice::from_ref(&folder), &[]);
    assert!(again.imported.is_empty());
    assert_eq!(again.already_present, vec![folder]);
}

#[test]
fn freelens_import_with_nothing_there_is_empty() {
    let summary = import_from_freelens(Path::new("/nonexistent/freelens"), &[], &[]);
    assert_eq!(summary, ImportSummary::default());
    assert!(summary.is_empty());
}

#[test]
fn freelens_import_tolerates_broken_json() {
    let data = freelens_dir("{ nope");
    assert!(import_from_freelens(data.path(), &[], &[]).is_empty());
}

#[test]
fn add_kubeconfig_summary_counts_new_and_known_paths() {
    let dir = tempfile::tempdir().unwrap();
    let a = put(dir.path(), "a.yaml", "x");
    let b = put(dir.path(), "b.yaml", "x");
    let mut settings = Settings {
        extra_kubeconfigs: vec![a.clone()],
        ..Settings::default()
    };
    let summary = kubeconfig_add_summary(
        &settings.extra_kubeconfigs,
        &[a.clone(), b.clone(), b.clone()],
    );
    assert_eq!(summary.imported, vec![b.clone()]);
    assert_eq!(summary.already_present, vec![a.clone()]);
    summary.apply(&mut settings);
    assert_eq!(settings.extra_kubeconfigs, vec![a, b]);
}

#[test]
fn default_config_beats_a_shorter_alias_in_an_old_backup() {
    let dir = tempfile::tempdir().unwrap();
    let arn = "arn:aws:eks:eu-central-1:1:cluster/prod-cluster";
    put(
        dir.path(),
        "config",
        &kubeconfig(&[(arn, "https://eks", "aws")]),
    );
    put(
        dir.path(),
        "config.bak",
        &kubeconfig(&[("prod-cluster", "https://eks", "aws")]),
    );
    let found = discover_clusters(Some(dir.path()), None, &[]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].context, arn);
    assert_eq!(found[0].source_file_name().as_deref(), Some("config"));
    assert_eq!(found[0].aliases, vec!["prod-cluster".to_string()]);
}

// Symlinks need std::os::unix; Windows symlinks require extra privileges.
#[cfg(unix)]
#[test]
fn symlink_and_target_in_kube_dir_keep_the_real_file_name() {
    let dir = tempfile::tempdir().unwrap();
    let body = kubeconfig(&[("same", "https://s", "u")]);
    let real = put(dir.path(), "real.yaml", &body);
    std::os::unix::fs::symlink(&real, dir.path().join("a-link")).unwrap();
    let found = discover_clusters(Some(dir.path()), None, &[]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].source_file_name().as_deref(), Some("real.yaml"));
    assert!(found[0].also_in.is_empty());
}

// Symlinks need std::os::unix; Windows symlinks require extra privileges.
#[cfg(unix)]
#[test]
fn symlinked_default_config_ranks_first_but_shows_the_real_file() {
    let dir = tempfile::tempdir().unwrap();
    let real = put(
        dir.path(),
        "config.real",
        &kubeconfig(&[("main", "https://m", "u")]),
    );
    std::os::unix::fs::symlink(&real, dir.path().join("config")).unwrap();
    put(
        dir.path(),
        "other",
        &kubeconfig(&[("zzz", "https://z", "u")]),
    );
    let found = discover_clusters(Some(dir.path()), None, &[]);
    assert_eq!(found[0].context, "main");
    assert_eq!(found[0].source_file_name().as_deref(), Some("config.real"));
}

#[test]
fn find_cluster_matches_name_context_id_and_aliases() {
    use natikube_clusters::find_cluster;
    let dir = tempfile::tempdir().unwrap();
    let arn = "arn:aws:eks:eu-central-1:1:cluster/prod-cluster";
    put(
        dir.path(),
        "config",
        &kubeconfig(&[
            (arn, "https://eks", "aws"),
            ("dev-cluster", "https://k", "u"),
        ]),
    );
    put(
        dir.path(),
        "config.bak",
        &kubeconfig(&[("prod-cluster", "https://eks", "aws")]),
    );
    let found = discover_clusters(Some(dir.path()), None, &[]);
    let by_alias = find_cluster(&found, "prod-cluster").unwrap();
    assert_eq!(by_alias.context, arn);
    assert_eq!(find_cluster(&found, arn).unwrap().context, arn);
    assert_eq!(
        find_cluster(&found, "dev-cluster").unwrap().context,
        "dev-cluster"
    );
    let id = found[1].id.as_str().to_string();
    assert_eq!(find_cluster(&found, &id).unwrap().context, "dev-cluster");
    assert!(find_cluster(&found, "nope").is_none());
}

#[test]
fn exact_context_beats_an_alias_of_another_cluster() {
    use natikube_clusters::find_cluster;
    let mut a = natikube_core::ClusterInfo::from_context("a");
    a.aliases.push("b".into());
    let b = natikube_core::ClusterInfo::from_context("b");
    let list = vec![a, b];
    assert_eq!(find_cluster(&list, "b").unwrap().context, "b");
}

#[test]
fn async_entry_points_match_the_sync_ones() {
    let dir = kube_dir();
    let settings = natikube_core::Settings {
        extra_kubeconfigs: vec![dir.path().join("config.bak")],
        ..Default::default()
    };
    let sync = natikube_clusters::load_catalog(&settings);
    let got = futures::executor::block_on(natikube_clusters::load_catalog_async(settings.clone()));
    assert_eq!(got, sync);
    let direct = natikube_clusters::import_freelens(&settings);
    let async_ = futures::executor::block_on(natikube_clusters::import_freelens_async(settings));
    assert_eq!(async_, direct);
}

#[test]
fn settings_and_kube_dirs_resolve_from_explicit_roots() {
    use natikube_clusters::{FileSettingsStore, kube_dir_in};
    use std::path::{Path, PathBuf};
    let store = FileSettingsStore::in_config_dir(Some(PathBuf::from("/cfg"))).unwrap();
    assert_eq!(
        store.path(),
        PathBuf::from("/cfg").join("NatiKube").join("settings.json")
    );
    assert!(FileSettingsStore::in_config_dir(None).is_none());
    assert_eq!(
        kube_dir_in(Some(Path::new("/home/u"))),
        Some(PathBuf::from("/home/u").join(".kube"))
    );
    assert_eq!(kube_dir_in(None), None);
}
