//! The canonical application-model frontend.

use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub(crate) enum ModelCommand {
    /// Adopt supported legacy declarations and their live Java into the canonical compiler
    Import,
    /// Parse, link, and type-check the application model without writing
    Check {
        /// Canonical model file to check
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Also require the committed managed tree to equal this compilation
        #[arg(long)]
        frozen: bool,
    },
    /// Rewrite the pre-v1 JDL draft as JDL v1, preserving every stable id
    Upgrade {
        /// JDL language version to upgrade to
        #[arg(long, value_name = "VERSION")]
        to: u16,
    },
    /// Canonically format the JDL authoring source
    Fmt {
        /// Refuse without writing when the source is not canonically formatted
        #[arg(long)]
        check: bool,
    },
    /// Compile the model into one content-addressed exact plan without applying it
    Plan {
        /// Canonical model file to compile
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Write the complete portable plan bundle to this path
        #[arg(long, value_name = "FILE")]
        bundle: Option<PathBuf>,
    },
    /// Apply one previously reviewed exact plan without recompiling
    Apply {
        /// Portable exact plan bundle written by `jails model plan`
        #[arg(long, value_name = "FILE")]
        bundle: PathBuf,
    },
    /// Show every name the convention derived rather than the author writing it
    Explain {
        /// Stable id, role, package or value to filter by; omit to list every record
        filter: Option<String>,
    },
    /// Transfer generated Java for one semantic node into reader-owned source
    Eject {
        /// Stable entity, operation, or capability id recorded in the model
        semantic_id: String,
    },
}
