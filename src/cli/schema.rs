//! Closed command-line vocabularies for schema observation and reconciliation.

use super::RunServicesArg;
use clap::{Subcommand, ValueEnum};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum SchemaAuthorityArg {
    Declared,
    Migrations,
    Live,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum IntrospectFormatArg {
    Human,
    Json,
    Manifest,
}

#[derive(Subcommand)]
pub(crate) enum IntrospectCommand {
    /// Observe a declared PostgreSQL datasource without mutating it
    Db {
        /// Declared datasource name
        #[arg(long, value_name = "NAME")]
        datasource: String,
        /// PostgreSQL schema to observe
        #[arg(long, default_value = "public")]
        schema: String,
        /// Restrict table-owned objects with a simple `*` glob
        #[arg(long, value_name = "GLOB")]
        table: Option<String>,
        /// Result projection
        #[arg(long, value_enum, default_value = "human")]
        format: IntrospectFormatArg,
        /// Existing-service policy; start is refused until `jails start` is run explicitly
        #[arg(long, value_enum, default_value = "existing")]
        services: RunServicesArg,
    },
}

#[derive(Subcommand)]
pub(crate) enum SchemaCommand {
    /// Compare two independent schema authorities
    Diff {
        #[arg(long, value_enum)]
        from: SchemaAuthorityArg,
        #[arg(long, value_enum)]
        to: SchemaAuthorityArg,
        /// Required when either authority is live
        #[arg(long, value_name = "NAME")]
        datasource: Option<String>,
        #[arg(long, default_value = "public")]
        schema: String,
        #[arg(long, value_enum, default_value = "existing")]
        services: RunServicesArg,
        #[arg(long, value_name = "MANIFEST")]
        manifest: Option<std::path::PathBuf>,
    },
}

#[derive(Subcommand)]
pub(crate) enum MigrateCommand {
    /// Classify destructive and deployment-sensitive migration statements
    Lint {
        #[arg(long, value_name = "MANIFEST")]
        manifest: Option<std::path::PathBuf>,
    },
}
