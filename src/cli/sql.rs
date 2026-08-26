//! Closed command-line vocabulary for named SQL contracts.

use super::RunServicesArg;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum SqlCommand {
    /// Verify managed queries against the catalog derived from ordered migrations
    Check {
        /// Project-relative query file or manifest query name
        target: Option<String>,
        #[arg(long, conflicts_with = "live")]
        offline: bool,
        #[arg(long, conflicts_with = "offline")]
        live: bool,
        #[arg(long, requires = "live", value_name = "NAME")]
        datasource: Option<String>,
        #[arg(long, value_enum, default_value = "existing", requires = "live")]
        services: RunServicesArg,
        #[arg(long)]
        frozen: bool,
        #[arg(long)]
        no_cache: bool,
        #[arg(long, value_name = "MANIFEST")]
        manifest: Option<std::path::PathBuf>,
    },
    /// Generate Java and checked-in contracts from verified offline evidence
    Generate {
        target: Option<String>,
        #[arg(long, value_name = "SLICE")]
        into_slice: Option<String>,
        #[arg(long, value_name = "MANIFEST")]
        manifest: Option<std::path::PathBuf>,
    },
}
