//! Running the one aggregate post-commit effect, after the commit.
//!
//! ## Why this is not a step in the commit
//!
//! plan.md §R6.6 keeps container start/stop explicitly outside the project
//! transaction: it is not a file operation, so no preimage restores it. What
//! makes it survivable anyway is that the commit records a *descriptor* first.
//! The project is correct the moment the ledger is written; the containers
//! catch up afterwards, and if they do not, the descriptor says exactly what a
//! retry would do.
//!
//! ## Why the project lock is not held while it runs
//!
//! §R6.6: the project lock is released before the subprocess, and a separate
//! `effects.lock` is held for it. A `docker compose up -d` can take a minute,
//! and holding the mutation lock across it would make every other jails
//! command in the tree wait on a container pull. Contention on the effect lock
//! is `EffectBusy` rather than a queue: two runtime reconciliations racing on
//! one project would interleave stop and up.
//!
//! ## Why `--file` is an object and never `compose.yaml`
//!
//! The document is frozen at preparation. Between the commit and the attempt
//! somebody may edit the live file; running against what they wrote would act
//! on services this transition never described. `--project-directory` still
//! points at the project, because relative bind mounts in the document are
//! resolved against it -- pointing it at the object store would silently
//! relocate every volume.

use jails_support::Result;
use std::collections::BTreeSet;
use std::path::Path;

use jails_protocol::database::MigrationInputV1;
use jails_protocol::effect::{EffectFailureCode, EffectId, EffectState, PostCommitEffect};
use jails_protocol::identity::{ObjectId, OperationId, ServiceName};
use jails_protocol::request::DatasourceRef;
use jails_support::codec::{Codec, Encoder, domain_hash};
use jails_support::process::{Diagnostics, OutputMode, compose_spec, run as run_process};

use crate::journal::ReceiptV1;
use crate::outcome::{CommitEffectOutcome, CommitError};
use crate::store::{self, Store};
use jails_support::lock::{Contention, Lock};

/// `SHA256("JAILS-EFFECT-1" || operation || index || descriptor)`, §R4.2's
/// idempotency key.
///
/// The index is in the key because a future transition with two effects would
/// otherwise give identical descriptors the same identity, and a retry could
/// not tell which one it had already run.
pub(crate) fn identify(
    operation: OperationId,
    index: u32,
    effect: &PostCommitEffect,
) -> Result<EffectId> {
    let mut encoder = Encoder::new();
    operation.encode(&mut encoder)?;
    encoder.u32(index);
    effect.encode(&mut encoder)?;
    Ok(EffectId::from_object(ObjectId::from_bytes(domain_hash(
        "JAILS-EFFECT-1",
        &encoder.finish()?,
    ))))
}

/// Attempt the deferred effect a published receipt carries.
///
/// Called with the project lock already released. A receipt with no effect, or
/// one whose effect already reached a terminal state, is `NotApplicable` --
/// running it again is not wrong, but claiming a fresh outcome for an attempt
/// that did not happen is.
pub fn reconcile(
    store: &Store,
    project_root: &Path,
    transaction: &jails_protocol::identity::TransactionId,
    debug: bool,
) -> std::result::Result<CommitEffectOutcome, CommitError> {
    reconcile_with_migrations(store, project_root, transaction, debug, |_, _, _| {
        Err(concat!(
            "the migration effect has no datasource adapter.\n       ",
            "fix: retry it through the jails command that recorded the receipt."
        )
        .to_string())
    })
}

/// Reconcile a receipt with the caller's credential-bearing datasource
/// adapter. The callback is invoked only after the project commit and outside
/// the project lock; descriptors and receipts remain credential-free.
pub fn reconcile_with_migrations<F>(
    store: &Store,
    project_root: &Path,
    transaction: &jails_protocol::identity::TransactionId,
    debug: bool,
    mut migrate: F,
) -> std::result::Result<CommitEffectOutcome, CommitError>
where
    F: FnMut(&DatasourceRef, &[MigrationInputV1], bool) -> std::result::Result<(), String>,
{
    let directory = store.receipt(transaction);
    let mut receipt = match ReceiptV1::read(&directory) {
        Ok(receipt) => receipt,
        // The commit itself succeeded; a receipt this process cannot read back
        // is a machine-state problem to report, never a reason to rerun files.
        Err(why) => return Err(CommitError::CorruptMachineState(why.to_string())),
    };
    let Some(row) = receipt.post_commit.first().cloned() else {
        return Ok(CommitEffectOutcome::NotApplicable);
    };
    if let Some(settled) = settled(&row.state, row.id) {
        return Ok(settled);
    }

    let _held = Lock::acquire(&store.effects_lock_path(), "runtime reconciliation").map_err(
        |why| match why {
            Contention::Held(_) => CommitError::EffectBusy(why.to_string()),
            Contention::Refused(_) => CommitError::PreActivationIo(why.to_string()),
        },
    )?;

    let state = match attempt(store, project_root, &row.effect, debug, &mut migrate) {
        Ok(()) => EffectState::Succeeded,
        Err(failure) => EffectState::Failed {
            attempt: 1,
            code: failure.code,
            summary: failure.summary,
        },
    };
    let outcome = match state {
        EffectState::Succeeded => CommitEffectOutcome::Succeeded { effect: row.id },
        _ => CommitEffectOutcome::Failed { effect: row.id },
    };
    receipt.post_commit[0].state = state;
    if receipt.persist(&directory).is_err() {
        // The attempt happened; only the record of it did not. Saying so is
        // the honest answer, and it is exactly what `DeferredError` is for.
        return Ok(CommitEffectOutcome::DeferredError {
            effect: row.id,
            error: crate::outcome::CommittedEffectError::ReceiptIo,
        });
    }
    Ok(outcome)
}

