//! Driving one commit: from a decided request to a durable outcome.
//!
//! The half of `route` that runs *after* the request is assembled. It captures
//! the store, prepares the transition, hands it to the executor and turns a
//! `CommitError` into something a person can act on.
//!
//! Split from [`super::request`] along the seam `pending.md` §8.1 names: that
//! module decides, this one drives. The division is not cosmetic — everything
//! here needs a lock, a journal and a project on disk, and nothing there does.

use super::*;

/// The store as it is, read once per command.
pub(super) fn observed(project: &Project) -> Result<ObservedStore> {
    jails_commit::store::Store::at(project.root()).observe()
}

/// Finish an interrupted transaction *before* anything plans against the
/// project, and say what it finished.
///
/// The executor recovers under the lock too, and has since §R4.4. What it
/// cannot do from there is make the caller's plan true again: by the time the
/// lock is taken the request has already been measured against the project the
/// interruption left behind, so the only honest answer is `RecoveredPrior-
/// Transaction` — replan. [`commit`] below does exactly that, but twelve
/// routes assemble their own `DesiredChangeSet` and call [`commit_set`]
/// directly, and for those the replan had nowhere to happen: the reader was
/// told to run the command again, and running it again said the same thing
/// until the recovery finally settled.
///
/// Recovering here, once, at the entry point every mutating command shares,
/// means every route plans against a settled project and the commit-time pass
/// is the race backstop it was designed to be — two processes, not two phases
/// of one.
///
/// Read-only when there is nothing to do: the lock is taken only after the
/// journal says a transaction is unfinished, so the ordinary command pays one
/// directory read.
pub fn finish_interrupted(project: &Project) -> Result<Vec<RecoveryOutcome>> {
    if jails_commit::store::Store::at(project.root())
        .unfinished_transactions()
        .is_empty()
    {
        return Ok(Vec::new());
    }
    let handle = ProjectHandle::at(project.root())?;
    let locked =
        LockedProject::acquire(handle, "finish an interrupted transaction").map_err(describe)?;
    let outcome =
        jails_commit::recover::recover_locked(&locked).map_err(|error| error.to_string())?;
    Ok(if outcome.is_clean() {
        Vec::new()
    } else {
        vec![outcome]
    })
}

/// Steps 3 and 5 to 7: capture, prepare, lock, commit.
///
/// Replans exactly once, and §R3.4 says why once: recovery may have finished
/// an interrupted earlier transaction between the read and the lock, which
/// makes this plan describe a store that has moved. That is not an error --
/// nothing is wrong, the plan is simply stale -- so the store is reread and
/// the request measured against it again. A *second* such answer is
/// `RecoveryBlocked` rather than a third attempt: recovery changing
/// authoritative state twice in one invocation is a loop, not a race.
pub(super) fn commit(
    run: &Run,
    request: Request,
    declaration: &ReadDeclaration,
    asked: &Asked,
) -> Result<Outcome> {
    commit_with_subject(run, request, declaration, asked, None)
}

/// Drive an ownership request whose canonical operation has a more specific
/// plan subject than ordinary reconciliation.
pub(super) fn commit_subject(
    run: &Run,
    request: Request,
    declaration: &ReadDeclaration,
    asked: &Asked,
    subject: PlannedSubject,
) -> Result<Outcome> {
    commit_with_subject(run, request, declaration, asked, Some(subject))
}

fn commit_with_subject(
    run: &Run,
    request: Request,
    declaration: &ReadDeclaration,
    asked: &Asked,
    subject: Option<PlannedSubject>,
) -> Result<Outcome> {
    let project = run.project();
    if let Some(plan) = retry_plan(run, declaration, asked)? {
        return retry_effect(run, plan);
    }
    let mut recovery = Vec::new();
    for attempt in 0..2 {
        // Read once per attempt, and let the same value decide the generation
        // the plan claims and the image the commit guards under the lock.
        // Reading them apart is how a plan comes to be written against a store
        // that moved in between.
        let observed = run.measure(jails_prepare::timing::TimingPhase::Parse, || {
            observed(project)
        })?;
        let mut set = request.clone().against(&observed)?;
        if let Some(subject) = &subject {
            set.subject = subject.clone();
        }
        let outcome = commit_set(run, set, declaration, asked)?;
        match outcome.replanned() {
            Some(outcome) if attempt == 0 => recovery.push(outcome),
            Some(_) => {
                return Err(
                    jails_support::Failure::Told("recovery changed this project twice while one command was running.\n                            fix: run `jails doctor`. Replanning again would be a loop rather than a \
                     race, so jails stops and says so instead."
                        .to_string()),
                );
            }
            None => return Ok(outcome.after_recovery(recovery)),
        }
    }
    unreachable!("the loop returns on every path")
}

