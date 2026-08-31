//! One argument, read.
//!
//! The values the CLI takes that are *not* a closed vocabulary: a Maven
//! coordinate, a `key=value` setting, and which properties file a setting was
//! written to. Everything else `main.rs` accepts is a `ValueEnum`, which clap
//! validates and `clap_complete` can list; these cannot be, so each one needs
//! a refusal that names the fix -- and a refusal is prose, which is why they
//! are here rather than inline in a match arm.

use jails_support::Result;

/// Maven's three scopes, as a closed set so `--scope <TAB>` completes.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum DependencyScope {
    Compile,
    Runtime,
    Test,
}

impl DependencyScope {
    pub(crate) fn canonical(self) -> jails_model::DependencyScope {
        match self {
            Self::Compile => jails_model::DependencyScope::Compile,
            Self::Runtime => jails_model::DependencyScope::Runtime,
            Self::Test => jails_model::DependencyScope::Test,
        }
    }
}

/// Returns [`std::process::ExitCode`] rather than calling [`std::process::exit`].
///
/// `crate::process::exit` terminates without unwinding, so destructors on the
/// current stack never run. jails holds real resources while a command is in
/// flight -- `migrate` creates a scratch database it is responsible for
/// dropping -- and anything staging a file beside its destination would be in
/// the same position. Returning lets the stack unwind normally first.
/// `group:artifact`, refused rather than guessed at.
///
/// A coordinate with a third part is almost always a `group:artifact:version`
/// pasted from somewhere -- so the refusal names `--version` rather than
/// repeating the shape back.
pub(crate) fn maven_coordinate(text: &str) -> Result<jails_protocol::coordinate::MavenCoordinate> {
    let mut parts = text.split(':');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(group), Some(artifact), None) if !group.is_empty() && !artifact.is_empty() => {
            jails_protocol::coordinate::MavenCoordinate::parse(group, artifact)
        }
        (_, _, Some(_)) => Err(format!(
            "`{text}` names a version as well as a coordinate.\n       \
             fix: jails add dependency <group>:<artifact> --version <version>"
        )
        .into()),
        _ => Err(format!(
            "`{text}` is not a Maven coordinate.\n       \
             fix: jails add dependency <group>:<artifact>"
        )
        .into()),
    }
}

/// `key=value`, split once so a value containing `=` survives.
pub(crate) fn split_setting(text: &str) -> Result<(String, String)> {
    match text.split_once('=') {
        Some((key, value)) => Ok((key.trim().to_string(), value.to_string())),
        None => Err(format!(
            "`{text}` is not a `key=value` setting.\n       \
             fix: jails set server.port=3000"
        )
        .into()),
    }
}
