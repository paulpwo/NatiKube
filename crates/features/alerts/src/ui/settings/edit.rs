//! Pure edit logic of the alerts settings: every control maps to one
//! [`AlertsEdit`], applied over an [`AlertsConfig`] copy. No GPUI here, so it
//! is unit tested.

use std::collections::BTreeSet;
use std::ops::RangeInclusive;

use natikube_core::{AlertsConfig, ClusterId, NamespaceFilter, Retention, RuleConfig, RuleKind};

/// Bounds of the numeric rule parameters (the evaluator treats 0 as unset).
pub const COUNT_RANGE: RangeInclusive<u32> = 1..=100;
pub const WINDOW_SECS_RANGE: RangeInclusive<u32> = 60..=86_400;
pub const DURATION_SECS_RANGE: RangeInclusive<u32> = 30..=86_400;

/// One change made in the section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertsEdit {
    RuleEnabled(ClusterId, RuleKind, bool),
    RestartCount(ClusterId, u32),
    WindowSecs(ClusterId, u32),
    /// Applies to the rule it names (pending pods or deployments).
    DurationSecs(ClusterId, RuleKind, u32),
    /// Warning event reasons, as typed (comma or space separated).
    Reasons(ClusterId, String),
    /// Namespace filter, as typed (comma or space separated, empty: all).
    Namespaces(ClusterId, String),
    OsNotifications(bool),
    InAppToasts(bool),
    Sound(bool),
    SoundCriticalOnly(bool),
    /// One of `AlertsConfig::SYSTEM_SOUNDS`; anything else is ignored.
    SoundName(String),
    RetentionDays(u32),
    RetentionRows(u32),
    CooldownSecs(u32),
}

/// Returns `config` with `edit` applied. Values are kept inside their allowed
/// range; the shell still clamps before persisting.
pub fn apply_edit(config: &AlertsConfig, edit: AlertsEdit) -> AlertsConfig {
    let mut next = config.clone();
    match edit {
        AlertsEdit::RuleEnabled(cluster, kind, on) => {
            next.cluster_mut(&cluster).set_enabled(kind, on);
        }
        AlertsEdit::RestartCount(cluster, value) => {
            rule_mut(&mut next, &cluster, RuleKind::PodRestarts).count =
                Some(clamp(value, &COUNT_RANGE));
        }
        AlertsEdit::WindowSecs(cluster, value) => {
            rule_mut(&mut next, &cluster, RuleKind::PodRestarts).window_secs =
                Some(clamp(value, &WINDOW_SECS_RANGE));
        }
        AlertsEdit::DurationSecs(cluster, kind, value) => {
            rule_mut(&mut next, &cluster, kind).duration_secs =
                Some(clamp(value, &DURATION_SECS_RANGE));
        }
        AlertsEdit::Reasons(cluster, text) => {
            rule_mut(&mut next, &cluster, RuleKind::WarningEvents).reasons = parse_list(&text);
        }
        AlertsEdit::Namespaces(cluster, text) => {
            let set = parse_list(&text);
            next.cluster_mut(&cluster).namespaces = if set.is_empty() {
                NamespaceFilter::All
            } else {
                NamespaceFilter::Only(set)
            };
        }
        AlertsEdit::OsNotifications(on) => next.os_notifications = on,
        AlertsEdit::InAppToasts(on) => next.in_app_toasts = on,
        AlertsEdit::Sound(on) => next.sound = on,
        AlertsEdit::SoundCriticalOnly(on) => next.sound_critical_only = on,
        AlertsEdit::SoundName(name) => {
            if AlertsConfig::SYSTEM_SOUNDS.contains(&name.as_str()) {
                next.sound_name = name;
            }
        }
        AlertsEdit::RetentionDays(days) => {
            next.retention.max_age_days = clamp(
                days,
                &(Retention::MAX_AGE_DAYS.0..=Retention::MAX_AGE_DAYS.1),
            );
        }
        AlertsEdit::RetentionRows(rows) => {
            next.retention.max_rows = clamp(rows, &(Retention::MAX_ROWS.0..=Retention::MAX_ROWS.1));
        }
        AlertsEdit::CooldownSecs(secs) => {
            next.cooldown_secs = clamp(
                secs,
                &(AlertsConfig::COOLDOWN_SECS.0..=AlertsConfig::COOLDOWN_SECS.1),
            );
        }
    }
    next
}

