//! Commands that answer a question about a project. **Read-only by contract.**
//!
//! [`doctor`] says what is wrong and never fixes it, so it stays safe to run
//! mid-debug. [`why`] turns a log into a cause. [`explain`] says why an
//! artifact is shaped the way it is. [`source`] says where a type lives.
//! [`commands`] walks the same `clap::Command` that parses the arguments, so
//! there is no second list of what jails can do.
//!
//! **The contract is structural now, not a promise.** These five lived one
//! `use` away from `run::mvn` in a crate that also drove Maven, started
//! containers and ran a JVM; `pending.md` §7.6 is the entry about it. This
//! crate cannot depend on `jails-drive` -- `jails-drive` depends on *it*,
//! because `jails run` scans its own output through [`why`] -- so a reporting
//! command that started something would not compile.
//!
//! Severing it took one deletion: `run::find_on_path` was a one-line alias for
//! `process::on_path`, and it was `doctor`'s only reason to name `run` at all.

pub mod commands;
mod diagnostic;
pub mod doctor;
pub mod explain;
pub mod lifecycle_status;
mod managed_drift;
pub mod schema_lineage;
pub mod source;
pub mod why;
pub mod why_subject;

// The lower crates, re-exported so every module in this one keeps saying
// `crate::…` wherever it ships.
//
// **`jails_generate` is the only legacy crate named here, and what is left of
// it is one function.** This block used to read `jails_generate::{add,
// generate}`, which made the coupling look four times its size: `generate`
// re-exported `ArtifactKind`, `find_project_root` and the field spec straight
// out of `jails-spec`, and `add` re-exported `Capability` from the same place.
// Every one of those is a symbol a *surviving* crate owns, so pointing at the
// owner cost nothing and freed `jails-drive` from `jails-generate` outright.
//
// What survives the re-pointing is `add::plan_for`, and the map the cutover
// needs is that **every remaining legacy reference in this crate is dead on a
// canonical project**, so none of them is a prerequisite for the deletion --
// they go in the same commit. `capability_drift_checks` returns one `Skip`
// when `Project::is_modelled`, because it is a check about `jails.toml` and a
// modelled project records its capabilities in the model. `managed_drift`,
// `lifecycle_status` and `schema_lineage` each open with
// `MachineState::Current(store)` and return nothing otherwise, and a canonical
// project has no `.jails/ledger.toml` to read. There is no canonical report to
// write first: the canonical answers are `jails model status`, `model doctor`
// and `model explain`, and they already exist.
pub(crate) use jails_java::{java, template};
pub(crate) use jails_project::{compose, inspect, maven, model, pom};
pub(crate) use jails_spec::build;
pub(crate) use jails_spec::spec::kind::{ArtifactKind, Capability};
pub(crate) use jails_spec::spec::paths::find_project_root;
pub(crate) use jails_support::{apply, json, process};
