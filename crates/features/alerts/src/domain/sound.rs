//! When an alert batch makes a sound: the severity filter and the throttle.
//! Pure; the system call lives behind the `SoundPlayer` port.

use natikube_core::AlertsConfig;

use super::model::{Severity, StoredAlert};

/// A sound never plays more than once in this window.
pub const SOUND_MIN_INTERVAL_MS: u64 = 2_000;

/// Remembers when the last sound played.
#[derive(Debug, Default)]
pub struct SoundThrottle {
    last_played_ms: Option<u64>,
}

impl SoundThrottle {
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` (and the window restarts) when enough time passed since the
    /// last sound.
    pub fn allow(&mut self, now_ms: u64) -> bool {
        let allowed = self
            .last_played_ms
            .is_none_or(|last| now_ms.saturating_sub(last) >= SOUND_MIN_INTERVAL_MS);
        if allowed {
            self.last_played_ms = Some(now_ms);
        }
        allowed
    }
}

/// What to do for one batch of fired alerts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchPlan {
    /// Show the alerts as in-app toasts.
    pub toasts: bool,
    /// System sound to play once for the whole batch.
    pub sound: Option<String>,
}

/// Decides toasts and sound for a batch. The sound plays once per batch, only
/// when the setting is on and (with "only critical") the batch holds a
/// critical alert, and never inside the throttle window. A batch that does
/// not qualify does not use up the window.
pub fn plan_batch(
    config: &AlertsConfig,
    throttle: &mut SoundThrottle,
    now_ms: u64,
    alerts: &[StoredAlert],
) -> BatchPlan {
    if alerts.is_empty() {
        return BatchPlan {
            toasts: false,
            sound: None,
        };
    }
    let qualifies = alerts
        .iter()
        .any(|stored| !config.sound_critical_only || stored.alert.severity == Severity::Critical);
    let sound =
        (config.sound && qualifies && throttle.allow(now_ms)).then(|| config.sound_name.clone());
    BatchPlan {
        toasts: config.in_app_toasts,
        sound,
    }
}

#[cfg(test)]
mod tests {
    use natikube_core::{ClusterId, RuleKind};

    use super::*;
    use crate::domain::model::Alert;

    fn stored(severity: Severity) -> StoredAlert {
        StoredAlert {
            row_id: 1,
            alert: Alert {
                id: "a".into(),
                cluster: ClusterId::new("c"),
                rule: RuleKind::NewPod,
                severity,
                namespace: None,
                object_kind: None,
                object_name: None,
                message: "m".into(),
                timestamp: 0,
                dedup_key: "k".into(),
            },
            read: false,
        }
    }

    fn config(sound: bool, critical_only: bool) -> AlertsConfig {
        AlertsConfig {
            sound,
            sound_critical_only: critical_only,
            sound_name: "Ping".into(),
            ..AlertsConfig::default()
        }
    }

    #[test]
    fn the_first_sound_is_allowed_then_the_window_applies() {
        let mut throttle = SoundThrottle::new();
        assert!(throttle.allow(10_000));
        assert!(!throttle.allow(10_000 + SOUND_MIN_INTERVAL_MS - 1));
        assert!(throttle.allow(10_000 + SOUND_MIN_INTERVAL_MS));
        assert!(!throttle.allow(10_000 + SOUND_MIN_INTERVAL_MS + 1));
    }

    #[test]
    fn a_clock_that_goes_backwards_does_not_panic_or_stick() {
        let mut throttle = SoundThrottle::new();
        assert!(throttle.allow(10_000));
        assert!(!throttle.allow(5_000));
    }

    #[test]
    fn sound_is_off_by_default() {
        let mut throttle = SoundThrottle::new();
        let plan = plan_batch(
            &AlertsConfig::default(),
            &mut throttle,
            0,
            &[stored(Severity::Critical)],
        );
        assert_eq!(plan.sound, None);
        assert!(plan.toasts, "toasts are on by default");
    }

    #[test]
    fn a_batch_plays_the_chosen_sound_once() {
        let mut throttle = SoundThrottle::new();
        let batch = vec![stored(Severity::Info); 5];
        let plan = plan_batch(&config(true, false), &mut throttle, 0, &batch);
        assert_eq!(plan.sound.as_deref(), Some("Ping"));
    }

    #[test]
    fn a_second_batch_inside_the_window_is_silent() {
        let mut throttle = SoundThrottle::new();
        let batch = [stored(Severity::Warning)];
        assert!(
            plan_batch(&config(true, false), &mut throttle, 0, &batch)
                .sound
                .is_some()
        );
        assert!(
            plan_batch(&config(true, false), &mut throttle, 1_500, &batch)
                .sound
                .is_none()
        );
        assert!(
            plan_batch(&config(true, false), &mut throttle, 2_000, &batch)
                .sound
                .is_some()
        );
    }

    #[test]
    fn only_critical_ignores_lesser_batches() {
        let mut throttle = SoundThrottle::new();
        let quiet = [stored(Severity::Info), stored(Severity::Warning)];
        let plan = plan_batch(&config(true, true), &mut throttle, 0, &quiet);
        assert_eq!(plan.sound, None);
        let loud = [stored(Severity::Info), stored(Severity::Critical)];
        let plan = plan_batch(&config(true, true), &mut throttle, 100, &loud);
        assert!(plan.sound.is_some(), "one critical alert is enough");
    }

    #[test]
    fn a_filtered_batch_does_not_use_up_the_window() {
        let mut throttle = SoundThrottle::new();
        let cfg = config(true, true);
        plan_batch(&cfg, &mut throttle, 0, &[stored(Severity::Info)]);
        let plan = plan_batch(&cfg, &mut throttle, 10, &[stored(Severity::Critical)]);
        assert!(plan.sound.is_some());
    }

    #[test]
    fn toasts_follow_their_own_switch() {
        let mut throttle = SoundThrottle::new();
        let cfg = AlertsConfig {
            in_app_toasts: false,
            ..config(true, false)
        };
        let plan = plan_batch(&cfg, &mut throttle, 0, &[stored(Severity::Info)]);
        assert!(!plan.toasts);
        assert!(plan.sound.is_some(), "sound does not depend on toasts");
    }

    #[test]
    fn an_empty_batch_does_nothing() {
        let mut throttle = SoundThrottle::new();
        let plan = plan_batch(&config(true, false), &mut throttle, 0, &[]);
        assert_eq!(
            plan,
            BatchPlan {
                toasts: false,
                sound: None
            }
        );
    }
}
