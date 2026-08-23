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

pub mod classfile;
pub mod java;
pub mod template;