/// Retry detection for routes whose original commit may make their ordinary
/// semantic lookup invalid (for example, destroying a resource retires it).
pub(super) fn retry_existing(run: &Run, asked: &Asked) -> Result<Option<Outcome>> {
    let reads = capture::capability_reads()?;
    retry_plan(run, &reads, asked)?
        .map(|plan| retry_effect(run, plan))
        .transpose()
}

/// Recognise a rerun of the canonical invocation that owns an unfinished or
/// failed effect. Project semantics are deliberately not planned first: the
/// original project transition is already durable and may have retired the
/// very resource the command named.
fn retry_plan(
    run: &Run,
    declaration: &ReadDeclaration,
    asked: &Asked,
) -> Result<Option<EffectRetryPlan>> {
    let store = jails_commit::store::Store::at(run.project().root());
    let receipts = store.read_receipts()?;
    let candidates: Vec<_> = receipts
        .into_iter()
        .filter(|receipt| {
            receipt.post_commit.iter().any(|row| {
                matches!(
                    row.state,
                    jails_protocol::effect::EffectState::Deferred
                        | jails_protocol::effect::EffectState::Pending { .. }
                        | jails_protocol::effect::EffectState::Running { .. }
                        | jails_protocol::effect::EffectState::Failed { .. }
                )
            })
        })
        .collect();
    if candidates.is_empty() {
        return Ok(None);
    }

    let (snapshot, _) = run.measure(jails_prepare::timing::TimingPhase::Observe, || {
        capture::projected(run.project(), declaration)
    })?;
    let invocation = asked.fingerprint(&snapshot)?;
    for receipt in candidates {
        let Some(recorded) = receipt.prepared.operation_identity.invocation.as_ref() else {
            continue;
        };
        if recorded != &invocation {
            continue;
        }
        let Some((index, row)) = receipt.post_commit.iter().enumerate().find(|(_, row)| {
            matches!(
                row.state,
                jails_protocol::effect::EffectState::Deferred
                    | jails_protocol::effect::EffectState::Pending { .. }
                    | jails_protocol::effect::EffectState::Running { .. }
                    | jails_protocol::effect::EffectState::Failed { .. }
            )
        }) else {
            continue;
        };
        let reason = match row.state {
            jails_protocol::effect::EffectState::Failed { .. } => EffectResumeReason::ExplicitRetry,
            _ => EffectResumeReason::Interrupted,
        };
        let checksum = ObjectId::from_bytes(jails_support::codec::sha256(&receipt.encode()?));
        return Ok(Some(EffectRetryPlan {
            invocation,
            receipt: ReceiptGuard {
                transaction: receipt.transaction,
                generation: receipt.generation,
                record_checksum: checksum,
            },
            operation: receipt.prepared.operation_id,
            effect_index: u32::try_from(index)
                .map_err(|_| "a receipt carries too many effects to address one by index")?,
            effect_id: row.id,
            effect: row.effect.clone(),
            expected_state: row.state.clone(),
            reason,
        }));
    }
    Ok(None)
}