fn rule_mut<'a>(
    config: &'a mut AlertsConfig,
    cluster: &ClusterId,
    kind: RuleKind,
) -> &'a mut RuleConfig {
    config.cluster_mut(cluster).rules.entry(kind).or_default()
}

fn clamp(value: u32, range: &RangeInclusive<u32>) -> u32 {
    value.clamp(*range.start(), *range.end())
}

/// `current` moved by `delta`, kept inside `range`.
pub fn step(current: u32, delta: i64, range: &RangeInclusive<u32>) -> u32 {
    let value =
        (i64::from(current) + delta).clamp(i64::from(*range.start()), i64::from(*range.end()));
    value as u32
}

/// Splits typed text on commas, semicolons and whitespace.
pub fn parse_list(text: &str) -> BTreeSet<String> {
    text.split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

/// Text shown in the reasons and namespaces inputs.
pub fn format_list(items: &BTreeSet<String>) -> String {
    items.iter().cloned().collect::<Vec<_>>().join(", ")
}

/// Namespace filter as input text (empty: all namespaces).
pub fn namespaces_text(filter: &NamespaceFilter) -> String {
    match filter {
        NamespaceFilter::All => String::new(),
        NamespaceFilter::Only(set) => format_list(set),
    }
}

/// "45 s", "10 min", "2 h", "1 h 30 min".
pub fn format_duration(secs: u32) -> String {
    if secs < 60 {
        return format!("{secs} s");
    }
    let (hours, minutes, rest) = (secs / 3600, secs % 3600 / 60, secs % 60);
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!("{hours} h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes} min"));
    }
    if rest > 0 {
        parts.push(format!("{rest} s"));
    }
    parts.join(" ")
}

