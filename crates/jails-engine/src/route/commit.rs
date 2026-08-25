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
    let project = run.project();
    let mut recovery = Vec::new();
    for attempt in 0..2 {
        // Read once per attempt, and let the same value decide the generation
        // the plan claims and the image the commit guards under the lock.
        // Reading them apart is how a plan comes to be written against a store
        // that moved in between.
        let observed = run.measure(jails_prepare::timing::TimingPhase::Parse, || {
            observed(project)
        })?;
        let set = request.clone().against(&observed)?;
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
    let bundle = prepare_set(run, set, declaration, Some(asked))?;
    if !run.write {
        return Ok(Outcome::Planned(Box::new(PreparedOutcome {
            report: jails_prepare::report::Report::of(&bundle.change)?,
            review: bundle.review,
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
    if committed.receipt.post_commit.is_empty() {
        return Ok(CommitResult::Committed(committed));
    }
    let store = jails_commit::store::Store::at(run.project().root());
    let effect = run.measure(jails_prepare::timing::TimingPhase::Container, || {
        jails_commit::runtime::reconcile(
            &store,
            run.project().root(),
            &committed.receipt.transaction_id,
            run.debug,
        )
        .map_err(describe)
    })?;
    Ok(CommitResult::Committed(Box::new(
        jails_commit::outcome::CommittedResult {
            effect,
            ..*committed
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
    format!("{error:?}")
}
