//! Reading Java, and rendering templates into it.
//!
//! Two readers and a substituter, none of which knows what jails generates.
//!
//! [`java`] is deliberately small — annotations and what they are attached to,
//! a type's supertypes, a constructor's parameters — and must not grow into a
//! parser. [`classfile`] is the same rule applied to bytecode: the smallest
//! reader that can answer "which types does this class name", constant pool
//! only. [`template`] is substitution, not a template engine: anything
//! structural stays in the generator layer and arrives already rendered.
//! [`annotate`] is the one *writer*: a surgical edit to one annotation on a
//! class the reader owns.

// Re-exported, not owned: these live in `jails-codemod`, which has no
// dependencies at all, so the canonical crates reach the `@Import` splice
// without depending on this one.
pub use jails_codemod::{annotate, dispatch, tidy};

pub mod classfile;
/// Identifier surgery lives in `jails-support`: `snake_case` and the bounded
/// replacements are string operations that know nothing about Java, and
/// `jails_support::identity` needs `snake_case`, so they must sit below it.
pub use jails_support::identifier;
pub mod java;

pub mod template;
