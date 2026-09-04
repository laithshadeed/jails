//! Closed request and application-tool command vocabularies.

use clap::{Args, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum DatabaseClientArg {
    Pgcli,
    Psql,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum WebModeArg {
    None,
    Random,
    Configured,
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
    /// HTTP method (GET, POST, PUT, DELETE, PATCH, etc.)
    pub(crate) method: String,
    /// Target route, path, or URL (e.g. /health, /api/orders, or a declared route)
    pub(crate) target: String,
    /// Spring profile to read configuration from
    #[arg(long)]
    pub(crate) profile: Option<String>,
    /// Base URL for the request (defaults to resolved server port, e.g. http://127.0.0.1:8080)
    #[arg(long)]
    pub(crate) base_url: Option<String>,
    /// Path parameter replacement in the target route (name=value)
    #[arg(long = "param")]
    pub(crate) params: Vec<String>,
    /// Query parameter to append (name=value)
    #[arg(long = "query")]
    pub(crate) query: Vec<String>,
    /// HTTP header to include (Name: Value)
    #[arg(long = "header")]
    pub(crate) headers: Vec<String>,
    /// HTTP header populated from environment variable (Name=ENV_VAR)
    #[arg(long = "header-env")]
    pub(crate) header_env: Vec<String>,
    /// JSON body content to send with the request
    #[arg(long, conflicts_with = "data")]
    pub(crate) json: Option<String>,
    /// Raw request body data to send
    #[arg(long, conflicts_with = "json")]
    pub(crate) data: Option<String>,
    /// Request timeout (e.g. 5s, 30s)
    #[arg(long)]
    pub(crate) timeout: Option<String>,
    /// Follow HTTP redirects
    #[arg(long)]
    pub(crate) follow: bool,
    /// Print the curl command instead of executing it
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
    #[arg(long, value_enum, default_value = "none")]
    pub(crate) web: WebModeArg,
    #[arg(long)]
    pub(crate) compile: bool,
    /// Authorize the printed non-dev/configured-web preflight in automation.
    #[arg(long)]
    pub(crate) yes: bool,
}

#[derive(Args)]
pub(crate) struct ConsoleArgs {
    #[arg(long = "profile")]
    pub(crate) profiles: Vec<String>,
    #[arg(long)]
    pub(crate) main: Option<String>,
    #[arg(long, value_enum, default_value = "none")]
    pub(crate) web: WebModeArg,
    #[arg(long)]
    pub(crate) compile: bool,
    /// Authorize the printed non-dev/configured-web preflight in automation.
    #[arg(long)]
    pub(crate) yes: bool,
    /// Compatibility spelling for the default existing-output mode.
    #[arg(long, hide = true, conflicts_with = "compile")]
    pub(crate) no_build: bool,
    /// Extra arguments forwarded verbatim to JShell.
    #[arg(last = true, allow_hyphen_values = true)]
    pub(crate) args: Vec<String>,
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
