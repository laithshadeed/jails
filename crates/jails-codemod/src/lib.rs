//! Surgical edits to text somebody else owns.
//!
//! **This crate has no dependencies at all**, so every crate in the workspace
//! can reach it, and each of these edits has exactly one implementation: a
//! second copy of a surgical edit is a copy that drifts.
//!
//! [`marked`] is the marked block -- `# jails:<marker>` … `# /jails:<marker>`,
//! how jails edits a file the reader owns and what makes `remove` the exact
//! inverse of `add`. [`dispatch`] splices one line above `return commands;`,
//! which is how `g command` registers itself in a project's CLI dispatcher
//! rather than leaving a paste instruction in a Javadoc. [`annotate`] is the
//! `@Import` splice and [`tidy`] the import normaliser; the crates that edit
//! Java reach them directly.
//!
//! [`text`] is the scanner all of them lean on: the source with its comments
//! and literals blanked to spaces of the same length, so a scan cannot be
//! fooled by `// @Service` while byte offsets still index the original.
//!
//! Nothing here reads or writes a file, and nothing here knows what a
//! capability is.

pub mod annotate;
pub mod dispatch;
pub mod marked;
pub mod text;
pub mod tidy;

pub use marked::Marked;
