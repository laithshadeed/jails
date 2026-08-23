//! What a jails project is made of, below anything that generates.
//!
//! Three questions, none of which is "what Java should I write":
//!
//! - **Where is the project, and where inside it does a class go** ([`paths`]).
//! - **What is the conventional package for each layer** ([`layout`]).
//! - **What does a field spec mean** ([`field`]).
//!
//! All three used to live in `generate.rs`, and every layer below the
//! generators reached up into it for them: `model`, `config`, `compose`,
//! `project` and `inspect` all did. That made twelve modules one cycle, and a
//! cycle is a boundary nothing can enforce -- the drift `CLAUDE.md` records in
//! `inspect.rs`'s private copy of the layer list is exactly what an
//! unenforceable boundary produces.
//!
//! The generator layer re-exports all of it, so `crate::generate::Field` still
//! resolves; what changed is the direction of the arrow.

pub mod field;
pub mod kind;
pub mod layout;
pub mod paths;

pub use field::*;
pub use paths::*;
