//! Where the suite's parallelism comes from, and the one place that decides
//! how wide it may get.
//!
//! Libtest parallelises per `#[test]` function, which is the wrong grain for a
//! table-driven test: each cell of `agreement.rs` or `golden.rs` is an
//! independent temporary directory and its own `jails` processes, so the table
//! loop gets its own scheduler. Three properties are the design:
//!
//! - **Work stealing, not chunking.** Workers pull the next index off one
//!   atomic cursor. Cell costs differ by more than an order of magnitude, and
//!   a static `chunks(n)` split hands one worker every expensive cell it
//!   happened to be given and leaves the rest idle for the whole tail.
//! - **One process-wide budget.** Libtest runs several test functions at once
//!   and each may open a scheduler of its own; every unit of work takes a
//!   permit from [`GATE`], so the *sum* across every concurrent table is
//!   bounded however many tables there are.
//! - **Longest-processing-time first.** With work stealing, makespan is set by
//!   whichever item starts last, so the expensive cells go first. That needs
//!   a cost estimate, which [`CostLedger`] measures rather than guesses.
//!
//! **The budget deliberately exceeds the core count.** These units spend a
//! large share of their time in `fork`/`exec`, the child's dynamic linking and
//! page faults rather than on a core, so one worker per core leaves the
//! machine idle waiting on the kernel. The multiplier is measured, not
//! guessed, and `JAILS_TEST_PARALLELISM` overrides it for a machine where it
//! is wrong.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Instant;

/// Units of work this process may have in flight at once, across every
/// concurrent [`map`] call.
///
/// See the module docs for why this is a multiple of the core count rather
/// than equal to it.
pub fn budget() -> usize {
    static BUDGET: OnceLock<usize> = OnceLock::new();
    *BUDGET.get_or_init(|| {
        if let Some(configured) = std::env::var("JAILS_TEST_PARALLELISM")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
        {
            return configured;
        }
        let cores = std::thread::available_parallelism().map_or(4, |value| value.get());
        // Four units per core, and capped: the gain is in covering process
        // start-up latency, and past a few dozen in-flight process trees the
        // memory and scheduler cost starts taking it back.
        (cores * 4).clamp(4, 32)
    })
}

/// The process-wide permit gate. See the module docs.
static GATE: Gate = Gate::new();

struct Gate {
    in_flight: Mutex<usize>,
    released: Condvar,
}

impl Gate {
    const fn new() -> Self {
        Self {
            in_flight: Mutex::new(0),
            released: Condvar::new(),
        }
    }

    /// A permit, blocking until one is free.
    ///
    /// Lock poisoning is recovered from rather than propagated: a worker
    /// panicking is an ordinary test failure here, and turning it into a
    /// second panic in every other worker would replace the real assertion
    /// message with a poisoned-lock one.
    fn acquire(&self) -> Permit<'_> {
        let maximum = budget();
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while *in_flight >= maximum {
            in_flight = self
                .released
                .wait(in_flight)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        *in_flight += 1;
        Permit { gate: self }
    }
}

struct Permit<'a> {
    gate: &'a Gate,
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut in_flight = self
            .gate
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *in_flight -= 1;
        self.gate.released.notify_one();
    }
}

/// Apply `work` to every item, in parallel, returning results in **input**
/// order.
///
/// The order results come back in is the order they went in, whatever order
/// they were computed in -- so a caller that builds a report out of them
/// still gets a stable, reviewable one.
pub fn map<T, R, F>(items: &[T], work: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    let schedule: Vec<usize> = (0..items.len()).collect();
    run(items, &schedule, work)
}