fn retry_effect(run: &Run, plan: EffectRetryPlan) -> Result<Outcome> {
    let fingerprint = plan.invocation.request_syntax;
    let preview = EffectRetryReport::describe(&plan);
    if !run.write {
        return Ok(Outcome::EffectRetry(Box::new(
            super::session::EffectRetryOutcome {
                report: preview,
                result: None,
                review: Default::default(),
                timings: run.timing_trace(),
                fingerprint,
            },
        )));
    }

    let store = jails_commit::store::Store::at(run.project().root());
    let result = run.measure(jails_prepare::timing::TimingPhase::Container, || {
        jails_commit::runtime::retry_with_migrations(
            &store,
            run.project().root(),
            &plan,
            run.debug,
            |datasource, migrations, debug| {
                jails_drive::migrate::apply_effect(
                    run.project(),
                    datasource.as_str(),
                    migrations,
                    debug,
                )
                .map_err(|failure| failure.to_string())
            },
        )
        .map_err(describe)
    })?;
    let terminal = store
        .read_receipt(&plan.receipt.transaction)?
        .post_commit
        .get(usize::try_from(plan.effect_index).map_err(|_| "invalid effect index")?)
        .map(|row| row.state.clone())
        .unwrap_or_else(|| plan.expected_state.clone());
    Ok(Outcome::EffectRetry(Box::new(
        super::session::EffectRetryOutcome {
            report: EffectRetryReport::describe_result(&plan, terminal),
            result: Some(result),
            review: Default::default(),
            timings: run.timing_trace(),
            fingerprint,
        },
    )))
}

/// The same steps, for a request that already knows what the store becomes.
///
/// A one-shot does not go through [`Request`]: there is no ownership to
/// reconcile, so there is nothing to measure against the store. It states its
/// receipt and its file and that is the whole transition.
pub(super) fn commit_set(
    run: &Run,
    set: DesiredChangeSet,
    declaration: &ReadDeclaration,
    asked: &Asked,
) -> Result<Outcome> {
    let project = run.project();
    let request_fingerprint = asked.syntax_fingerprint()?;
    let bundle = prepare_set(run, set, declaration, Some(asked))?;
    if !run.write {
        let report = jails_prepare::report::Report::of_bundle(&bundle)?;
        return Ok(Outcome::Planned(Box::new(PreparedOutcome {
            report,
            bundle,
            timings: run.timing_trace(),
        })));
    }
    let (locked, result) = run.measure(jails_prepare::timing::TimingPhase::Commit, || {
        let handle = ProjectHandle::at(project.root())?;
        let locked = LockedProject::acquire(handle, &asked.display()).map_err(describe)?;
        let result = execute::commit(&locked, &bundle).map_err(describe)?;
        Ok::<_, jails_support::Failure>((locked, result))
    })?;
    // The project lock goes *before* the runtime reconciliation, which is
    // §R6.6's rule and not an optimisation: `docker compose up -d` can take a
    // minute pulling an image, and holding the mutation lock across it would
    // make every other jails command in the tree wait on a container.
    drop(locked);
    Ok(Outcome::Committed(
        reconciled(run, result)?,
        Box::new(bundle.review),
        run.timing_trace(),
        request_fingerprint,
    ))
}

/// Attempt the effect the commit recorded, if it recorded one.
///
/// The commit is already durable when this runs. A failed attempt is
/// therefore reported, never unwound: the project is in the state it was
/// asked for, and what is missing is a container -- which the receipt now
/// carries a retryable descriptor for.
pub(super) fn reconciled(run: &Run, result: CommitResult) -> Result<CommitResult> {
    let CommitResult::Committed(committed) = result else {
        return Ok(result);
    };
    let mut committed = *committed;
    if committed.receipt.post_commit.is_empty() {
        return Ok(CommitResult::Committed(Box::new(committed)));
    }
    let store = jails_commit::store::Store::at(run.project().root());
    let effect = run.measure(jails_prepare::timing::TimingPhase::Container, || {
        jails_commit::runtime::reconcile_with_migrations(
            &store,
            run.project().root(),
            &committed.receipt.transaction_id,
            run.debug,
            |datasource, migrations, debug| {
                jails_drive::migrate::apply_effect(
                    run.project(),
                    datasource.as_str(),
                    migrations,
                    debug,
                )
                .map_err(|failure| failure.to_string())
            },
        )
        .map_err(describe)
    })?;
    if let Ok(published) = store.read_receipt(&committed.receipt.transaction_id) {
        committed.receipt.post_commit = published.post_commit;
    }
    Ok(CommitResult::Committed(Box::new(
        jails_commit::outcome::CommittedResult {
            effect,
            ..committed
        },
    )))
}

