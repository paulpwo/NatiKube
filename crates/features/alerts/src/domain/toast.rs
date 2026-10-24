//! In-app toast queue: pure state, time passed in as milliseconds so expiry
//! and stacking are deterministic in tests.

use super::model::{Alert, StoredAlert};

/// Toasts shown at once; a newer one pushes the oldest out.
pub const MAX_TOASTS: usize = 3;
/// How long a toast stays when nobody hovers it.
pub const TOAST_TTL_MS: u64 = 6_000;

/// One toast on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastEntry {
    pub id: u64,
    /// History row of the alert, to mark it read when the toast is clicked.
    pub row_id: i64,
    pub alert: Alert,
    expires_at: u64,
    hovered: bool,
}

/// The visible toasts, oldest first (the newest sits nearest the corner).
#[derive(Debug, Default)]
pub struct ToastQueue {
    next_id: u64,
    entries: Vec<ToastEntry>,
}

impl ToastQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn items(&self) -> &[ToastEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Adds a batch (oldest alert first). Only the newest [`MAX_TOASTS`]
    /// stay visible.
    pub fn push(&mut self, alerts: &[StoredAlert], now_ms: u64) {
        for stored in alerts {
            self.next_id += 1;
            self.entries.push(ToastEntry {
                id: self.next_id,
                row_id: stored.row_id,
                alert: stored.alert.clone(),
                expires_at: now_ms.saturating_add(TOAST_TTL_MS),
                hovered: false,
            });
        }
        let excess = self.entries.len().saturating_sub(MAX_TOASTS);
        self.entries.drain(..excess);
    }

    /// Removes a toast; returns whether it existed.
    pub fn dismiss(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        self.entries.len() != before
    }

    /// A hovered toast never expires; when the pointer leaves it gets a
    /// fresh [`TOAST_TTL_MS`].
    pub fn set_hovered(&mut self, id: u64, hovered: bool, now_ms: u64) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            if entry.hovered && !hovered {
                entry.expires_at = now_ms.saturating_add(TOAST_TTL_MS);
            }
            entry.hovered = hovered;
        }
    }

    /// Drops expired toasts; returns whether anything changed.
    pub fn expire(&mut self, now_ms: u64) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|entry| entry.hovered || entry.expires_at > now_ms);
        self.entries.len() != before
    }

    /// When the next toast expires (`None` when nothing is pending or all
    /// are hovered).
    pub fn next_deadline(&self) -> Option<u64> {
        self.entries
            .iter()
            .filter(|entry| !entry.hovered)
            .map(|entry| entry.expires_at)
            .min()
    }
}

#[cfg(test)]
mod tests {
    use natikube_core::{ClusterId, RuleKind};

    use super::*;
    use crate::domain::model::Severity;

    fn stored(n: i64) -> StoredAlert {
        StoredAlert {
            row_id: n,
            alert: Alert {
                id: format!("id-{n}"),
                cluster: ClusterId::new("c"),
                rule: RuleKind::PodFailing,
                severity: Severity::Critical,
                namespace: Some("shop".into()),
                object_kind: Some("Pod".into()),
                object_name: Some(format!("pod-{n}")),
                message: "m".into(),
                timestamp: n,
                dedup_key: format!("k{n}"),
            },
            read: false,
        }
    }

    fn rows(queue: &ToastQueue) -> Vec<i64> {
        queue.items().iter().map(|t| t.row_id).collect()
    }

    #[test]
    fn a_new_toast_goes_after_the_older_ones() {
        let mut queue = ToastQueue::new();
        assert!(queue.is_empty());
        queue.push(&[stored(1)], 0);
        queue.push(&[stored(2)], 10);
        assert_eq!(rows(&queue), vec![1, 2]);
        let ids: Vec<u64> = queue.items().iter().map(|t| t.id).collect();
        assert_ne!(ids[0], ids[1], "every toast has its own id");
    }

    #[test]
    fn at_most_three_are_visible_and_the_oldest_leaves_first() {
        let mut queue = ToastQueue::new();
        for n in 1..=4 {
            queue.push(&[stored(n)], n as u64);
        }
        assert_eq!(rows(&queue), vec![2, 3, 4]);
    }

    #[test]
    fn a_big_batch_keeps_only_its_newest_alerts() {
        let mut queue = ToastQueue::new();
        queue.push(&[stored(1)], 0);
        let batch: Vec<_> = (2..=8).map(stored).collect();
        queue.push(&batch, 5);
        assert_eq!(rows(&queue), vec![6, 7, 8]);
    }

    #[test]
    fn an_empty_batch_changes_nothing() {
        let mut queue = ToastQueue::new();
        queue.push(&[], 0);
        assert!(queue.is_empty());
        assert_eq!(queue.next_deadline(), None);
    }

    #[test]
    fn toasts_expire_after_their_ttl() {
        let mut queue = ToastQueue::new();
        queue.push(&[stored(1)], 1_000);
        queue.push(&[stored(2)], 3_000);
        assert_eq!(queue.next_deadline(), Some(1_000 + TOAST_TTL_MS));
        assert!(!queue.expire(1_000 + TOAST_TTL_MS - 1));
        assert!(queue.expire(1_000 + TOAST_TTL_MS));
        assert_eq!(rows(&queue), vec![2]);
        assert_eq!(queue.next_deadline(), Some(3_000 + TOAST_TTL_MS));
        assert!(queue.expire(3_000 + TOAST_TTL_MS));
        assert!(queue.is_empty());
        assert!(!queue.expire(99_999), "nothing left to expire");
    }

    #[test]
    fn a_hovered_toast_stays_and_restarts_its_timer_on_leave() {
        let mut queue = ToastQueue::new();
        queue.push(&[stored(1)], 0);
        let id = queue.items()[0].id;
        queue.set_hovered(id, true, 2_000);
        assert_eq!(queue.next_deadline(), None);
        assert!(!queue.expire(60_000), "hover keeps it");
        queue.set_hovered(id, false, 60_000);
        assert_eq!(queue.next_deadline(), Some(60_000 + TOAST_TTL_MS));
        assert!(!queue.expire(60_000 + TOAST_TTL_MS - 1));
        assert!(queue.expire(60_000 + TOAST_TTL_MS));
    }

    #[test]
    fn hover_on_an_unknown_toast_is_ignored() {
        let mut queue = ToastQueue::new();
        queue.push(&[stored(1)], 0);
        queue.set_hovered(999, true, 10);
        assert_eq!(queue.next_deadline(), Some(TOAST_TTL_MS));
    }

    #[test]
    fn dismiss_removes_only_that_toast() {
        let mut queue = ToastQueue::new();
        queue.push(&[stored(1), stored(2)], 0);
        let first = queue.items()[0].id;
        assert!(queue.dismiss(first));
        assert!(!queue.dismiss(first), "already gone");
        assert_eq!(rows(&queue), vec![2]);
    }

    #[test]
    fn ids_are_never_reused_after_eviction() {
        let mut queue = ToastQueue::new();
        queue.push(&[stored(1), stored(2), stored(3)], 0);
        let first = queue.items()[0].id;
        queue.push(&[stored(4)], 1);
        assert!(!queue.dismiss(first), "evicted toast is gone");
        assert!(queue.items().iter().all(|t| t.id != first));
    }
}
