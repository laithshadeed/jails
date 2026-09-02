//! The reader: what a project is, captured once.
//!
//! [`capture`] is the one place jails looks at a project. It fills a
//! `WorkspaceSnapshot` for the compiler, and its `observe` half produces the
//! `ProjectFacts` every command reads -- so [`project::Project`], the
//! parameter object the commands take, is a root plus those facts and
//! nothing a renderer could re-derive from a `&Path`.
//!
//! The files jails writes *about* a project live here too, and they divide by
//! who owns them. [`config`] and [`compose`] are the reader's, edited by
//! byte-preserving splice; [`documents`] holds the adapters that reconcile a
//! plan into the reader's build file, properties and compose file, and
//! [`pom`], the one reader of what a POM says; [`merge`] is the three-way
//! merge the adapters and the workspace above share.
//!
//! [`maven`] is how to invoke this project's Maven, deliberately separate from
//! `jails_spec::build`, which recognises a build file and never runs one.
//!
//! [`inspect`] reads the project's source to report routes and beans. It is
//! here rather than with the commands because `add`'s HTTP capability derives
//! its client from the same route list, and a command layer above the
//! generators could not be reached from there.
//!
//! Reading Java, and rendering templates into it, is here too. [`java`] is
//! deliberately small -- annotations and what they are attached to, a type's
//! supertypes, a constructor's parameters -- and must not grow into a parser.
//! [`classfile`] is the same rule applied to bytecode: the smallest reader
//! that can answer "which types does this class name", constant pool only.
//! [`template`] is substitution, not a template engine: anything structural
//! stays in the generator layer and arrives already rendered. They were a
//! crate of their own until nothing below this one needed them.

/// The marked-block splice, from its own dependency-free crate so that every
/// tree can reach one implementation: a format with several owners is several
/// answers to what `remove db` deletes. Module code says `crate::codemod`.
pub use jails_codemod as codemod;
pub mod capability;
pub mod capture;
pub mod classfile;
pub mod compose;
pub mod config;
pub mod documents;
pub mod feature;
pub mod gradle;
pub mod inspect;
pub mod java;
pub mod maven;
pub mod merge;
pub mod modernize;
pub mod project;
pub mod properties;
pub mod synonyms;
pub mod template;

// The lower crates, re-exported so every module in this one keeps saying
// `crate::…` wherever it ships. Only this block knows which crate a module
// actually lives in, which is what makes moving one a one-line change.
/// The one reader of `pom.xml`, beside the document adapters that write it.
/// Module code says `crate::pom`.
pub use documents::pom;

/// The eleven layers, from the crate that owns every closed vocabulary.
/// Module code says `crate::layout`.
pub use jails_model::layout;
pub use jails_spec::{build, release, spec};
pub(crate) use jails_support::{json, process};
