//! Pure presentation logic for the bell: no GPUI, no I/O.

use natikube_core::ClusterId;

use crate::domain::{Alert, StoredAlert, Timestamp};

/// How many history rows the panel loads at once.
pub const PAGE_SIZE: u32 = 200;

const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

/// Text of the unread badge: hidden at zero, capped at `99+`.
pub fn badge_text(unread: u32) -> Option<String> {
    match unread {
        0 => None,
        1..=99 => Some(unread.to_string()),
        _ => Some("99+".to_string()),
    }
}

/// "just now", "5m ago", "3h ago", "2d ago", then a week count. Timestamps in
/// the future (clock skew) read as "just now".
pub fn relative_time(now: Timestamp, then: Timestamp) -> String {
    let secs = now.saturating_sub(then).max(0);
    if secs < MINUTE {
        "just now".to_string()
    } else if secs < HOUR {
        format!("{}m ago", secs / MINUTE)
    } else if secs < DAY {
        format!("{}h ago", secs / HOUR)
    } else if secs < 7 * DAY {
        format!("{}d ago", secs / DAY)
    } else {
        format!("{}w ago", secs / (7 * DAY))
    }
}

/// Calendar bucket an alert is listed under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    Today,
    Yesterday,
    Earlier,
}

impl Bucket {
    pub fn label(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Yesterday => "Yesterday",
            Self::Earlier => "Earlier",
        }
    }
}

/// Bucket of `then` relative to `now`, using local days (`utc_offset_secs` is
/// the local offset from UTC). Future timestamps count as today.
pub fn bucket(now: Timestamp, then: Timestamp, utc_offset_secs: i64) -> Bucket {
    let today = (now + utc_offset_secs).div_euclid(DAY);
    let day = (then + utc_offset_secs).div_euclid(DAY);
    match today - day {
        d if d <= 0 => Bucket::Today,
        1 => Bucket::Yesterday,
        _ => Bucket::Earlier,
    }
}

/// A run of consecutive items sharing a bucket: `items[start..end]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Group {
    pub bucket: Bucket,
    pub start: usize,
    pub end: usize,
}

/// Splits a newest-first list into consecutive day groups.
pub fn group_by_day(items: &[StoredAlert], now: Timestamp, utc_offset_secs: i64) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (ix, item) in items.iter().enumerate() {
        let b = bucket(now, item.alert.timestamp, utc_offset_secs);
        match groups.last_mut() {
            Some(last) if last.bucket == b => last.end = ix + 1,
            _ => groups.push(Group {
                bucket: b,
                start: ix,
                end: ix + 1,
            }),
        }
    }
    groups
}

/// `namespace/name`, the bare name (cluster-scoped objects) or the kind alone.
pub fn subject(alert: &Alert) -> Option<String> {
    match (&alert.namespace, &alert.object_name, &alert.object_kind) {
        (Some(ns), Some(name), _) => Some(format!("{ns}/{name}")),
        (None, Some(name), Some(kind)) => Some(format!("{kind} {name}")),
        (None, Some(name), None) => Some(name.clone()),
        (_, None, Some(kind)) => Some(kind.clone()),
        _ => None,
    }
}

/// What the shell needs to switch cluster and open the object. Mirrors the
/// core `AlertTarget` the integrator maps it to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BellTarget {
    pub cluster: ClusterId,
    pub namespace: Option<String>,
    pub kind: Option<String>,
    pub name: Option<String>,
}

