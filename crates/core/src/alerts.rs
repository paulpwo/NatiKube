//! Alert configuration: which rules the user turned on, per cluster, plus the
//! global OS-notification switch and history retention. Pure data; the
//! evaluation lives in `natikube-alerts`.
//!
//! Nothing is monitored until the user turns a rule on, and no OS notification
//! or sound happens until they enable it (in-app toasts are on by default). Parsing is tolerant per field, like the
//! rest of the settings file.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::cluster::ClusterId;
use crate::prefs::lenient;
use crate::settings::NamespaceFilter;

/// Where an alert points: the cluster and, when known, the object to open.
/// Carried by `AppCommand::OpenAlertObject`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertTarget {
    pub cluster: ClusterId,
    pub namespace: Option<String>,
    /// A Kubernetes kind such as `Pod`; resolve with `kinds::find`.
    pub kind: Option<String>,
    pub name: Option<String>,
}

/// What a rule can watch for. Declaration order is the order shown in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    NewPod,
    PodFailing,
    PodRestarts,
    PodPendingTooLong,
    DeploymentUnavailable,
    NodeNotReady,
    ClusterUnreachable,
    WarningEvents,
}

/// A tunable value of a rule, so the Settings UI can render inputs without
/// knowing each rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    /// Number of restarts that triggers the rule.
    RestartCount,
    /// Sliding window in seconds.
    WindowSecs,
    /// How long a bad state must last, in seconds.
    DurationSecs,
    /// Warning event reasons to match (empty: every reason).
    Reasons,
}

impl RuleKind {
    pub const ALL: [RuleKind; 8] = [
        Self::NewPod,
        Self::PodFailing,
        Self::PodRestarts,
        Self::PodPendingTooLong,
        Self::DeploymentUnavailable,
        Self::NodeNotReady,
        Self::ClusterUnreachable,
        Self::WarningEvents,
    ];

    /// Stable key used in settings.json.
    pub fn key(self) -> &'static str {
        match self {
            Self::NewPod => "new_pod",
            Self::PodFailing => "pod_failing",
            Self::PodRestarts => "pod_restarts",
            Self::PodPendingTooLong => "pod_pending_too_long",
            Self::DeploymentUnavailable => "deployment_unavailable",
            Self::NodeNotReady => "node_not_ready",
            Self::ClusterUnreachable => "cluster_unreachable",
            Self::WarningEvents => "warning_events",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::NewPod => "New pod",
            Self::PodFailing => "Pod failing",
            Self::PodRestarts => "Pod restarts",
            Self::PodPendingTooLong => "Pod pending too long",
            Self::DeploymentUnavailable => "Deployment unavailable",
            Self::NodeNotReady => "Node not ready",
            Self::ClusterUnreachable => "Cluster unreachable",
            Self::WarningEvents => "Warning events",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::NewPod => "A pod appears after the watch started",
            Self::PodFailing => {
                "A container is in CrashLoopBackOff, Error, ImagePullBackOff, ErrImagePull, \
                 CreateContainerConfigError or was OOMKilled"
            }
            Self::PodRestarts => "A pod's restart count grows by N within a time window",
            Self::PodPendingTooLong => "A pod stays Pending longer than the limit",
            Self::DeploymentUnavailable => {
                "A deployment has unavailable replicas for longer than the limit"
            }
            Self::NodeNotReady => "A node's Ready condition becomes False or Unknown",
            Self::ClusterUnreachable => {
                "The cluster API stops answering, and again when it recovers"
            }
            Self::WarningEvents => "Kubernetes Warning events (FailedScheduling, BackOff, ...)",
        }
    }

    /// Parameters the rule accepts, in display order.
    pub fn params(self) -> &'static [ParamKind] {
        match self {
            Self::PodRestarts => &[ParamKind::RestartCount, ParamKind::WindowSecs],
            Self::PodPendingTooLong | Self::DeploymentUnavailable => &[ParamKind::DurationSecs],
            Self::WarningEvents => &[ParamKind::Reasons],
            _ => &[],
        }
    }

    /// Whether the cluster's namespace filter applies (nodes and connection
    /// health are cluster scoped).
    pub fn is_namespaced(self) -> bool {
        !matches!(self, Self::NodeNotReady | Self::ClusterUnreachable)
    }

    /// Parameters used when the user has not set any.
    pub fn default_params(self) -> RuleParams {
        let mut params = RuleParams {
            count: 3,
            window_secs: 600,
            duration_secs: 0,
            reasons: BTreeSet::new(),
        };
        match self {
            Self::PodPendingTooLong => params.duration_secs = 300,
            Self::DeploymentUnavailable => params.duration_secs = 120,
            _ => {}
        }
        params
    }
}

