//! `g migration <description>` on a canonical project.
//!
//! **This is deliberately not a model declaration**, and it is the one
//! generator for which that is the answer rather than a gap. JDL v1 §2.1
//! puts ordered migration files outside JDL -- "immutable, append-only
//! history" -- §12's naming rules say "authors never name managed migrations
//! in JDL", and §2 lists "writing an append-only migration" among the
//! *non-model* actions a familiar command may map to. A `migration` node would
//! contradict all three, and would turn history into desired state: a model
//! carrying every migration ever written is one the compiler must keep
//! agreeing with forever.
//!
//! So it is the reader's own migration, joined to the plan beside the derived
//! ones. That is what keeps it honest rather than a side effect: the
//! materializer allocates its version from the observed Flyway history,
//! records a `Missing` precondition for the path, and emits an ordinary
//! `AppendMigration` operation -- so it appears in `--pretend`, it is part of
//! the reviewed digest, and a second run refuses rather than overwriting.
//!
//! The body is a comment and nothing else. Guessing SQL for a migration
//! nobody derived is the one thing this must not do.

use crate::Invocation;
use crate::cli::GenerateArgs;
use crate::model_generate::{PreparedMutation, finish_generation};
use jails_contracts::RenderedMigration;
use jails_model::ModelPatch;
use jails_support::{Failure, Result};
use std::collections::BTreeSet;

/// What the file says until the reader writes the change.
const BODY: &str = "-- Forward-only migration. Write explicit SQL below.\n";

pub(crate) fn run(args: GenerateArgs, invocation: Invocation) -> Result<()> {
    reject_unsupported_options(&args)?;
    let description = description(&args.name)?;
    let current = crate::model_command::Current::load(&invocation)?;
    finish_generation(PreparedMutation {
        name: args.name.clone(),
        invocation,
        // The source is unchanged and so is the model: an empty batch is the
        // honest patch for an action that declares nothing. Writing one
        // anyway -- a `migration` node -- is what JDL v1 §2.1 forbids.
        next_source: current.source.clone(),
        current,
        patch: ModelPatch::Batch(Vec::new()),
        authored_migration: Some(RenderedMigration {
            logical_name: description,
            bytes: BODY.as_bytes().to_vec(),
            // Nothing owns it. A derived migration names the declaration it
            // came from so removing that declaration can find it; this one
            // came from a reader, and claiming a semantic owner would make a
            // later `destroy` believe it may retire somebody's hand-written
            // SQL.
            semantic_ids: BTreeSet::new(),
        }),
        reader_paths: Vec::new(),
    })
}

/// The Flyway description, checked against JDL v1 §12.6's shape.
///
/// Lower snake case, because the allocated path is `V<n>__<description>.sql`
/// and the materializer refuses anything else -- catching it here means the
/// reader is told what to type rather than shown a compiler-produced-invalid
/// message about a name they chose.
fn description(name: &str) -> Result<String> {
    let description = jails_support::identifier::sql_name(name)
        .map_err(|error| Failure::Told(format!("{error}")))?;
    if description.is_empty()
        || !description
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(Failure::Told(format!(
            "`{name}` is not a migration description\n       fix: use lower snake case, for example `jails g migration add_note_archived_at`"
        )));
    }
    Ok(description)
}

fn reject_unsupported_options(args: &GenerateArgs) -> Result<()> {
    let unsupported = !args.fields.is_empty()
        || args.timestamps
        || args.package.is_some()
        || args.default_literal.is_some()
        || args.backfill_file.is_some()
        || !args.indexes.is_empty()
        || args.strategy_on.is_some()
        || args.strategy_yields.is_some()
        || args.via.is_some()
        || args.order_by.is_some()
        || args.limit.is_some()
        || args.on_conflict.is_some()
        || args.path.is_some()
        || args.select.is_some()
        || !args.set.is_empty()
        || args.if_match.is_some()
        || !args.bind.is_empty()
        || args.method.is_some()
        || args.consumes.is_some();
    if unsupported {
        return Err(Failure::Told(
            "a migration is a description and nothing else -- jails does not derive its SQL\n       fix: run `jails g migration <description>`, then write the statement in the file it allocates"
                .to_string(),
        ));
    }
    Ok(())
}