impl From<&Alert> for BellTarget {
    fn from(alert: &Alert) -> Self {
        Self {
            cluster: alert.cluster.clone(),
            namespace: alert.namespace.clone(),
            kind: alert.object_kind.clone(),
            name: alert.object_name.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use natikube_core::RuleKind;

    use crate::domain::{Severity, rule_severity};

    fn stored(
        ts: Timestamp,
        ns: Option<&str>,
        kind: Option<&str>,
        name: Option<&str>,
    ) -> StoredAlert {
        StoredAlert {
            row_id: ts,
            read: false,
            alert: Alert {
                id: ts.to_string(),
                cluster: ClusterId::new("/k#dev"),
                rule: RuleKind::PodFailing,
                severity: rule_severity(RuleKind::PodFailing),
                namespace: ns.map(String::from),
                object_kind: kind.map(String::from),
                object_name: name.map(String::from),
                message: "boom".into(),
                timestamp: ts,
                dedup_key: ts.to_string(),
            },
        }
    }

    #[test]
    fn badge_hidden_at_zero_and_capped() {
        assert_eq!(badge_text(0), None);
        assert_eq!(badge_text(1).as_deref(), Some("1"));
        assert_eq!(badge_text(99).as_deref(), Some("99"));
        assert_eq!(badge_text(100).as_deref(), Some("99+"));
        assert_eq!(badge_text(u32::MAX).as_deref(), Some("99+"));
    }

    #[test]
    fn relative_time_steps() {
        let now = 1_000_000;
        assert_eq!(relative_time(now, now), "just now");
        assert_eq!(relative_time(now, now - 59), "just now");
        assert_eq!(relative_time(now, now - 60), "1m ago");
        assert_eq!(relative_time(now, now - 3599), "59m ago");
        assert_eq!(relative_time(now, now - 3600), "1h ago");
        assert_eq!(relative_time(now, now - 86_399), "23h ago");
        assert_eq!(relative_time(now, now - 86_400), "1d ago");
        assert_eq!(relative_time(now, now - 6 * 86_400), "6d ago");
        assert_eq!(relative_time(now, now - 14 * 86_400), "2w ago");
    }

    #[test]
    fn relative_time_future_is_just_now() {
        assert_eq!(relative_time(100, 5_000), "just now");
        assert_eq!(relative_time(i64::MIN, i64::MAX), "just now");
    }

    #[test]
    fn bucket_uses_local_days() {
        // 2024-01-02 01:00 UTC; in UTC-5 it is still Jan 1, 20:00.
        let now = 1_704_157_200;
        let two_hours_ago = now - 2 * HOUR;
        assert_eq!(bucket(now, two_hours_ago, 0), Bucket::Yesterday);
        // At UTC-5, "now" is Jan 1 20:00 and 2h ago is Jan 1 18:00: same day.
        assert_eq!(bucket(now, two_hours_ago, -5 * HOUR), Bucket::Today);
        assert_eq!(bucket(now, now - 4 * HOUR, 0), Bucket::Yesterday);
        assert_eq!(bucket(now, now - 3 * DAY, 0), Bucket::Earlier);
        assert_eq!(bucket(now, now + HOUR, 0), Bucket::Today);
    }

    #[test]
    fn groups_split_consecutive_buckets() {
        let now = 10 * DAY + 12 * HOUR;
        let items = vec![
            stored(now - 10, None, None, None),
            stored(now - HOUR, None, None, None),
            stored(now - DAY, None, None, None),
            stored(now - 3 * DAY, None, None, None),
            stored(now - 4 * DAY, None, None, None),
        ];
        let groups = group_by_day(&items, now, 0);
        assert_eq!(
            groups,
            vec![
                Group {
                    bucket: Bucket::Today,
                    start: 0,
                    end: 2
                },
                Group {
                    bucket: Bucket::Yesterday,
                    start: 2,
                    end: 3
                },
                Group {
                    bucket: Bucket::Earlier,
                    start: 3,
                    end: 5
                },
            ]
        );
        assert!(group_by_day(&[], now, 0).is_empty());
    }

    #[test]
    fn subject_variants() {
        let a = stored(1, Some("web"), Some("Pod"), Some("api-1")).alert;
        assert_eq!(subject(&a).as_deref(), Some("web/api-1"));
        let a = stored(1, None, Some("Node"), Some("node-a")).alert;
        assert_eq!(subject(&a).as_deref(), Some("Node node-a"));
        let a = stored(1, None, None, Some("x")).alert;
        assert_eq!(subject(&a).as_deref(), Some("x"));
        let a = stored(1, None, Some("Pod"), None).alert;
        assert_eq!(subject(&a).as_deref(), Some("Pod"));
        let a = stored(1, None, None, None).alert;
        assert_eq!(subject(&a), None);
    }

    #[test]
    fn target_copies_navigation_fields() {
        let a = stored(1, Some("web"), Some("Pod"), Some("api-1")).alert;
        let t = BellTarget::from(&a);
        assert_eq!(t.cluster, ClusterId::new("/k#dev"));
        assert_eq!(t.namespace.as_deref(), Some("web"));
        assert_eq!(t.kind.as_deref(), Some("Pod"));
        assert_eq!(t.name.as_deref(), Some("api-1"));
        assert_eq!(a.severity, Severity::Critical);
    }
}
