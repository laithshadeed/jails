//! The marked block, and nothing else.
//!
//! `# jails:<marker>` … `# /jails:<marker>` is how jails edits a file the
//! reader owns, and it is what makes `remove` the exact inverse of `add`. It
//! lived in `jails-project` until 2026-08-29, when the architecture gate that
//! was supposed to hold it at one owner turned out to be counting text that
//! blanking had already erased -- and, unwatched, three more implementations
//! had appeared in `jails-compiler` and `jails-workspace`.
//!
//! They were not careless. Neither crate depends on `jails-project`, so there
//! was nothing to reuse. This crate has **no dependencies at all**, so there is
//! nowhere left that cannot reach it, and the format has one owner again.

pub mod marked;

pub use marked::Marked;