/// Step size of a duration stepper: finer while the value is small.
pub fn duration_step(current: u32) -> i64 {
    if current < 300 {
        30
    } else if current < 3600 {
        60
    } else {
        600
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> ClusterId {
        ClusterId::new("k3s-dev")
    }

    #[test]
    fn toggling_a_rule_enables_only_that_rule() {
        let next = apply_edit(
            &AlertsConfig::default(),
            AlertsEdit::RuleEnabled(id(), RuleKind::PodFailing, true),
        );
        let cluster = next.cluster(&id());
        assert!(cluster.is_enabled(RuleKind::PodFailing));
        assert!(!cluster.is_enabled(RuleKind::NewPod));
        assert_eq!(next.enabled_clusters(), vec![id()]);
    }

    #[test]
    fn disabling_the_last_rule_stops_watching_the_cluster() {
        let on = apply_edit(
            &AlertsConfig::default(),
            AlertsEdit::RuleEnabled(id(), RuleKind::NewPod, true),
        );
        let off = apply_edit(&on, AlertsEdit::RuleEnabled(id(), RuleKind::NewPod, false));
        assert!(off.enabled_clusters().is_empty());
    }

    #[test]
    fn parameters_are_stored_clamped_and_keep_the_rule_state() {
        let mut config = apply_edit(
            &AlertsConfig::default(),
            AlertsEdit::RuleEnabled(id(), RuleKind::PodRestarts, true),
        );
        config = apply_edit(&config, AlertsEdit::RestartCount(id(), 0));
        config = apply_edit(&config, AlertsEdit::WindowSecs(id(), 10_000_000));
        let params = config.cluster(&id()).params(RuleKind::PodRestarts);
        assert_eq!(params.count, 1);
        assert_eq!(params.window_secs, 86_400);
        assert!(config.cluster(&id()).is_enabled(RuleKind::PodRestarts));
    }

    #[test]
    fn duration_applies_to_the_named_rule_only() {
        let config = apply_edit(
            &AlertsConfig::default(),
            AlertsEdit::DurationSecs(id(), RuleKind::DeploymentUnavailable, 600),
        );
        let cluster = config.cluster(&id());
        assert_eq!(
            cluster
                .params(RuleKind::DeploymentUnavailable)
                .duration_secs,
            600
        );
        assert_eq!(
            cluster.params(RuleKind::PodPendingTooLong).duration_secs,
            300
        );
    }

    #[test]
    fn editing_a_parameter_does_not_enable_the_rule() {
        let config = apply_edit(&AlertsConfig::default(), AlertsEdit::RestartCount(id(), 5));
        assert!(!config.cluster(&id()).is_enabled(RuleKind::PodRestarts));
    }

    #[test]
    fn reasons_and_namespaces_parse_lists() {
        let config = apply_edit(
            &AlertsConfig::default(),
            AlertsEdit::Reasons(id(), "BackOff, FailedMount  Unhealthy;".into()),
        );
        let reasons = config
            .cluster(&id())
            .params(RuleKind::WarningEvents)
            .reasons;
        assert_eq!(format_list(&reasons), "BackOff, FailedMount, Unhealthy");

        let config = apply_edit(
            &config,
            AlertsEdit::Namespaces(id(), "shop, billing".into()),
        );
        assert_eq!(
            namespaces_text(&config.cluster(&id()).namespaces),
            "billing, shop"
        );
        let config = apply_edit(&config, AlertsEdit::Namespaces(id(), "  ".into()));
        assert_eq!(config.cluster(&id()).namespaces, NamespaceFilter::All);
    }

    #[test]
    fn global_options_are_clamped() {
        let mut config = AlertsConfig::default();
        assert!(!config.os_notifications, "OS notifications start off");
        config = apply_edit(&config, AlertsEdit::OsNotifications(true));
        config = apply_edit(&config, AlertsEdit::RetentionDays(0));
        config = apply_edit(&config, AlertsEdit::RetentionRows(u32::MAX));
        config = apply_edit(&config, AlertsEdit::CooldownSecs(1));
        assert!(config.os_notifications);
        assert_eq!(config.retention.max_age_days, 1);
        assert_eq!(config.retention.max_rows, 100_000);
        assert_eq!(config.cooldown_secs, 10);
    }

    #[test]
    fn toast_and_sound_switches_edit_only_their_own_field() {
        let base = AlertsConfig::default();
        let off = apply_edit(&base, AlertsEdit::InAppToasts(false));
        assert!(!off.in_app_toasts && !off.sound);
        let sound = apply_edit(&base, AlertsEdit::Sound(true));
        assert!(sound.sound && sound.in_app_toasts && !sound.sound_critical_only);
        let critical = apply_edit(&sound, AlertsEdit::SoundCriticalOnly(true));
        assert!(critical.sound && critical.sound_critical_only);
    }

    #[test]
    fn the_sound_name_must_be_a_known_system_sound() {
        let base = AlertsConfig::default();
        let other = AlertsConfig::SYSTEM_SOUNDS[2];
        let picked = apply_edit(&base, AlertsEdit::SoundName(other.into()));
        assert_eq!(picked.sound_name, other);
        let rejected = apply_edit(&picked, AlertsEdit::SoundName("Nope".into()));
        assert_eq!(rejected.sound_name, other, "unknown names are ignored");
    }

    #[test]
    fn step_stays_inside_the_range() {
        assert_eq!(step(1, -1, &COUNT_RANGE), 1);
        assert_eq!(step(100, 1, &COUNT_RANGE), 100);
        assert_eq!(step(60, 60, &WINDOW_SECS_RANGE), 120);
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(format_duration(45), "45 s");
        assert_eq!(format_duration(600), "10 min");
        assert_eq!(format_duration(5400), "1 h 30 min");
        assert_eq!(format_duration(7200), "2 h");
    }
}
