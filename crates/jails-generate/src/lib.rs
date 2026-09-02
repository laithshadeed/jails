//! What survives of the pre-compiler generator: the write path, and the
//! SQL projection of a field spec.
//!
//! **This crate was 23,809 lines and is 1,300.** It held four halves that
//! called each other freely -- `generate` dispatching an `ArtifactKind` to a
//! recipe, `spring` holding the kinds and capabilities that need a Boot
//! parent, `add` growing a project by a whole slice, and [`sql`] projecting
//! the field spec -- and three of them are gone, because `jails-compiler`
//! emits all thirty-nine advertised kinds and all twenty-five capabilities,
//! and every mutating command seeds a model before it runs. Nothing dispatched
//! to a recipe any more; nothing planned a capability any more.
//!
//! Two things kept the rest alive and they are what is left. [`generate`] owns
//! the rules keyed off emitted bytes -- import normalisation,
//! `package-info.java`, `ensure_failsafe`, `ensure_assertj` -- which the binary
//! still calls on the way to disk. [`sql`] answers what one record component
//! is on both sides of the JDBC boundary, which `jails-report`'s schema
//! lineage still asks.
//!
//! The boundary that matters is the one *below*: nothing here is reachable
//! from `jails-project` or lower. That is the edge which keeps the ladder
//! acyclic, and it is the one to defend.

pub mod generate;
pub mod sql;

// The lower crates, re-exported so every module in this one keeps saying
// `crate::…` wherever it ships. Only this block knows which crate a module
// actually lives in, which is what makes moving one a one-line change.
pub use jails_java::{java, template};
pub(crate) use jails_project::{model, pom};
pub use jails_spec::{build, spec};

/// This crate's templates live at the repository root, two levels up from its
/// own manifest.
///
/// The root cannot be implicit: `CARGO_MANIFEST_DIR` inside
/// [`jails_java::template_at`] expands at the *call site*, so a macro that
/// baked it in would look under `crates/jails-generate/templates/`. Naming it
/// here makes a wrong root a compile error instead of a silent miss.
macro_rules! template_here {
    ($name:literal) => {
        jails_java::template_at!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../templates/"),
            $name
        )
    };
}
pub(crate) use template_here;
