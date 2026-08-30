//! Surgical edits to text somebody else owns.
//!
//! Three of them, and one reason they share a crate: **this crate has no
//! dependencies at all**, so there is nowhere in the workspace that cannot
//! reach it. Every one of these edits has two engines performing it, and a
//! second copy of a surgical edit is a copy that drifts.
//!
//! [`marked`] is the marked block -- `# jails:<marker>` … `# /jails:<marker>`,
//! how jails edits a file the reader owns and what makes `remove` the exact
//! inverse of `add`. It lived in `jails-project` until 2026-08-29, when the
//! architecture gate that was supposed to hold it at one owner turned out to
//! be counting text that blanking had already erased -- and, unwatched, three
//! more implementations had appeared in `jails-compiler` and
//! `jails-workspace`. They were not careless: neither crate depends on
//! `jails-project`, so there was nothing to reuse.
//!
//! [`dispatch`] is the other splice: one line above `return commands;`, which
//! is how `g command` registers itself in a project's CLI dispatcher rather
//! than leaving a paste instruction in a Javadoc.
//!
//! [`annotate`], [`dispatch`] and [`tidy`] arrived the same way and for the
//! same reason.
//! They lived in `jails-java`, which no canonical crate may depend on, and
//! `jails-workspace` needed the `@Import` splice to give a canonical
//! `storage postgres` project the test wiring its build cannot start without.
//! Rather than write a fourth implementation, they moved here. `jails-java`
//! re-exports both, so every existing caller is unchanged.
//!
//! [`text`] is the scanner all three lean on: the source with its comments
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
