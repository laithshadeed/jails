//! Stable root aliases for the lower workspace crates.

pub(crate) use jails_drive::{bench, console, doctor, kafka, lint, migrate, run, testd};
pub(crate) use jails_model::{ArtifactKind, CapabilityKind};
pub(crate) use jails_project::template;
pub(crate) use jails_project::{compose, inspect, pom, project};
pub(crate) use jails_report::{commands, explain, source, why};
pub(crate) use jails_spec::release;
pub(crate) use jails_spec::spec::suffix::{recorded_name, strip_redundant_suffix};
