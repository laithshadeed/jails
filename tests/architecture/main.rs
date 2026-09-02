//! The architecture ratchets: numbers measured over production Rust, each
//! held under a recorded ceiling.
//!
//! Each row is a **ratchet** and bites in both directions. Rising above the
//! ceiling fails: that is the regression guard. Falling below it also fails,
//! telling you to record the new number, so an improvement that is not written
//! down cannot be silently undone by the next change.
//!
//! `target` is where a row is finished. A ceiling equal to its target is a
//! closed rung; the test prints the gap for every row, so
//! `cargo test --test architecture -- --nocapture` is the progress report.
//!
//! Measurement is over **blanked** Rust: comments and string literals are
//! replaced by spaces of the same length, so a `fn` inside an inline Java body
//! cannot be counted as a function and a `root: &Path` written in a doc comment
//! cannot be counted as a parameter.
//!
//! The board, the rules, the blanking parser and its unit tests share one
//! binary because each extra integration-test binary is a full link of the
//! workspace, and they all draw on [`measure::sources`].

mod board;
mod measure;
/// The suite-wide scheduler, shared through `#[path]` rather than copied: the
/// blanking scan runs over the same permit gate every other test binary draws
/// from, so a full-workspace run cannot oversubscribe the machine with two
/// schedulers that know nothing of each other.
#[path = "../common/parallel.rs"]
mod parallel;
mod rules;
