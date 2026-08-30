//! Arguments for `jails history` and `jails history show`.
//!
//! Argument shapes only — the receipts themselves are `jails-commit`'s, and
//! reading them is `history_command.rs`'s. Kept beside the rest of the clap
//! tree so `jails commands` walks one structure to describe the whole surface.

/// Arguments for authenticated receipt history.
#[derive(clap::Args)]
pub(crate) struct HistoryArgs {
    /// Maximum receipts to return, newest generation first
    #[arg(long, default_value_t = 20)]
    pub(crate) limit: usize,
}

/// Arguments for one authenticated receipt.
#[derive(clap::Args)]
pub(crate) struct ShowArgs {
    /// Full 64-character transaction identifier
    pub(crate) transaction: String,
    /// Explain the receipt's preparation and undo eligibility evidence
    #[arg(long)]
    pub(crate) why: bool,
}

/// Arguments for a safe receipt-driven forward file restoration.
#[derive(clap::Args)]
pub(crate) struct UndoArgs {
    /// Full 64-character transaction identifier
    pub(crate) transaction: String,
    /// Permit a three-way merge when a text replacement changed afterwards
    #[arg(long)]
    pub(crate) merge: bool,
}
