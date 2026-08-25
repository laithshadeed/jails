//! Debounce and overflow state for test watch.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

pub(super) const POLL: Duration = Duration::from_millis(25);
const QUIET: Duration = Duration::from_millis(75);
const MAX_WAIT: Duration = Duration::from_millis(500);

#[derive(Default)]
pub(super) struct Batch {
    first: Option<Instant>,
    last: Option<Instant>,
    changes: BTreeSet<String>,
    overflow: bool,
}

impl Batch {
    pub(super) fn observe(&mut self, now: Instant, changes: Vec<String>, overflow: bool) {
        self.first.get_or_insert(now);
        self.last = Some(now);
        self.changes.extend(changes);
        self.overflow |= overflow;
    }

    pub(super) fn due(&self, now: Instant) -> bool {
        self.first
            .is_some_and(|first| now.duration_since(first) >= MAX_WAIT)
            || self
                .last
                .is_some_and(|last| now.duration_since(last) >= QUIET)
    }

    pub(super) fn take(&mut self) -> (Vec<String>, bool) {
        let changes = std::mem::take(&mut self.changes).into_iter().collect();
        let overflow = self.overflow;
        self.first = None;
        self.last = None;
        self.overflow = false;
        (changes, overflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quiet_edit_waits_75_milliseconds() {
        let start = Instant::now();
        let mut batch = Batch::default();
        batch.observe(start, vec!["changed A.java".into()], false);
        assert!(!batch.due(start + Duration::from_millis(74)));
        assert!(batch.due(start + Duration::from_millis(75)));
    }

    #[test]
    fn a_continuous_stream_is_forced_through_at_500_milliseconds() {
        let start = Instant::now();
        let mut batch = Batch::default();
        batch.observe(start, vec!["changed A.java".into()], false);
        for elapsed in [60, 120, 180, 240, 300, 360, 420, 480] {
            batch.observe(
                start + Duration::from_millis(elapsed),
                vec!["changed A.java".into()],
                false,
            );
        }
        assert!(!batch.due(start + Duration::from_millis(499)));
        assert!(batch.due(start + Duration::from_millis(500)));
    }

    #[test]
    fn overflow_and_changes_survive_until_the_batch_is_taken() {
        let start = Instant::now();
        let mut batch = Batch::default();
        batch.observe(start, vec!["changed B.java".into()], true);
        batch.observe(start, vec!["changed A.java".into()], false);
        assert_eq!(
            batch.take(),
            (
                vec!["changed A.java".to_string(), "changed B.java".to_string()],
                true
            )
        );
        assert!(batch.first.is_none());
    }
}
