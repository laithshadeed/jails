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
//!
//! `Rolling` is a variant so it can be refused by name: the compiler plans one
//! reviewed transition, so a rolling rename is a sequence of ordinary plans,
//! and a reader asking for one is told that and which two strategies exist,
//! rather than clap's "invalid value".

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

impl From<RenameStrategy> for jails_spec::spec::policy::RenameStrategy {
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

impl From<ExternalRenamePolicy> for jails_spec::spec::policy::ExternalRenamePolicy {
    fn from(value: ExternalRenamePolicy) -> Self {
        match value {
            ExternalRenamePolicy::Preserve => Self::Preserve,
            ExternalRenamePolicy::Rename => Self::Rename,
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum RenameCommand {
    /// Rename a managed entity with an explicit storage strategy
    #[command(name = "entity", visible_alias = "resource")]
    Resource {
        /// `<slice>.<current-name>` selector resolved before planning
        from: String,
        /// New entity name
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
        /// Answer the confirmation prompt yes in advance
        #[arg(long, alias = "force")]
        yes: bool,
    },
}
