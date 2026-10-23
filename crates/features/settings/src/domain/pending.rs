//! Debounce bookkeeping for free-text fields: typing updates the field at
//! once but the resulting preference edit is applied (and persisted) only
//! after the user pauses, instead of once per keystroke.

use std::mem::discriminant;
use std::time::Duration;

use super::PrefEdit;

/// How long typing must pause before pending text edits are applied.
pub const TEXT_DEBOUNCE: Duration = Duration::from_millis(400);

/// Text edits waiting for the debounce timer. One slot per kind of edit: a
/// newer edit of the same kind replaces the older one.
#[derive(Debug, Default)]
pub struct PendingEdits {
    generation: u64,
    edits: Vec<PrefEdit>,
}

impl PendingEdits {
    /// Queues `edit` and returns the generation its timer must present to
    /// [`take_if_current`](Self::take_if_current).
    pub fn push(&mut self, edit: PrefEdit) -> u64 {
        self.edits
            .retain(|queued| discriminant(queued) != discriminant(&edit));
        self.edits.push(edit);
        self.generation += 1;
        self.generation
    }

    /// The queued edits when no newer edit arrived since `generation` was
    /// issued (the user paused typing); `None` when the timer is stale.
    pub fn take_if_current(&mut self, generation: u64) -> Option<Vec<PrefEdit>> {
        (generation == self.generation && !self.edits.is_empty())
            .then(|| std::mem::take(&mut self.edits))
    }

    /// Everything queued, now (focus left a field or the window is closing).
    pub fn take_all(&mut self) -> Vec<PrefEdit> {
        self.generation += 1;
        std::mem::take(&mut self.edits)
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_last_keystroke_timer_fires() {
        let mut pending = PendingEdits::default();
        let first = pending.push(PrefEdit::Shell("p".into()));
        let second = pending.push(PrefEdit::Shell("pw".into()));
        assert_eq!(pending.take_if_current(first), None);
        assert_eq!(
            pending.take_if_current(second),
            Some(vec![PrefEdit::Shell("pw".into())])
        );
        assert!(pending.is_empty());
    }

    #[test]
    fn different_fields_are_kept_side_by_side() {
        let mut pending = PendingEdits::default();
        pending.push(PrefEdit::Shell("pwsh".into()));
        let last = pending.push(PrefEdit::DefaultNamespace("shop".into()));
        let edits = pending.take_if_current(last).unwrap();
        assert_eq!(edits.len(), 2);
    }

    #[test]
    fn take_all_flushes_and_invalidates_running_timers() {
        let mut pending = PendingEdits::default();
        let generation = pending.push(PrefEdit::MonoFont("Menlo".into()));
        assert_eq!(pending.take_all(), vec![PrefEdit::MonoFont("Menlo".into())]);
        assert_eq!(pending.take_if_current(generation), None);
        assert!(pending.take_all().is_empty());
    }
}
