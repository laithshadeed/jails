//! The arguments of creating a project.
//!
//! Its own module for the same reason `schema`, `sql` and `rename` are: this
//! is one command's vocabulary rather than the command list.

use super::*;

/// Everything `jails new` takes.
///
/// A parameter object rather than fifteen fields on the enum variant: they
/// are computed together and consumed together, as one `new::Request`, and
/// destructuring them in dispatch only to rebuild them one for one is dispatch
/// doing construction.
#[derive(clap::Args)]
pub(crate) struct NewArgs {
    pub(crate) name: String,
    /// Maven groupId, e.g. `com.intercom`
    ///
    /// Without `--package`, the base package becomes `<group>.<name>`.
    #[arg(long)]
    pub(crate) group: Option<String>,
    /// Base package, e.g. `com.intercom.spring`
    ///
    /// Outranks `--group`: it states the whole answer. An existing service
    /// already has a package, and it is never `com.example`.
    #[arg(long)]
    pub(crate) package: Option<String>,
    #[arg(long, default_value = "web")]
    pub(crate) deps: String,
    #[arg(long, default_value = release::TARGET_RELEASE)]
    pub(crate) java: String,
    /// Skip `git init` and the .gitignore it normally sets up
    #[arg(long)]
    pub(crate) no_git: bool,
    /// Skip adding spring-boot-devtools (needed for `run --watch`)
    #[arg(long)]
    pub(crate) no_devtools: bool,
    /// Create from jails' vendored Spring fixture without contacting
    /// start.spring.io
    #[arg(long)]
    pub(crate) offline: bool,
    /// Write a Groovy Gradle build instead of fetching a Maven project
    ///
    /// jails writes every file itself on this path and never contacts
    /// start.spring.io, which is what makes `--boot` able to name a
    /// version Initializr no longer serves -- and what makes `--pretend`
    /// honest here when it cannot be for Maven.
    #[arg(long)]
    pub(crate) gradle: bool,
    /// Spring Boot version to pin, e.g. `2.7.18`. Gradle only
    ///
    /// A 2.x version selects the `buildscript {}` build file, which is the
    /// only shape that applies the Boot 2 Gradle plugin. Anything later
    /// gets `plugins {}` and Gradle's native bom support.
    #[arg(long, value_name = "VERSION")]
    pub(crate) boot: Option<String>,
    /// Gradle distribution the wrapper pins. Gradle only
    ///
    /// Defaults from the Boot version rather than to one number: Boot 4's
    /// plugin throws below Gradle 8.14, and Boot 2.7 does not run on 9.x.
    #[arg(long, value_name = "VERSION")]
    pub(crate) gradle_version: Option<String>,
    /// `bootJar` archive base name. Gradle only
    ///
    /// Omitted, there is no `bootJar` block and Gradle names the jar after
    /// the project.
    #[arg(long, value_name = "NAME")]
    pub(crate) jar_name: Option<String>,
    /// `bootJar` archive version. Gradle only, and only with `--jar-name`
    #[arg(long, value_name = "VERSION")]
    pub(crate) jar_version: Option<String>,
    /// Start the new project from this model or manifest
    ///
    /// A `.jdl` is copied in as the project's own `.jails/model.jdl` and
    /// compiled, under the identity `new` just wrote: the file's `pkg`,
    /// `java`, `platform` and `build` give way to the ones this command
    /// chose, and every declaration in it is kept. A `.toml` is an
    /// application manifest whose rows are replayed as commands. The path is
    /// read relative to where you are standing, and the extension decides
    /// which it is.
    #[arg(long, visible_alias = "app", value_name = "FILE")]
    pub(crate) model: Option<std::path::PathBuf>,
    /// Write Compose services but do not start them
    ///
    /// `--app` is `jails app apply` inside the publication, so it takes
    /// that command's flag too. Without it a manifest declaring `db` or
    /// `kafka` makes project creation depend on a container engine being
    /// up, and a failed start is now reported truthfully -- which turns
    /// "generate this project" into a command that fails on a machine
    /// whose port is already taken.
    #[arg(long)]
    pub(crate) no_start: bool,
}

/// Everything `jails new-cli` takes.
#[derive(clap::Args)]
pub(crate) struct NewCliArgs {
    pub(crate) name: String,
    /// Maven groupId, e.g. `com.intercom`
    ///
    /// Without `--package`, the base package becomes `<group>.<name>`.
    #[arg(long)]
    pub(crate) group: Option<String>,
    /// Base package, e.g. `com.intercom.spring`
    ///
    /// Outranks `--group`: it states the whole answer.
    #[arg(long)]
    pub(crate) package: Option<String>,
    /// Java release to compile against (>= 21)
    #[arg(long, default_value = release::TARGET_RELEASE)]
    pub(crate) release: String,
    /// Skip `git init` and the .gitignore it normally sets up
    #[arg(long)]
    pub(crate) no_git: bool,
    /// Start the new project from this model or manifest
    ///
    /// A `.jdl` is copied in as the project's own `.jails/model.jdl` and
    /// compiled, under the identity `new` just wrote: the file's `pkg`,
    /// `java`, `platform` and `build` give way to the ones this command
    /// chose, and every declaration in it is kept. A `.toml` is an
    /// application manifest whose rows are replayed as commands. The path is
    /// read relative to where you are standing, and the extension decides
    /// which it is.
    #[arg(long, visible_alias = "app", value_name = "FILE")]
    pub(crate) model: Option<std::path::PathBuf>,
    /// Write Compose services but do not start them
    ///
    /// `--app` is `jails app apply` inside the publication, so it takes
    /// that command's flag too. Without it a manifest declaring `db` or
    /// `kafka` makes project creation depend on a container engine being
    /// up, and a failed start is now reported truthfully -- which turns
    /// "generate this project" into a command that fails on a machine
    /// whose port is already taken.
    #[arg(long)]
    pub(crate) no_start: bool,
}
