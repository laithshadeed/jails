//! Commands that drive a toolchain. **Every one of these starts something.**
//!
//! Maven, Gradle, Docker Compose, psql, a JVM, k6. Some of them write
//! `target/`; some start containers. That is the line against
//! [`jails_report`], which is read-only by contract and sits *below* this
//! crate so the contract is structural rather than a promise
//! (`pending.md` §7.6).
//!
//! [`run`], `launcher` and [`testd`] are three ways to run the same tests,
//! fastest last, and `affected` is how `testd` decides which ones.
//!
//! The one edge back down is `run` -> `jails_report::why`: `mvn
//! spring-boot:run` exits 0 over a failed startup, so `run` pipes the output,
//! scans it for fatal markers and explains the failure inline.

pub(crate) mod affected;
pub mod baseline;
pub mod bench;
pub mod console;
mod datasource;
pub mod doctor;
pub mod kafka;
pub(crate) mod launcher;
pub mod lint;
pub mod live_sql;
pub mod migrate;
pub(crate) mod reports;
pub mod run;
pub mod testd;
pub mod testing;

// The lower crates, re-exported so every module in this one keeps saying
// `crate::…` wherever it ships.
//
// **No legacy generator crate appears here, and removing the last one changed
// no behaviour at all.** This block read `jails_generate::generate`, and every
// symbol nine modules took through it -- `find_project_root` and
// `is_dispatcher` -- was re-exported from a crate that survives the cutover.
// Naming the owner is the whole change. What is left of the legacy ladder here
// is `jails_commit::store` in `migrate`, which reads a frozen migration back
// out of the object store by digest.
pub(crate) use jails_codemod::dispatch::is_dispatcher;
pub(crate) use jails_java::{classfile, java};
pub(crate) use jails_project::{compose, maven, model, pom};
pub(crate) use jails_report::why;
pub(crate) use jails_spec::build;
pub(crate) use jails_spec::spec::paths::find_project_root;
pub(crate) use jails_support::{json, process};
