use std::collections::BTreeSet;
use std::path::PathBuf;

use natikube_core::{
    ClusterId, MemorySettingsStore, NamespaceFilter, Settings, SettingsError, SettingsStore,
    ViewId, kinds,
};

#[test]
fn defaults_are_empty() {
    let s = Settings::default();
    assert!(s.extra_kubeconfigs.is_empty());
    assert!(s.last_cluster.is_none());
    assert_eq!(
        s.prefs(&ClusterId::new("x")).namespaces,
        NamespaceFilter::All
    );
}

#[test]
fn json_round_trip_keeps_per_cluster_state() {
    let mut s = Settings::default();
    s.extra_kubeconfigs.push(PathBuf::from("/tmp/extra"));
    s.last_cluster = Some(ClusterId::new("dev-cluster"));
    let prefs = s.prefs_mut(&ClusterId::new("dev-cluster"));
    prefs.namespaces = NamespaceFilter::Only(BTreeSet::from(["default".to_string()]));
    prefs.last_view = Some(ViewId::Resource(kinds::POD));
    let json = serde_json::to_string(&s).unwrap();
    let back: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
}

#[test]
fn unknown_and_missing_fields_are_tolerated() {
    let back: Settings = serde_json::from_str(r#"{"future_field": 1}"#).unwrap();
    assert_eq!(back, Settings::default());
}

#[test]
fn memory_store_round_trips() {
    let store = MemorySettingsStore::default();
    assert_eq!(store.load().unwrap(), Settings::default());
    let s = Settings {
        last_cluster: Some(ClusterId::new("a")),
        ..Settings::default()
    };
    store.save(&s).unwrap();
    assert_eq!(store.load().unwrap(), s);
}

#[test]
fn namespace_filter_matching_and_label() {
    let all = NamespaceFilter::All;
    assert!(all.matches("anything"));
    assert_eq!(all.label(), "All namespaces");
    let one = NamespaceFilter::Only(BTreeSet::from(["default".to_string()]));
    assert!(one.matches("default"));
    assert!(!one.matches("kube-system"));
    assert_eq!(one.label(), "default");
    let two = NamespaceFilter::Only(BTreeSet::from(["a".to_string(), "b".to_string()]));
    assert_eq!(two.label(), "a, b");
    let many = NamespaceFilter::Only((0..5).map(|i| format!("ns{i}")).collect());
    assert_eq!(many.label(), "5 namespaces");
    assert_eq!(
        NamespaceFilter::Only(BTreeSet::new()).label(),
        "All namespaces"
    );
    assert!(NamespaceFilter::Only(BTreeSet::new()).matches("x"));
}

#[test]
fn appearance_defaults_to_standard_for_old_files() {
    use natikube_core::Appearance;
    let old: Settings =
        serde_json::from_str(r#"{"extra_kubeconfigs":["/a"],"last_cluster":"dev-cluster"}"#)
            .unwrap();
    assert_eq!(old.appearance, Appearance::Standard);
    assert_eq!(Settings::default().appearance, Appearance::Standard);
}

#[test]
fn appearance_round_trips_as_lowercase_string() {
    use natikube_core::Appearance;
    let s = Settings {
        appearance: Appearance::Observatory,
        ..Settings::default()
    };
    let json = serde_json::to_string(&s).unwrap();
    assert!(json.contains(r#""appearance":"observatory""#), "{json}");
    let back: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(back.appearance, Appearance::Observatory);
    for a in Appearance::all() {
        assert_eq!(Appearance::from_slug(a.slug()), Some(a));
        assert!(!a.label().is_empty());
    }
    assert_eq!(Appearance::from_slug("MacOS"), Some(Appearance::Native));
    assert_eq!(Appearance::from_slug("nope"), None);
}

// ----- Settings window fields (settings-perf-polish S2/S3) -----

use natikube_core::{Appearance, Density, UserPrefs};

#[test]
fn new_preference_fields_have_documented_defaults() {
    let s = Settings::default();
    assert_eq!(s.ui.font_size, 13);
    assert_eq!(s.ui.zoom_percent, 100);
    assert_eq!(s.ui.density, Density::Comfortable);
    assert_eq!(s.ui.mono_font, None);
    assert_eq!(s.ui.mono_font_size, 12);
    assert_eq!(s.terminal.font_size, 12);
    assert_eq!(s.terminal.shell, None);
    assert_eq!(s.logs.tail_lines, 1000);
    assert!(!s.logs.timestamps);
    assert!(!s.logs.wrap);
    assert_eq!(s.data.metrics_poll_secs, 15);
    assert_eq!(s.data.default_namespace, None);
}

#[test]
fn old_files_without_new_fields_load_with_defaults() {
    let old: Settings =
        serde_json::from_str(r#"{"extra_kubeconfigs":["/a"],"appearance":"native"}"#).unwrap();
    assert_eq!(old.appearance, Appearance::Native);
    assert_eq!(old.ui, Settings::default().ui);
    assert_eq!(old.logs, Settings::default().logs);
}

#[test]
fn unknown_appearance_never_discards_the_file() {
    let s: Settings = serde_json::from_str(
        r#"{"extra_kubeconfigs":["/keep"],"last_cluster":"dev-cluster","appearance":"hologram"}"#,
    )
    .unwrap();
    assert_eq!(s.appearance, Appearance::Standard);
    assert_eq!(s.extra_kubeconfigs, vec![PathBuf::from("/keep")]);
    assert_eq!(s.last_cluster, Some(ClusterId::new("dev-cluster")));
}

#[test]
fn bad_field_values_fall_back_per_field_only() {
    let s: Settings = serde_json::from_str(
        r#"{
            "extra_kubeconfigs": ["/keep"],
            "ui": {"font_size": "huge", "density": "compact", "mono_font": "Menlo", "future": true},
            "terminal": 7,
            "logs": {"tail_lines": -5, "wrap": true},
            "data": {"metrics_poll_secs": 30, "default_namespace": "prod"},
            "brand_new_top_level": {}
        }"#,
    )
    .unwrap();
    assert_eq!(s.extra_kubeconfigs, vec![PathBuf::from("/keep")]);
    // Bad scalar -> default for that field; siblings survive.
    assert_eq!(s.ui.font_size, 13);
    assert_eq!(s.ui.density, Density::Compact);
    assert_eq!(s.ui.mono_font.as_deref(), Some("Menlo"));
    // Wrong shape for a whole section -> that section's defaults.
    assert_eq!(s.terminal, Settings::default().terminal);
    assert_eq!(s.logs.tail_lines, 1000);
    assert!(s.logs.wrap);
    assert_eq!(s.data.metrics_poll_secs, 30);
    assert_eq!(s.data.default_namespace.as_deref(), Some("prod"));
}

#[test]
fn out_of_range_values_are_clamped_on_load() {
    let s: Settings = serde_json::from_str(
        r#"{"ui":{"font_size":2,"zoom_percent":900,"mono_font_size":99},
            "terminal":{"font_size":0},
            "logs":{"tail_lines":0},
            "data":{"metrics_poll_secs":0}}"#,
    )
    .unwrap();
    assert_eq!(s.ui.font_size, 10);
    assert_eq!(s.ui.zoom_percent, 150);
    assert_eq!(s.ui.mono_font_size, 20);
    assert_eq!(s.terminal.font_size, 8);
    assert_eq!(s.logs.tail_lines, 10);
    assert_eq!(s.data.metrics_poll_secs, 5);
}

#[test]
fn new_fields_round_trip() {
    let mut s = Settings::default();
    s.ui.density = Density::Compact;
    s.ui.font_size = 15;
    s.terminal.shell = Some("/bin/zsh".into());
    s.logs.timestamps = true;
    s.data.default_namespace = Some("default".into());
    let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn user_prefs_snapshot_and_apply_keep_cluster_state() {
    let mut s = Settings {
        last_cluster: Some(ClusterId::new("a")),
        ..Settings::default()
    };
    let mut prefs: UserPrefs = s.user_prefs();
    prefs.appearance = Appearance::Command;
    prefs.logs.tail_lines = 250;
    s.apply_user_prefs(prefs);
    assert_eq!(s.appearance, Appearance::Command);
    assert_eq!(s.logs.tail_lines, 250);
    assert_eq!(s.last_cluster, Some(ClusterId::new("a")));
}

#[test]
fn apply_user_prefs_clamps() {
    let mut s = Settings::default();
    let mut prefs = s.user_prefs();
    prefs.ui.zoom_percent = 1;
    s.apply_user_prefs(prefs);
    assert_eq!(s.ui.zoom_percent, 75);
}

#[test]
fn initial_namespaces_prefer_a_saved_filter_then_the_default() {
    let id = ClusterId::new("dev-cluster");
    let mut s = Settings::default();
    assert_eq!(s.initial_namespaces(&id), NamespaceFilter::All);

    s.data.default_namespace = Some("prod".to_string());
    assert_eq!(
        s.initial_namespaces(&id),
        NamespaceFilter::Only(BTreeSet::from(["prod".to_string()]))
    );

    // A saved explicit filter always wins.
    s.prefs_mut(&id).namespaces =
        NamespaceFilter::Only(BTreeSet::from(["kube-system".to_string()]));
    assert_eq!(
        s.initial_namespaces(&id),
        NamespaceFilter::Only(BTreeSet::from(["kube-system".to_string()]))
    );

    // An empty saved set behaves like All, so the default applies.
    s.prefs_mut(&id).namespaces = NamespaceFilter::Only(BTreeSet::new());
    assert_eq!(
        s.initial_namespaces(&id),
        NamespaceFilter::Only(BTreeSet::from(["prod".to_string()]))
    );
}

#[test]
fn one_bad_cluster_entry_does_not_drop_the_others() {
    let json = r#"{"clusters": {
        "good": {"namespaces": {"Only": ["kube-system"]}},
        "bad-string": "oops",
        "bad-null": null,
        "also-good": {"last_view": null}
    }}"#;
    let back: Settings = serde_json::from_str(json).unwrap();
    let keys: Vec<&str> = back.clusters.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["also-good", "good"]);
    assert_eq!(
        back.prefs(&ClusterId::new("good")).namespaces,
        NamespaceFilter::Only(BTreeSet::from(["kube-system".to_string()]))
    );
}

#[test]
fn a_bad_field_inside_a_cluster_entry_only_resets_that_field() {
    let json = r#"{"clusters": {"c": {"namespaces": 7, "last_view": {"Resource": "pods"}}}}"#;
    let back: Settings = serde_json::from_str(json).unwrap();
    let prefs = back.prefs(&ClusterId::new("c"));
    assert_eq!(prefs.namespaces, NamespaceFilter::All);
    assert!(back.clusters.contains_key("c"));
}

#[test]
fn a_non_map_clusters_value_falls_back_to_empty() {
    let back: Settings = serde_json::from_str(r#"{"clusters": [1, 2]}"#).unwrap();
    assert!(back.clusters.is_empty());
}

#[test]
fn load_reporting_turns_a_load_failure_into_defaults_plus_a_warning() {
    struct Broken;
    impl SettingsStore for Broken {
        fn load(&self) -> Result<Settings, SettingsError> {
            Err(SettingsError::Format("expected value at line 1".into()))
        }
        fn save(&self, _: &Settings) -> Result<(), SettingsError> {
            Ok(())
        }
    }
    let report = Broken.load_reporting();
    assert_eq!(report.settings, Settings::default());
    assert!(report.warning.unwrap().contains("expected value"));

    let ok = MemorySettingsStore::default().load_reporting();
    assert!(ok.warning.is_none());
}

#[test]
fn update_prefs_default_to_checking_with_nothing_skipped() {
    let s = Settings::default();
    assert!(s.updates.check_on_startup);
    assert_eq!(s.updates.skipped_version, None);
    // A settings file from before the feature gets the same defaults.
    let back: Settings = serde_json::from_str(r#"{"appearance":"command"}"#).unwrap();
    assert!(back.updates.check_on_startup);
}

#[test]
fn update_prefs_load_tolerantly_and_round_trip() {
    let back: Settings = serde_json::from_str(
        r#"{"updates":{"check_on_startup":"yes","skipped_version":"  0.2.0-alpha.1 "}}"#,
    )
    .unwrap();
    // The wrong-typed flag falls back to its default; the version is trimmed.
    assert!(back.updates.check_on_startup);
    assert_eq!(
        back.updates.skipped_version.as_deref(),
        Some("0.2.0-alpha.1")
    );

    let back: Settings = serde_json::from_str(r#"{"updates":"garbage"}"#).unwrap();
    assert_eq!(back.updates, natikube_core::UpdatePrefs::default());

    let back: Settings =
        serde_json::from_str(r#"{"updates":{"check_on_startup":false,"skipped_version":" "}}"#)
            .unwrap();
    assert!(!back.updates.check_on_startup);
    assert_eq!(back.updates.skipped_version, None);

    let mut s = Settings::default();
    s.updates.check_on_startup = false;
    s.updates.skipped_version = Some("0.3.0".into());
    let again: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, again);
}

#[test]
fn user_prefs_carry_the_updates_section() {
    let mut s = Settings::default();
    let mut prefs = s.user_prefs();
    prefs.updates.check_on_startup = false;
    s.apply_user_prefs(prefs);
    assert!(!s.updates.check_on_startup);
}
