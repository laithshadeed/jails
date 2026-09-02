//! Arguments for `jails test daemon`.
//!
//! The daemon itself is `jails-drive`'s `testd`; this is the subcommand shape
//! that reaches it, kept with the rest of the clap tree so `jails commands`
//! describes it at the depth a reader types (`test daemon status`, not
//! `test`).

use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum TestCommand {
    /// Inspect or control this project's resident test process
    Daemon {
        #[command(subcommand)]
        action: TestDaemonAction,
    },
}

#[derive(Subcommand)]
pub(crate) enum TestDaemonAction {
    /// Say whether the daemon is running
    Status,
    /// Stop the daemon if it is running
    Stop,
    /// Replace the daemon and report its new status
    Restart,
}
