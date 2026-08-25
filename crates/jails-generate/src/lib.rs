//! Everything that decides what Java to write.
//!
//! [`generate`] dispatches one `ArtifactKind` to the recipe that renders it,
//! [`spring`] holds the kinds and capabilities that need a Spring Boot parent,
//! [`add`] grows a project by a whole slice (dependency, code, test and where
//! needed a compose service), and [`sql`] is the SQL/JDBC projection of the
//! same field spec the domain side reads.
//!
//! These four call each other freely and ship together on purpose. `generate`
//! dispatches into `spring`, `spring` renders through `generate`'s shared
//! helpers, and separating them would buy a boundary that neither wants. The
//! boundary that matters is the one *below*: nothing here is reachable from
//! `jails-project` or lower, which is what the twelve-module cycle used to
//! make impossible.

pub mod add;
pub mod generate;
pub mod spring;
pub mod sql;

// The lower crates, re-exported so every module in this one keeps saying
// `crate::…` wherever it ships. Only this block knows which crate a module
// actually lives in, which is what makes moving one a one-line change.
pub use jails_java::{java, template};
pub(crate) use jails_project::{compose, generated_files, gradle, inspect, model, pom, project};
pub use jails_spec::{build, spec};
pub(crate) use jails_support::json;

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
