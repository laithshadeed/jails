//! Closed request and application-tool command vocabularies.

use clap::{Args, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum DatabaseClientArg {
    Pgcli,
    Psql,
}

#[derive(Subcommand)]
pub(crate) enum DbCommand {
    /// Open a real PostgreSQL client against a declared datasource
    Console {
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long, value_enum, default_value = "pgcli")]
        client: DatabaseClientArg,
        #[arg(long, requires_if("pgcli", "client"))]
        single_connection: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum ContractFormatArg {
    Openapi,
    JsonSchema,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum ContractScopeArg {
    Declared,
    Source,
}

#[derive(Subcommand)]
pub(crate) enum ContractCommand {
    /// Project a portable HTTP contract to stdout
    Emit {
        #[arg(long, value_enum, default_value = "openapi")]
        format: ContractFormatArg,
        /// Reserved project-relative output path; omitted writes stdout
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Refuse backward-incompatible route changes
    Check {
        #[arg(long)]
        against: String,
        #[arg(long, value_enum, default_value = "declared")]
        scope: ContractScopeArg,
    },
}

#[derive(Args)]
pub(crate) struct HttpRequestArgs {
    pub(crate) method: String,
    pub(crate) target: String,
    #[arg(long)]
    pub(crate) profile: Option<String>,
    #[arg(long)]
    pub(crate) base_url: Option<String>,
    #[arg(long = "param")]
    pub(crate) params: Vec<String>,
    #[arg(long = "query")]
    pub(crate) query: Vec<String>,
    #[arg(long = "header")]
    pub(crate) headers: Vec<String>,
    #[arg(long = "header-env")]
    pub(crate) header_env: Vec<String>,
    #[arg(long, conflicts_with = "data")]
    pub(crate) json: Option<String>,
    #[arg(long, conflicts_with = "json")]
    pub(crate) data: Option<String>,
    #[arg(long)]
    pub(crate) timeout: Option<String>,
    #[arg(long)]
    pub(crate) follow: bool,
    #[arg(long)]
    pub(crate) print: bool,
}

#[derive(Args)]
pub(crate) struct RunnerArgs {
    #[arg(long)]
    pub(crate) file: PathBuf,
    #[arg(long = "profile")]
    pub(crate) profiles: Vec<String>,
    #[arg(long)]
    pub(crate) main: Option<String>,
    #[arg(long)]
    pub(crate) compile: bool,
}

#[derive(Args)]
pub(crate) struct LogsArgs {
    pub(crate) services: Vec<String>,
    #[arg(long)]
    pub(crate) follow: bool,
    #[arg(long)]
    pub(crate) since: Option<String>,
    #[arg(long, default_value_t = 200)]
    pub(crate) tail: usize,
}
