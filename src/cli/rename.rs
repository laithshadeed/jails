//! Arguments for `jails rename`, and the two policies it forces a caller to
//! state.
//!
//! `RenameStrategy` and `StoragePolicy` are `ValueEnum`s rather than flags
//! because each names a decision with no safe default: preserve-table is a
//! projection change with no migration, single-cutover changes the SQL
//! explicitly, and rolling is a campaign. Retiring a stored entity is the same
//! shape — preserve keeps an inactive semantic node, drop appends a forward
//! migration — so the caller says which, and jails never guesses which of two
//! irreversible readings was meant.

use clap::Subcommand;

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum StoragePolicy {
    Preserve,
    Drop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub(crate) enum RenameStrategy {
    PreserveTable,
    SingleCutover,
    Rolling,
}

impl From<RenameStrategy> for jails_protocol::request::RenameStrategy {
    fn from(value: RenameStrategy) -> Self {
        match value {
            RenameStrategy::PreserveTable => Self::PreserveTable,
            RenameStrategy::SingleCutover => Self::SingleCutover,
            RenameStrategy::Rolling => Self::Rolling,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, clap::ValueEnum)]
pub(crate) enum ExternalRenamePolicy {
    #[default]
    Preserve,
    Rename,
}

impl From<ExternalRenamePolicy> for jails_protocol::request::ExternalRenamePolicy {
    fn from(value: ExternalRenamePolicy) -> Self {
        match value {
            ExternalRenamePolicy::Preserve => Self::Preserve,
            ExternalRenamePolicy::Rename => Self::Rename,
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum RenameCommand {
    /// Rename a managed resource with an explicit storage strategy
    Resource {
        /// `<slice>.<current-name>` selector resolved before planning
        from: String,
        /// New logical resource name
        to: String,
        /// Explicit physical-storage transition
        #[arg(long, value_enum)]
        strategy: RenameStrategy,
        /// Target physical table for cutover or rolling storage
        #[arg(long)]
        table: Option<String>,
        /// Preserve external names unless a breaking rename is requested
        #[arg(long, value_enum, default_value_t)]
        api: ExternalRenamePolicy,
        /// Target route prefix; valid only with `--api rename`
        #[arg(long, requires = "api")]
        route: Option<String>,
        /// Skip the confirmation prompt
        #[arg(long)]
        force: bool,
    },
    /// Complete the physical-storage stage of a rolling rename
    Storage {
        /// Current `<slice>.<resource>` identity
        resource: String,
        /// Exact durable campaign identifier
        #[arg(long)]
        complete: String,
        /// Attest that no old application version can access the old table
        #[arg(long)]
        old_version_retired: bool,
        /// Skip the confirmation prompt
        #[arg(long)]
        force: bool,
    },
}
