//! Runtime timings for one command.
//!
//! Timings describe how an invocation ran; they do not describe the change it
//! prepared. Keeping them in this separate, shared trace prevents clock noise
//! from entering operation identities, journals, receipts, or object hashes.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// A stable phase name shared by human and JSON reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimingPhase {
    Discover,
    Observe,
    Parse,
    Project,
    Prepare,
    Verify,
    Commit,
    Process,
    Container,
}

impl TimingPhase {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Discover => "discover",
            Self::Observe => "observe",
            Self::Parse => "parse",
            Self::Project => "project",
            Self::Prepare => "prepare",
            Self::Verify => "verify",
            Self::Commit => "commit",
            Self::Process => "process",
            Self::Container => "container",
        }
    }
}

/// One completed piece of work, measured with a monotonic clock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimingSpan {
    pub phase: TimingPhase,
    pub duration_micros: u64,
}

/// The invocation-local collector shared across preparation layers.
#[derive(Clone, Debug, Default)]
pub struct TimingTrace {
    spans: Arc<Mutex<Vec<TimingSpan>>>,
}

impl TimingTrace {
    /// Measure a closure, recording the phase whether its returned value is
    /// success or failure. The caller decides how that value is interpreted.
    pub fn measure<T>(&self, phase: TimingPhase, work: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let value = work();
        self.record(phase, started.elapsed());
        value
    }

    pub fn record(&self, phase: TimingPhase, elapsed: Duration) {
        self.lock().push(TimingSpan {
            phase,
            duration_micros: elapsed.as_micros().min(u128::from(u64::MAX)) as u64,
        });
    }

    pub fn spans(&self) -> Vec<TimingSpan> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<TimingSpan>> {
        self.spans
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_are_recorded_in_completion_order() {
        let trace = TimingTrace::default();
        trace.record(TimingPhase::Discover, Duration::from_micros(7));
        assert_eq!(trace.measure(TimingPhase::Parse, || 42), 42);

        let spans = trace.spans();
        assert_eq!(spans[0].phase, TimingPhase::Discover);
        assert_eq!(spans[0].duration_micros, 7);
        assert_eq!(spans[1].phase, TimingPhase::Parse);
    }
}