/// Everything a commit does except taking the lock and activating.
///
/// A plan is not a weaker commit that stops early by accident -- it is the
/// same computation, and the bundle it produces is the *exact* one the commit
/// would have activated. Anything that describes a transition therefore
/// describes this value, which is what makes `--pretend` an answer about what
/// will happen rather than a second implementation that hopes to agree.
pub(super) fn prepare_set(
    run: &Run,
    set: DesiredChangeSet,
    declaration: &ReadDeclaration,
    asked: Option<&Asked>,
) -> Result<pipeline::PreparedBundle> {
    let project = run.project();
    let (snapshot, mut projection) = run
        .measure(jails_prepare::timing::TimingPhase::Observe, || {
            capture::projected(project, declaration)
        })?;
    let observed = run.measure(jails_prepare::timing::TimingPhase::Parse, || {
        observed(project)
    })?;
    let (loaded, read_set, invocation) = run.measure(
        jails_prepare::timing::TimingPhase::Project,
        || -> Result<_> {
            if let Some(store) = &observed.ledger {
                projection.record(&store.resources);
            }
            let root = capture::canonical_root(project.root())?;
            let machine = if project.root().join(".jails").is_dir() {
                MachineRootPresence::Present
            } else {
                MachineRootPresence::Absent
            };
            let loaded = Bootstrap::begin(root, machine)
                .with_ledger(None)?
                .classify()?;
            let read_set = snapshot.read_set()?;
            let invocation = match asked {
                Some(asked) => Some(asked.fingerprint(&snapshot)?),
                None => None,
            };
            Ok((loaded, read_set, invocation))
        },
    )?;
    let context = PreparationContext {
        read_set,
        // Nothing is rendered from a template on this route yet: a recipe
        // hands over bytes it already produced. An empty store is therefore
        // the honest value, and `TemplateStore::resolve` refuses anything
        // that tries to render from bytes nothing recorded.
        templates: TemplateStore::new(Vec::new())?,
        observed_generation: observed.generation(),
        observed_store: observed,
        operation_context: Default::default(),
        preparation: Default::default(),
        // Derived from the canonical request, not from the flag alone: §R3.3
        // makes every request variant without a `no_start` field ineligible
        // and says it behaves as `no_start == true`, so a maintenance action
        // cannot reconcile a runtime by accident.
        start_services: asked.is_some_and(Asked::starts_services),
        // Computed against the same capture the plan was, so the row for
        // `jails.toml` describes the bytes this plan actually read rather
        // than whatever is on disk by the time it is asked for.
        invocation,
        // The durable object store, as the one question preparation asks of
        // it: given a recorded base, the bytes jails wrote. A three-way merge
        // measures the reader's edit and the generator's change from exactly
        // those, and there is nowhere else to get them -- the file on disk is
        // one of the two sides, not the origin.
        objects: {
            let at = jails_commit::store::Store::at(project.root()).objects();
            std::sync::Arc::new(move |id: &jails_protocol::identity::ObjectId| {
                jails_commit::store::read_object(&at, id).ok()
            })
        },
        timings: run.timing_trace(),
    };
    let bundle = run.measure(jails_prepare::timing::TimingPhase::Prepare, || {
        pipeline::prepare(
            &loaded,
            CommitPlan::Apply(set),
            snapshot,
            projection,
            context,
        )
    })?;
    run.measure(jails_prepare::timing::TimingPhase::Verify, || {
        bundle.change.validate()
    })?;
    Ok(bundle)
}

/// A commit failure as the one line a person reads.
///
/// Every one of these is a refusal before anything was activated -- that is
/// what `CommitError` *means*, and it is why this is a plain message rather
/// than a recovery instruction.
pub(super) fn describe(error: CommitError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::describe;
    use jails_commit::outcome::CommitError;

    #[test]
    fn commit_errors_use_their_human_facing_message() {
        let message = describe(CommitError::StaleInput(
            "the project changed; rerun the command".to_string(),
        ));

        assert_eq!(message, "the project changed; rerun the command");
        assert!(!message.contains("StaleInput"));
    }
}
