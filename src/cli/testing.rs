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
