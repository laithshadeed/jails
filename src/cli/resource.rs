//! `jails resource`: the durable-identity surface, as clap sees it.
//!
//! Split out of `src/cli.rs` for the board's largest-module rung, and it is
//! the natural seam: three enums that only this one command's frontends
//! read, none of them named anywhere else in the definition.

use super::{ColumnRenamePolicy, TypeChangeStrategy};
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum ResourceCommand {
    /// Check that recorded identity, generated source and sealed migrations agree
    Status {
        /// Simple entity name or fully qualified generated Java type
        selector: String,
    },
    /// Regenerate the files for preserved storage without another create migration
    Revive {
        /// Simple entity name or fully qualified generated Java type
        selector: String,
        /// The preserved SQL table name
        #[arg(long)]
        table: String,
    },
    /// Rewrite a sealed migration whose bytes changed
    ///
    /// On a modelled project this takes no arguments: managed output is
    /// rendered from the model, so repair is ordinary compilation with two
    /// guards waived, and there is nothing to select or to choose a strategy
    /// between. `jails sync` waives the first of them -- it writes back any
    /// managed file that is simply gone -- so the case left here is a sealed
    /// migration whose text a reader changed, which `sync` deliberately will
    /// not overwrite because a database has already run the old one.
    Repair {
        /// Simple entity name or fully qualified generated Java type
        selector: Option<String>,
    },
    /// Evolve one field through a new forward migration
    Field {
        #[command(subcommand)]
        command: ResourceFieldCommand,
    },
    /// Add an index to a table that already exists
    Index {
        #[command(subcommand)]
        command: ResourceIndexCommand,
    },
}

#[derive(Subcommand)]
pub(crate) enum ResourceIndexCommand {
    /// Append one composite or ordered index and its migration
    ///
    ///   jails entity index add Message 'customer_id, created_at desc'
    ///
    /// The columns are the ones the table has, each optionally `asc`/`desc`
    /// and nothing else -- arbitrary SQL is refused rather than recorded as
    /// trusted generated SQL, the same rule `--index` follows at creation.
    Add {
        entity: String,
        columns: String,
        /// Subpackage containing the generated entity
        #[arg(long)]
        package: Option<String>,
    },
    /// Drop one previously declared composite or ordered index
    ///
    ///   jails entity index remove Message 'customer_id, created_at desc' \
    ///     --confirm-index idx_message_index_ab12cd34ef56
    Remove {
        entity: String,
        columns: String,
        /// The physical index name that will be dropped
        #[arg(long)]
        confirm_index: String,
        /// Subpackage containing the generated entity
        #[arg(long)]
        package: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum ResourceFieldCommand {
    /// Add one field and append its migration
    Add {
        entity: String,
        field_spec: String,
        /// Typed value used to backfill rows before a required field is enforced
        #[arg(long, conflicts_with = "backfill_file")]
        default_literal: Option<String>,
        /// Project-relative reader-owned SQL used to backfill existing rows
        #[arg(long, conflicts_with = "default_literal")]
        backfill_file: Option<String>,
        /// Subpackage containing the generated entity
        #[arg(long)]
        package: Option<String>,
    },
    /// Rename a field with an explicit physical-column policy
    Rename {
        entity: String,
        field: String,
        new_name: String,
        #[arg(long, value_enum)]
        column: ColumnRenamePolicy,
        #[arg(long)]
        package: Option<String>,
    },
    /// Change a field's type through a checked strategy
    Type {
        entity: String,
        field: String,
        #[arg(long)]
        to: String,
        #[arg(long, value_enum)]
        strategy: TypeChangeStrategy,
        #[arg(long)]
        package: Option<String>,
    },
    /// Change whether a field accepts null values
    Nullability {
        entity: String,
        field: String,
        #[arg(
            long,
            conflicts_with = "required",
            required_unless_present = "required"
        )]
        nullable: bool,
        #[arg(
            long,
            conflicts_with = "nullable",
            required_unless_present = "nullable"
        )]
        required: bool,
        /// Project-relative SQL that removes nulls before `--required`
        #[arg(long)]
        backfill_file: Option<String>,
        #[arg(long)]
        package: Option<String>,
    },
    /// Drop a field after confirming the physical column it maps to
    Drop {
        entity: String,
        field: String,
        #[arg(long)]
        confirm_column: String,
        #[arg(long)]
        package: Option<String>,
    },
}
