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