/// Effective parameters of a rule (defaults filled in).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleParams {
    /// Restart growth that triggers `PodRestarts` (default 3).
    pub count: u32,
    /// Window for `PodRestarts` in seconds (default 600).
    pub window_secs: u32,
    /// Duration limit in seconds (pending: 300, deployment: 120).
    pub duration_secs: u32,
    /// Reason filter for `WarningEvents`; empty matches every reason.
    pub reasons: BTreeSet<String>,
}

/// What the user saved for one rule. Unset parameters mean "use the default".
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleConfig {
    #[serde(deserialize_with = "lenient")]
    pub enabled: bool,
    #[serde(deserialize_with = "lenient")]
    pub count: Option<u32>,
    #[serde(deserialize_with = "lenient")]
    pub window_secs: Option<u32>,
    #[serde(deserialize_with = "lenient")]
    pub duration_secs: Option<u32>,
    #[serde(deserialize_with = "lenient")]
    pub reasons: BTreeSet<String>,
}

impl RuleConfig {
    pub fn on() -> Self {
        Self {
            enabled: true,
            ..Self::default()
        }
    }

    /// Saved parameters over the rule's defaults (zero values are ignored).
    pub fn resolve(&self, kind: RuleKind) -> RuleParams {
        let mut params = kind.default_params();
        if let Some(count) = self.count.filter(|v| *v > 0) {
            params.count = count;
        }
        if let Some(window) = self.window_secs.filter(|v| *v > 0) {
            params.window_secs = window;
        }
        if let Some(duration) = self.duration_secs.filter(|v| *v > 0) {
            params.duration_secs = duration;
        }
        params.reasons = self.reasons.clone();
        params
    }
}

/// Reads the rule map entry by entry: unknown rules and entries of the wrong
/// shape are skipped, a non-map value yields no rules.
fn tolerant_rules<'de, D>(deserializer: D) -> Result<BTreeMap<RuleKind, RuleConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Valid(RuleConfig),
        Invalid(serde::de::IgnoredAny),
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Maybe {
        Map(BTreeMap<String, Entry>),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Maybe::deserialize(deserializer)? {
        Maybe::Map(map) => map
            .into_iter()
            .filter_map(|(key, entry)| match (RuleKind::from_key(&key), entry) {
                (Some(kind), Entry::Valid(config)) => Some((kind, config)),
                _ => None,
            })
            .collect(),
        Maybe::Other(_) => BTreeMap::new(),
    })
}

/// Alert settings of one cluster.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClusterAlerts {
    #[serde(deserialize_with = "tolerant_rules")]
    pub rules: BTreeMap<RuleKind, RuleConfig>,
    /// Applies to namespaced rules only.
    #[serde(deserialize_with = "lenient")]
    pub namespaces: NamespaceFilter,
}

impl ClusterAlerts {
    pub fn is_enabled(&self, kind: RuleKind) -> bool {
        self.rules.get(&kind).is_some_and(|rule| rule.enabled)
    }

    pub fn any_enabled(&self) -> bool {
        self.rules.values().any(|rule| rule.enabled)
    }

    pub fn enabled_rules(&self) -> impl Iterator<Item = RuleKind> + '_ {
        self.rules
            .iter()
            .filter(|(_, rule)| rule.enabled)
            .map(|(kind, _)| *kind)
    }

    /// Effective parameters of a rule (defaults when never configured).
    pub fn params(&self, kind: RuleKind) -> RuleParams {
        self.rules
            .get(&kind)
            .map_or_else(|| kind.default_params(), |rule| rule.resolve(kind))
    }

    pub fn set_enabled(&mut self, kind: RuleKind, enabled: bool) {
        self.rules.entry(kind).or_default().enabled = enabled;
    }

    pub fn set_rule(&mut self, kind: RuleKind, config: RuleConfig) {
        self.rules.insert(kind, config);
    }
}

/// How long history is kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Retention {
    #[serde(deserialize_with = "max_age_days")]
    pub max_age_days: u32,
    #[serde(deserialize_with = "max_rows")]
    pub max_rows: u32,
}

