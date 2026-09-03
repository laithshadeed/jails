//! The canonical application-model frontend.

use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub(crate) enum ModelCommand {
    /// Write an application model for a project jails did not create
    Init,
    /// Parse, link, and type-check the application model without writing
    Check {
        /// Model file to check
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Also require the committed managed tree to equal this compilation
        #[arg(long)]
        frozen: bool,
    },
    /// Lay out the JDL authoring source the way jails writes it
    Fmt {
        /// Refuse without writing when the source is not canonically formatted
        #[arg(long)]
        check: bool,
    },
    /// Compile the model into one content-addressed plan without applying it
    Plan {
        /// Model file to compile
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Write the complete portable plan bundle to this path
        ///
        /// **Hidden, because `--plan-out` is the spelling.** The global flag
        /// writes the reviewed bundle on every mutation, and this asked the
        /// same question of one command under a second name. It parses for
        /// one release and `plan` reads whichever of the two was given.
        #[arg(long, value_name = "FILE", hide = true)]
        bundle: Option<PathBuf>,
    },
    /// Apply one previously reviewed plan without compiling again
    Apply {
        /// Portable plan file written by `jails model plan`
        ///
        /// **Hidden, because `--plan-in` is the spelling.** It is the same
        /// question the global flag asks -- apply this reviewed bundle and do
        /// not replan -- and it parses for one release beside it.
        #[arg(long, value_name = "FILE", hide = true)]
        bundle: Option<PathBuf>,
    },
    /// List the files jails generated, and whether each still matches the model
    Status,
    /// Move managed output a release before this one wrote under .jails/generated into src/
    Relocate,
    /// Print the JDL language: declarations, attributes, types, projections
    ///
    /// Walked out of the parser's own registries, so it says what this binary
    /// accepts rather than what a document once said it accepted.
    Jdl,
    /// Show every name the convention derived rather than the author writing it
    Explain {
        /// Stable id, role, package or value to filter by; omit to list every record
        filter: Option<String>,
    },
    /// Release one implementation boundary to you: its files stay put and leave the generated tree
    Eject {
        /// A boundary path (Note.repo.fake, Audit.implementation), an artifact id from provenance, or a capability id
        #[arg(value_name = "TARGET")]
        semantic_id: String,
    },
}