/// The outcome of an effect that has already reached one, or `None` when
/// there is still an attempt to make.
///
/// A terminal state is never re-attempted and never rewritten: claiming a
/// fresh outcome for an attempt that did not happen is exactly the lie the
/// receipt exists to prevent. `Running` counts as settled and reports failure
/// -- a state left mid-attempt means the process died holding the effect lock,
/// and reporting success for it would be worse than reporting a failure a
/// retry can clear.
fn settled(state: &EffectState, id: EffectId) -> Option<CommitEffectOutcome> {
    match state {
        EffectState::Deferred | EffectState::Pending { .. } => None,
        EffectState::Succeeded => Some(CommitEffectOutcome::Succeeded { effect: id }),
        EffectState::Failed { .. } | EffectState::Running { .. } => {
            Some(CommitEffectOutcome::Failed { effect: id })
        }
        EffectState::Superseded { .. } => Some(CommitEffectOutcome::Superseded { effect: id }),
    }
}

struct EffectFailure {
    code: EffectFailureCode,
    summary: String,
}

/// §R3.3's closed idempotent sequence: stop, then remove, then up.
///
/// Never `down` and never `--remove-orphans`: both can destroy unmanaged
/// services, networks or volumes in a compose project jails shares with the
/// reader. Removal is bounded to the names that were managed and are no longer
/// declared, which is what makes it an inverse rather than a sweep.
fn attempt<F>(
    store: &Store,
    project_root: &Path,
    effect: &PostCommitEffect,
    debug: bool,
    migrate: &mut F,
) -> std::result::Result<(), EffectFailure>
where
    F: FnMut(&DatasourceRef, &[MigrationInputV1], bool) -> std::result::Result<(), String>,
{
    match effect {
        PostCommitEffect::ComposeReconcile {
            before_document,
            after_document,
            desired_services,
            stop_services,
            ..
        } => {
            if !stop_services.is_empty() {
                let document = object(store, before_document)?;
                let names = sorted(stop_services);
                for verb in [vec!["stop"], vec!["rm", "-f"]] {
                    run(project_root, &document, &verb, &names, debug)?;
                }
            }
            if !desired_services.is_empty() {
                let document = object(store, after_document)?;
                let names = sorted(&desired_services.keys().cloned().collect());
                run(project_root, &document, &["up", "-d"], &names, debug)?;
            }
            Ok(())
        }
        PostCommitEffect::ApplyMigrations {
            datasource,
            migrations,
        } => migrate(datasource, migrations, debug).map_err(|summary| EffectFailure {
            code: EffectFailureCode::ExitNonzero,
            summary,
        }),
    }
}

fn object(
    store: &Store,
    id: &Option<ObjectId>,
) -> std::result::Result<std::path::PathBuf, EffectFailure> {
    let id = id.ok_or_else(|| EffectFailure {
        code: EffectFailureCode::Protocol,
        summary: "the effect names no compose document to run against".to_string(),
    })?;
    let path = store::object_path(&store.objects(), &id);
    if !path.is_file() {
        return Err(EffectFailure {
            code: EffectFailureCode::Protocol,
            summary: format!("the frozen compose document {id} is not in the object store"),
        });
    }
    Ok(path)
}

fn sorted(names: &BTreeSet<ServiceName>) -> Vec<String> {
    names.iter().map(ServiceName::to_string).collect()
}