/// [`map`], visiting the most expensive items first.
///
/// `cost` is an estimate and only ever affects scheduling: a wrong one costs
/// makespan, never correctness.
pub fn map_by_cost<T, R, F, C>(items: &[T], cost: C, work: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
    C: Fn(&T) -> u64,
{
    let mut schedule: Vec<usize> = (0..items.len()).collect();
    schedule.sort_by_key(|index| std::cmp::Reverse(cost(&items[*index])));
    run(items, &schedule, work)
}

/// [`map_by_cost`] with the cost **measured on the previous run** rather than
/// estimated, through a [`CostLedger`] named `ledger`.
///
/// `key` names the cell in that ledger, and must be stable across runs -- a
/// scenario name, not an index, or renaming one row re-teaches the whole
/// table.
pub fn map_recording<T, R, F, K>(ledger: &str, items: &[T], key: K, work: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
    K: Fn(&T) -> String + Sync,
{
    let ledger = CostLedger::open(ledger);
    let keys: Vec<String> = items.iter().map(&key).collect();
    let mut schedule: Vec<usize> = (0..items.len()).collect();
    schedule.sort_by_key(|index| std::cmp::Reverse(ledger.cost(&keys[*index])));

    let observed: Mutex<Vec<(String, u64)>> = Mutex::new(Vec::new());
    let results = run(items, &schedule, |item| {
        let started = Instant::now();
        let value = work(item);
        let elapsed = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        observed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((key(item), elapsed));
        value
    });
    ledger.save(observed.into_inner().unwrap_or_else(|e| e.into_inner()));
    results
}

/// Run `work`, turning a panic into its message.
///
/// A `#[test]` body moved onto a worker thread loses something real: libtest's
/// output capture is thread-local and a scoped thread does not inherit it, so
/// an `assert!` that fires inside one prints its message to the terminal
/// mid-run and the failure the harness reports is `a scoped thread panicked`.
/// Wrapping the body here keeps the assertion's own message, and lets the
/// caller collect every failing cell into one report instead of stopping at
/// whichever thread happened to panic first.
///
/// The default hook still runs, so the location is still printed; what this
/// adds is the message arriving where the reader is looking.
pub fn catching<R>(work: impl FnOnce() -> R) -> Result<R, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).map_err(|payload| {
        payload
            .downcast_ref::<&str>()
            .map(|message| (*message).to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panicked with a non-string payload".to_string())
    })
}

/// The scheduler proper: `schedule` is the order to *start* items in, and the
/// returned vector is indexed the way `items` is.
fn run<T, R, F>(items: &[T], schedule: &[usize], work: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    // One worker is not worth a thread, and -- more importantly -- running
    // the body on this thread keeps a panic's backtrace pointing at the test
    // rather than at a scoped thread.
    let width = schedule.len().min(budget());
    if width <= 1 {
        return schedule
            .iter()
            .map(|index| (*index, work(&items[*index])))
            .collect::<BTreeMap<_, _>>()
            .into_values()
            .collect();
    }

    let slots: Vec<Mutex<Option<R>>> = items.iter().map(|_| Mutex::new(None)).collect();
    // Borrowed explicitly, because the worker closures are `move`: without
    // this the first one would take the cursor itself and the rest would have
    // nothing to pull from.
    let cursor = &AtomicUsize::new(0);
    let work = &work;
    let slots = &slots;
    std::thread::scope(|scope| {
        for _ in 0..width {
            scope.spawn(move || {
                loop {
                    let at = cursor.fetch_add(1, Ordering::Relaxed);
                    let Some(&index) = schedule.get(at) else {
                        return;
                    };
                    let value = {
                        let _permit = GATE.acquire();
                        work(&items[index])
                    };
                    *slots[index]
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(value);
                }
            });
        }
    });

    slots
        .iter()
        .map(|slot| {
            slot.lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take()
                .expect("a scheduled unit of work produced no result")
        })
        .collect()
}

/// **What each cell of a table cost last time**, so this run can start the
/// expensive ones first.
///
/// LPT scheduling needs a cost per item and nothing in a scenario row derives
/// one: step count is a poor proxy -- one `add db` step outweighs four
/// `g record`s -- and hand-written weights are a second table that goes stale.
/// So nothing is declared. Each run writes down what it observed under
/// `target/`, and the next run schedules by it.
///
/// Three properties keep it honest:
///
/// - **It is a hint and nothing else.** A missing, stale, corrupt or
///   truncated ledger changes the order work starts in and no result. That is
///   why it lives under `target/` rather than in the repository, why every
///   read failure is silently an empty ledger, and why every write failure is
///   ignored.
/// - **An unmeasured cell is scheduled first**, not last. An unknown cost is
///   most likely a newly added row, and the expensive assumption is the safe
///   one: starting a cheap item early costs nothing, while starting an
///   expensive one late costs its whole duration on the critical path.
/// - **One file per table**, keyed by the ledger's name, so two tables in two
///   binaries cannot overwrite each other's measurements.
pub struct CostLedger {
    path: PathBuf,
    known: BTreeMap<String, u64>,
}

/// The cost an unmeasured cell is scheduled at: ahead of everything measured.
const UNMEASURED: u64 = u64::MAX;

impl CostLedger {
    pub fn open(name: &str) -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/jails-test-costs")
            .join(format!("{name}.tsv"));
        let known = fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .filter_map(|(key, millis)| Some((key.to_string(), millis.trim().parse().ok()?)))
            .collect();
        Self { path, known }
    }

    pub fn cost(&self, key: &str) -> u64 {
        self.known.get(key).copied().unwrap_or(UNMEASURED)
    }

    /// Fold this run's measurements in and write the file back.
    ///
    /// Measurements are merged rather than replacing the file wholesale: a
    /// binary that ran under a filter measured only the cells it selected,
    /// and dropping the rest would make the next unfiltered run schedule
    /// blind.
    fn save(mut self, observed: Vec<(String, u64)>) {
        if observed.is_empty() {
            return;
        }
        for (key, millis) in observed {
            self.known.insert(key, millis);
        }
        let Some(parent) = self.path.parent() else {
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            return;
        }
        let body: String = self
            .known
            .iter()
            .map(|(key, millis)| format!("{key}\t{millis}\n"))
            .collect();
        // Written through a uniquely named neighbour and renamed: two test
        // binaries scheduled by the same ledger can be running at once, and a
        // half-written file read by the next run would be a silently worse
        // schedule rather than a visible failure.
        let staging = self
            .path
            .with_extension(format!("{}.tmp", std::process::id()));
        if fs::write(&staging, body).is_ok() {
            let _ = fs::rename(&staging, &self.path);
        }
        let _ = fs::remove_file(&staging);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn results_come_back_in_input_order_whatever_order_they_ran_in() {
        let items: Vec<usize> = (0..64).collect();
        // Descending cost, so the schedule is the exact reverse of the input.
        let doubled = map_by_cost(&items, |item| *item as u64, |item| item * 2);
        assert_eq!(
            doubled,
            items.iter().map(|item| item * 2).collect::<Vec<_>>()
        );
    }

    #[test]
    fn every_item_is_visited_exactly_once() {
        let items: Vec<usize> = (0..256).collect();
        let visits = AtomicUsize::new(0);
        let seen = map(&items, |item| {
            visits.fetch_add(1, Ordering::Relaxed);
            *item
        });
        assert_eq!(visits.load(Ordering::Relaxed), 256);
        assert_eq!(seen, items);
    }

    #[test]
    fn the_gate_holds_concurrency_at_the_budget() {
        let items: Vec<usize> = (0..128).collect();
        let live = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        map(&items, |_| {
            let now = live.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            std::thread::yield_now();
            live.fetch_sub(1, Ordering::SeqCst);
        });
        assert!(
            peak.load(Ordering::SeqCst) <= budget(),
            "{} units ran at once against a budget of {}",
            peak.load(Ordering::SeqCst),
            budget()
        );
    }

    #[test]
    fn an_unmeasured_cell_is_scheduled_ahead_of_every_measured_one() {
        let ledger = CostLedger {
            path: PathBuf::new(),
            known: [("cheap".to_string(), 1), ("dear".to_string(), 10_000)]
                .into_iter()
                .collect(),
        };
        assert!(ledger.cost("unseen") > ledger.cost("dear"));
        assert!(ledger.cost("dear") > ledger.cost("cheap"));
    }

    #[test]
    fn an_absent_or_corrupt_ledger_is_an_empty_one_rather_than_a_failure() {
        let ledger = CostLedger {
            path: PathBuf::new(),
            known: BTreeMap::new(),
        };
        assert_eq!(ledger.cost("anything"), UNMEASURED);
    }
}
