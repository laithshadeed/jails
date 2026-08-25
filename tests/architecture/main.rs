//! The eleven gates of `abstract.md` §8, as ratchets rather than as a table.
//!
//! `abstract.md` prices every rung of its refactor ladder with a falsifiable
//! gate and says to revert the rung when the gate is missed. Nothing measured
//! them, and §8.1 recorded the consequence: `root: &Path` rose 21% across four
//! commits while `src/model/mod.rs` was being added *beside* the primitive
//! rather than instead of it, and nothing said so. Prose did not move the
//! number; `tests/genericity.rs` moved the vocabulary problem only once it put
//! a failure in the build.
//!
//! So each row below is a **ratchet**, and it bites in both directions:
//!
//! - **Rising above the ceiling fails.** That is the regression guard.
//! - **Falling below the ceiling also fails**, telling you to record the new
//!   number. That is what makes progress permanent: an improvement that is not
//!   written down here is an improvement the next change may silently undo.
//!
//! `target` is the number `abstract.md` §8 actually asks for. A ceiling equal
//! to its target is a finished rung; the test prints the gap for every row, so
//! `cargo test --test architecture -- --nocapture` is the ladder's progress
//! report.
//!
//! Measurement is over **blanked** Rust: comments and string literals are
//! replaced by spaces of the same length, so a `fn` inside one of `spring.rs`'s
//! inline Java bodies cannot be counted as a function, and a `root: &Path`
//! written in a doc comment cannot be counted as a parameter. That is
//! `src/java.rs`'s own trick, applied to Rust for the same reason.

//! ## Four files, one binary
//!
//! `pending.md` §8.2. This file mixed the ratchet board, the architecture
//! rules, a small Rust blanking parser, the crate-layer table and that parser's
//! own unit tests -- five subjects, and the only thing they had in common was
//! that they all needed [`measure::sources`].
//!
//! **One binary, not four.** Each extra integration-test binary is a full link
//! of the workspace, and there are already ten.

mod board;
mod measure;
mod rules;