/// §R3.3's exact argument vector for one step.
///
/// `--project-directory` is the project, because a relative bind mount in the
/// document resolves against it and pointing it at the object store would
/// silently relocate every volume. `--file` is the frozen object, which is
/// also what disables compose's implicit override discovery: an explicit file
/// list means `compose.override.yaml` is not read. `--` ends the options so a
/// service whose name begins with a dash cannot become one.
fn compose_args(
    project_root: &Path,
    document: &Path,
    verb: &[&str],
    names: &[String],
) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = vec![
        "--project-directory".into(),
        project_root.as_os_str().to_owned(),
        "--file".into(),
        document.as_os_str().to_owned(),
    ];
    args.extend(verb.iter().map(Into::into));
    args.push("--".into());
    args.extend(names.iter().map(Into::into));
    args
}

fn run(
    project_root: &Path,
    document: &Path,
    verb: &[&str],
    names: &[String],
    debug: bool,
) -> std::result::Result<(), EffectFailure> {
    let args = compose_args(project_root, document, verb, names);

    let spec = compose_spec(args).ok_or_else(|| EffectFailure {
        code: EffectFailureCode::Spawn,
        summary: "neither `docker compose` nor `docker-compose` is on PATH".to_string(),
    })?;
    // Captured rather than inherited: an effect attempt is not the reader's
    // interactive session, and its failure has to become a `summary` in the
    // receipt rather than scroll past.
    let spec = spec.output(OutputMode::Capture);
    match run_process(&spec, Diagnostics::from_flag(debug)) {
        Ok(done) if done.status.success() => Ok(()),
        Ok(done) => Err(EffectFailure {
            code: EffectFailureCode::ExitNonzero,
            summary: format!(
                "`compose {}` exited with {}: {}",
                verb.join(" "),
                done.status,
                first_line(&String::from_utf8_lossy(&done.stderr))
            ),
        }),
        Err(why) => Err(EffectFailure {
            code: EffectFailureCode::Spawn,
            summary: format!("could not run compose: {why}"),
        }),
    }
}

/// One line of a tool's complaint, which is what a receipt has room for.
fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("no output")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_protocol::identity::ProjectPath;
    use std::collections::{BTreeMap, BTreeSet};

    fn descriptor() -> PostCommitEffect {
        PostCommitEffect::ComposeReconcile {
            compose_output: ProjectPath::parse("compose.yaml").unwrap(),
            before_document: None,
            after_document: None,
            prior_managed_services: BTreeMap::new(),
            desired_services: BTreeMap::new(),
            stop_services: BTreeSet::new(),
        }
    }

    fn operation() -> OperationId {
        OperationId::from_bytes([7; 32])
    }

    /// The idempotency key is a function of the operation, the position and
    /// the descriptor -- so a retry of the same attempt is recognisable, and
    /// two effects of one operation are not confused for each other.
    #[test]
    fn an_effect_identity_depends_on_its_operation_and_its_position() {
        let one = identify(operation(), 0, &descriptor()).unwrap();
        assert_eq!(one, identify(operation(), 0, &descriptor()).unwrap());
        assert_ne!(one, identify(operation(), 1, &descriptor()).unwrap());
        assert_ne!(
            one,
            identify(OperationId::from_bytes([8; 32]), 0, &descriptor()).unwrap()
        );
    }

    /// §R3.3's closed sequence, and the two spellings it forbids.
    #[test]
    fn the_argument_vector_is_the_closed_one() {
        let args = compose_args(
            Path::new("/srv/demo"),
            Path::new("/srv/demo/.jails/objects/ab/cd"),
            &["up", "-d"],
            &["postgres".to_string(), "redis".to_string()],
        );
        let rendered: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            rendered,
            vec![
                "--project-directory",
                "/srv/demo",
                "--file",
                "/srv/demo/.jails/objects/ab/cd",
                "up",
                "-d",
                "--",
                "postgres",
                "redis",
            ]
        );
        // Never `down`, never `--remove-orphans`: both can destroy unmanaged
        // services, networks or volumes in a compose project jails shares.
        assert!(
            !rendered
                .iter()
                .any(|a| a == "down" || a == "--remove-orphans")
        );
    }

    /// An effect that has already reached a terminal state is reported, never
    /// re-run: a second attempt would claim an outcome nothing produced.
    #[test]
    fn a_settled_effect_is_reported_rather_than_attempted() {
        let id = identify(operation(), 0, &descriptor()).unwrap();
        assert_eq!(settled(&EffectState::Deferred, id), None);
        assert_eq!(settled(&EffectState::Pending { next_attempt: 2 }, id), None);
        assert_eq!(
            settled(&EffectState::Succeeded, id),
            Some(CommitEffectOutcome::Succeeded { effect: id })
        );
        assert_eq!(
            settled(&EffectState::Superseded { by: None }, id),
            Some(CommitEffectOutcome::Superseded { effect: id })
        );
        // A state left mid-attempt means the process died holding the lock.
        assert_eq!(
            settled(&EffectState::Running { attempt: 1 }, id),
            Some(CommitEffectOutcome::Failed { effect: id })
        );
    }
}
