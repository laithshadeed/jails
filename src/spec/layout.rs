// Where each kind of artifact lives, relative to the project's base package.
//
// A generated project should look like one a person laid out, and nobody
// lays out thirty classes as siblings of `App.java`. The names are the ones
// the Java ecosystem already uses, so the layout reads as conventional rather
// than as jails' invention -- and every one of them is a package a human
// would have created by hand on about the third file.
//
// This is a default, not a policy: `--package` overrides it, and `--package
// ''` puts everything back in the base package for a project small enough not
// to want the ceremony.

pub const DOMAIN: &str = "domain";
/// Ports -- the interfaces the application depends on, kept free of the
/// technology that implements them.
pub const APP: &str = "app";
pub const SERVICE: &str = "service";
pub const WEB: &str = "web";
pub const CLI: &str = "cli";
pub const ADAPTERS: &str = "adapters";
pub const API: &str = "api";
pub const TESTKIT: &str = "testkit";
/// Outbound HTTP: interfaces this application calls, kept apart from
/// `api` (what it serves) so the direction of a dependency is visible
/// from the package name alone.
pub const CLIENTS: &str = "clients";
/// Scheduled work.
pub const JOBS: &str = "jobs";
/// Events published to and consumed from a broker.
pub const MESSAGING: &str = "messaging";