impl Retention {
    pub const MAX_AGE_DAYS: (u32, u32) = (1, 365);
    pub const MAX_ROWS: (u32, u32) = (100, 100_000);

    pub fn clamped(mut self) -> Self {
        self.max_age_days = self
            .max_age_days
            .clamp(Self::MAX_AGE_DAYS.0, Self::MAX_AGE_DAYS.1);
        self.max_rows = self.max_rows.clamp(Self::MAX_ROWS.0, Self::MAX_ROWS.1);
        self
    }
}

impl Default for Retention {
    fn default() -> Self {
        Self {
            max_age_days: 30,
            max_rows: 5_000,
        }
    }
}

/// Reads the per-cluster map entry by entry, like the cluster prefs map.
fn tolerant_alert_clusters<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, ClusterAlerts>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Valid(ClusterAlerts),
        Invalid(serde::de::IgnoredAny),
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Maybe {
        Map(BTreeMap<String, Entry>),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Maybe::deserialize(deserializer)? {
        Maybe::Map(map) => map
            .into_iter()
            .filter_map(|(key, entry)| match entry {
                Entry::Valid(alerts) => Some((key, alerts)),
                Entry::Invalid(_) => None,
            })
            .collect(),
        Maybe::Other(_) => BTreeMap::new(),
    })
}

/// The `alerts` section of settings.json.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertsConfig {
    #[serde(deserialize_with = "tolerant_alert_clusters")]
    pub clusters: BTreeMap<String, ClusterAlerts>,
    /// Also send operating system notifications (default off).
    #[serde(deserialize_with = "lenient")]
    pub os_notifications: bool,
    #[serde(deserialize_with = "lenient")]
    pub retention: Retention,
    /// Repeats of the same alert inside this window are collapsed.
    #[serde(deserialize_with = "cooldown")]
    pub cooldown_secs: u32,
    /// Show a toast inside the main window when an alert fires (default on;
    /// alerts themselves stay opt-in per rule).
    #[serde(deserialize_with = "toasts_default")]
    pub in_app_toasts: bool,
    /// Play a short system sound when alerts fire (default off).
    #[serde(deserialize_with = "lenient")]
    pub sound: bool,
    /// With `sound`, only critical alerts make a sound (default off).
    #[serde(deserialize_with = "lenient")]
    pub sound_critical_only: bool,
    /// One of [`SYSTEM_SOUNDS`](Self::SYSTEM_SOUNDS).
    #[serde(deserialize_with = "sound_name")]
    pub sound_name: String,
}

impl AlertsConfig {
    /// System sounds offered in Settings. macOS plays them with `NSSound`;
    /// Windows plays the sound aliases of the current scheme.
    #[cfg(target_os = "windows")]
    pub const SYSTEM_SOUNDS: &'static [&'static str] = &[
        "SystemNotification",
        "SystemAsterisk",
        "SystemExclamation",
        "SystemHand",
        "SystemDefault",
    ];
    #[cfg(not(target_os = "windows"))]
    pub const SYSTEM_SOUNDS: &'static [&'static str] = &[
        "Glass",
        "Ping",
        "Submarine",
        "Funk",
        "Hero",
        "Pop",
        "Purr",
        "Sosumi",
        "Tink",
        "Basso",
        "Blow",
        "Bottle",
        "Frog",
        "Morse",
    ];
    pub const DEFAULT_SOUND_NAME: &'static str = Self::SYSTEM_SOUNDS[0];

    pub const COOLDOWN_SECS: (u32, u32) = (10, 86_400);
    pub const DEFAULT_COOLDOWN_SECS: u32 = 300;

    /// Alert settings of a cluster (all off when never configured).
    pub fn cluster(&self, id: &ClusterId) -> ClusterAlerts {
        self.clusters.get(id.as_str()).cloned().unwrap_or_default()
    }

    pub fn cluster_mut(&mut self, id: &ClusterId) -> &mut ClusterAlerts {
        self.clusters.entry(id.to_string()).or_default()
    }

    /// Clusters with at least one enabled rule: the only ones to watch.
    pub fn enabled_clusters(&self) -> Vec<ClusterId> {
        self.clusters
            .iter()
            .filter(|(_, alerts)| alerts.any_enabled())
            .map(|(id, _)| ClusterId::new(id.clone()))
            .collect()
    }

    pub fn clamp_in_place(&mut self) {
        self.retention = self.retention.clone().clamped();
        self.cooldown_secs = self
            .cooldown_secs
            .clamp(Self::COOLDOWN_SECS.0, Self::COOLDOWN_SECS.1);
        if !Self::SYSTEM_SOUNDS.contains(&self.sound_name.as_str()) {
            self.sound_name = Self::DEFAULT_SOUND_NAME.to_string();
        }
    }
}

