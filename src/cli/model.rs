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
    /// Transfer generated Java for one semantic node into reader-owned source
    Eject {
        /// Stable entity, operation, or capability id recorded in the model
        semantic_id: String,
    },
}
