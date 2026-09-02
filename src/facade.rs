//! Stable root aliases for the lower workspace crates.

pub(crate) use jails_drive::{bench, console, doctor, kafka, lint, migrate, run, testd};
pub(crate) use jails_generate::generate;
pub(crate) use jails_java::template;
pub(crate) use jails_project::{compose, inspect, model, pom, project};
pub(crate) use jails_protocol::recipe::{recorded_name, strip_redundant_suffix};
pub(crate) use jails_report::{commands, explain, source, why};
pub(crate) use jails_spec::spec::kind::{ArtifactKind, Capability};