/// `u32` that falls back to `fallback` when the value has the wrong shape.
fn u32_or<'de, D>(deserializer: D, fallback: u32) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Maybe {
        Value(u32),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Maybe::deserialize(deserializer)? {
        Maybe::Value(value) => value,
        Maybe::Other(_) => fallback,
    })
}

fn cooldown<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    u32_or(d, AlertsConfig::DEFAULT_COOLDOWN_SECS)
}

fn toasts_default<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Maybe {
        Value(bool),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Maybe::deserialize(d)? {
        Maybe::Value(value) => value,
        Maybe::Other(_) => true,
    })
}

fn sound_name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Maybe {
        Value(String),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Maybe::deserialize(d)? {
        Maybe::Value(value) if !value.trim().is_empty() => value,
        _ => AlertsConfig::DEFAULT_SOUND_NAME.to_string(),
    })
}

fn max_age_days<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    u32_or(d, Retention::default().max_age_days)
}

fn max_rows<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    u32_or(d, Retention::default().max_rows)
}

impl Default for AlertsConfig {
    fn default() -> Self {
        Self {
            clusters: BTreeMap::new(),
            os_notifications: false,
            retention: Retention::default(),
            cooldown_secs: Self::DEFAULT_COOLDOWN_SECS,
            in_app_toasts: true,
            sound: false,
            sound_critical_only: false,
            sound_name: Self::DEFAULT_SOUND_NAME.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_is_off_by_default() {
        let config = AlertsConfig::default();
        assert!(!config.os_notifications);
        assert!(config.enabled_clusters().is_empty());
        assert!(!config.cluster(&ClusterId::new("x")).any_enabled());
        assert_eq!(
            config.retention,
            Retention {
                max_age_days: 30,
                max_rows: 5_000
            }
        );
    }

    #[test]
    fn toasts_are_on_and_sound_is_off_by_default() {
        let config = AlertsConfig::default();
        assert!(config.in_app_toasts);
        assert!(!config.sound);
        assert!(!config.sound_critical_only);
        assert_eq!(config.sound_name, AlertsConfig::DEFAULT_SOUND_NAME);
        assert!(AlertsConfig::SYSTEM_SOUNDS.contains(&AlertsConfig::DEFAULT_SOUND_NAME));
        if cfg!(target_os = "windows") {
            assert_eq!(config.sound_name, "SystemNotification");
        } else {
            assert_eq!(config.sound_name, "Glass");
        }
    }

    #[test]
    fn old_settings_files_get_the_new_defaults() {
        let config: AlertsConfig = serde_json::from_str(r#"{ "os_notifications": true }"#).unwrap();
        assert!(config.os_notifications);
        assert!(config.in_app_toasts);
        assert!(!config.sound);
        assert_eq!(config.sound_name, AlertsConfig::DEFAULT_SOUND_NAME);
    }

    #[test]
    fn notification_fields_load_tolerantly() {
        let config: AlertsConfig = serde_json::from_str(
            r#"{ "in_app_toasts": "no", "sound": 3, "sound_critical_only": null, "sound_name": 7 }"#,
        )
        .unwrap();
        assert!(
            config.in_app_toasts,
            "a bad value falls back to the default"
        );
        assert!(!config.sound);
        assert!(!config.sound_critical_only);
        assert_eq!(config.sound_name, AlertsConfig::DEFAULT_SOUND_NAME);
        let off: AlertsConfig = serde_json::from_str(
            r#"{ "in_app_toasts": false, "sound": true, "sound_critical_only": true, "sound_name": "Ping" }"#,
        )
        .unwrap();
        assert!(!off.in_app_toasts && off.sound && off.sound_critical_only);
        assert_eq!(off.sound_name, "Ping");
    }

    #[test]
    fn clamp_replaces_an_unknown_sound_name_with_the_default() {
        let mut config = AlertsConfig {
            sound_name: "NoSuchSound".into(),
            ..AlertsConfig::default()
        };
        config.clamp_in_place();
        assert_eq!(config.sound_name, AlertsConfig::DEFAULT_SOUND_NAME);
        config.sound_name = AlertsConfig::SYSTEM_SOUNDS[1].to_string();
        config.clamp_in_place();
        assert_eq!(config.sound_name, AlertsConfig::SYSTEM_SOUNDS[1]);
    }

    #[test]
    fn rule_keys_round_trip_and_are_unique() {
        for kind in RuleKind::ALL {
            assert_eq!(RuleKind::from_key(kind.key()), Some(kind));
        }
        assert_eq!(RuleKind::from_key("nope"), None);
        let json = serde_json::to_string(&RuleKind::PodPendingTooLong).unwrap();
        assert_eq!(json, "\"pod_pending_too_long\"");
    }

    #[test]
    fn defaults_follow_the_catalog() {
        assert_eq!(RuleKind::PodRestarts.default_params().count, 3);
        assert_eq!(RuleKind::PodRestarts.default_params().window_secs, 600);
        assert_eq!(
            RuleKind::PodPendingTooLong.default_params().duration_secs,
            300
        );
        assert_eq!(
            RuleKind::DeploymentUnavailable
                .default_params()
                .duration_secs,
            120
        );
    }

    #[test]
    fn saved_params_override_defaults_and_zero_is_ignored() {
        let mut alerts = ClusterAlerts::default();
        alerts.set_rule(
            RuleKind::PodRestarts,
            RuleConfig {
                enabled: true,
                count: Some(5),
                window_secs: Some(0),
                ..RuleConfig::default()
            },
        );
        let params = alerts.params(RuleKind::PodRestarts);
        assert_eq!(params.count, 5);
        assert_eq!(params.window_secs, 600);
        assert!(alerts.is_enabled(RuleKind::PodRestarts));
        assert!(!alerts.is_enabled(RuleKind::NewPod));
    }

    #[test]
    fn only_clusters_with_an_enabled_rule_are_watched() {
        let mut config = AlertsConfig::default();
        config
            .cluster_mut(&ClusterId::new("a"))
            .set_enabled(RuleKind::NewPod, true);
        config
            .cluster_mut(&ClusterId::new("b"))
            .set_enabled(RuleKind::NewPod, false);
        assert_eq!(config.enabled_clusters(), vec![ClusterId::new("a")]);
    }

    #[test]
    fn tolerant_load_skips_bad_entries() {
        let json = r#"{
            "os_notifications": "yes",
            "cooldown_secs": "soon",
            "retention": { "max_age_days": 7, "max_rows": "many" },
            "clusters": {
                "good": { "rules": { "new_pod": { "enabled": true, "count": "x" },
                                      "bogus_rule": { "enabled": true },
                                      "pod_failing": 5 },
                           "namespaces": 3 },
                "bad": 12
            }
        }"#;
        let config: AlertsConfig = serde_json::from_str(json).unwrap();
        assert!(!config.os_notifications);
        assert_eq!(config.cooldown_secs, AlertsConfig::DEFAULT_COOLDOWN_SECS);
        assert_eq!(config.retention.max_age_days, 7);
        assert_eq!(config.retention.max_rows, 5_000);
        assert_eq!(config.clusters.len(), 1);
        let good = config.cluster(&ClusterId::new("good"));
        assert!(good.is_enabled(RuleKind::NewPod));
        assert_eq!(good.rules.len(), 1);
        assert_eq!(good.namespaces, NamespaceFilter::All);
    }

    #[test]
    fn serialization_round_trips() {
        let mut config = AlertsConfig {
            os_notifications: true,
            ..AlertsConfig::default()
        };
        let cluster = config.cluster_mut(&ClusterId::new("a#dev"));
        cluster.set_enabled(RuleKind::PodFailing, true);
        cluster.namespaces = NamespaceFilter::Only(BTreeSet::from(["apps".to_string()]));
        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(serde_json::from_str::<AlertsConfig>(&json).unwrap(), config);
    }

    #[test]
    fn clamp_brings_values_into_range() {
        let mut config = AlertsConfig {
            retention: Retention {
                max_age_days: 0,
                max_rows: 10_000_000,
            },
            cooldown_secs: 1,
            ..AlertsConfig::default()
        };
        config.clamp_in_place();
        assert_eq!(
            config.retention,
            Retention {
                max_age_days: 1,
                max_rows: 100_000
            }
        );
        assert_eq!(config.cooldown_secs, 10);
    }
}
