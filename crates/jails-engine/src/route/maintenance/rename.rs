//! `jails rename <Old> <New>` — a type and everything that names it, as one
//! transition.

use super::*;

/// Rename a Java type across the project, as one transition.
///
/// **Reach for the language server first.** Neovim's `grn` (jdt.ls rename)
/// understands scope, so it will not touch an unrelated `Reward` in another
/// package, and where it works it is strictly better than this command. What
/// this exists for is the case jdt.ls cannot serve: the server is not attached,
/// the project does not currently compile (jdt.ls degrades badly there, and a
/// rename is often exactly how you are trying to fix it), or the rename has to
/// reach a file no buffer has opened.
///
/// It is textual, and two properties are what keep textual honest.
/// `jails_java::identifier` holds both: `Reward` never matches inside
/// `RewardHistory`, so the classic sed disaster cannot happen; and string
/// literals are left alone, because a literal is data and silently rewriting
/// `"Reward not found"` is a change nobody asked for. A literal that genuinely
/// names the class -- a `Class.forName` argument -- is therefore missed, which
/// is the safe direction and is reported rather than hidden.
///
/// The reason this is worth routing is written in V1's own source: it writes
/// every file's new contents, *then* moves the files, with a comment saying
/// that order at least leaves "one consistent state" if a write fails partway.
/// That is a defence against a partial rename, not a prevention of one -- a
/// rename that stops halfway leaves a project that does not compile, and
/// nothing records where it stopped.
///
/// Here every rewrite and every move is one commit. A move is a `Create` at
/// the destination and a `Delete` at the source in the same operation list, so
/// there is no moment where a file exists under both names or neither.
///
/// **Every `.java` file under `src/` is a declared read**, along with every
/// directory it walks. That is not bookkeeping: it is what makes §R4.3 step 2
/// able to refuse a rename planned against a tree somebody has changed since.
/// A file added under `src/` between planning and committing changes a
/// captured listing, and a rename that would silently skip it fails instead.
pub fn rename(run: &Run, old: &str, new: &str, force: bool) -> Result<Outcome> {
    let project = run.project();
    // Refused by name before anything is read. `JavaType::parse` accepts a
    // qualified name by splitting at the last dot, so without this a
    // `com.example.Reward` would be quietly read as the simple name `Reward`
    // in package `com.example` -- and then matched textually against every
    // source, which is not what the caller asked for at all.
    validate(old, new)?;
    let from = jails_protocol::identity::JavaType::parse(old)?;
    let to = jails_protocol::identity::JavaType::parse(new)?;

    // The walk that finds the sources is not itself a read the snapshot can
    // guard -- something has to look first. What it finds is then declared,
    // directories included, so the recheck covers both the contents of every
    // file considered and the membership of every directory walked.
    let mut reads = capture::capability_reads()?;
    let mut sources = Vec::new();
    for absolute in jails_java::java::source_files(&project.root().join("src")) {
        let relative = super::relative_path(project, &absolute)?;
        // The destination is declared as well, and its absence is the fact
        // the plan depends on. A file appearing at a destination between
        // planning and committing must fail the precondition rather than be
        // silently replaced -- which is only possible if the emptiness was
        // recorded in the first place. Every destination is derivable from
        // the source path alone, so this costs no second walk.
        reads = reads.file(destination_of(&relative, old, new)?);
        reads = reads.file(relative.clone());
        sources.push(relative);
    }
    for directory in walked_directories(&sources) {
        reads = reads.directory(directory);
    }
    let (snapshot, _) = capture::projected(project, &reads)?;

    // Which entities this rename is an identity transition *for*. An entity
    // is named by its `IntentId`, and the name is half of that -- so
    // `g record Reward` followed by `rename Reward Bonus` leaves a project
    // whose `Bonus.java` is owned by an entity called `Reward`, and
    // `destroy record Bonus` finds nothing while `destroy record Reward`
    // strands the file it claims to delete. plan.md §R2.5 permits exactly
    // this: a maintenance change may propose rows owned by real entities when
    // its `LedgerIntent` describes the exact identity transition.
    let store = observed(project)?;
    let mut renamed_entities = BTreeMap::new();
    for applied in store.ledger.iter().flat_map(|ledger| ledger.applied.iter()) {
        let EntityId::Intent(id) = &applied.id else {
            continue;
        };
        if id.name.as_str() != old {
            continue;
        }
        let mut renamed = id.clone();
        renamed.name = jails_protocol::identity::Name::parse(new)?;
        renamed_entities.insert(
            applied.id.clone(),
            (
                EntityId::Intent(renamed),
                applied.version.spec.clone(),
                applied.owners.clone(),
            ),
        );
    }

    let mut change = DesiredChange::maintenance(MaintenanceAttribution::Rename);
    let mut moved = 0usize;
    let mut occurrences = 0usize;
    let mut in_literals = 0usize;
    for source in &sources {
        let jails_protocol::snapshot::Captured::Present(file) = snapshot.read(source)? else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&file.bytes) else {
            continue;
        };
        let (updated, hits) = jails_java::identifier::replace_identifier(text, old, new);
        let destination = destination_of(source, old, new)?;
        if hits == 0 && &destination == source {
            continue;
        }
        occurrences += hits;
        in_literals += jails_java::identifier::literal_mentions(text, old);
        if &destination != source {
            // Refused from the capture, so a destination that appears between
            // planning and committing fails the precondition rather than
            // being quietly overwritten.
            if let jails_protocol::snapshot::Captured::Present(_) = snapshot.read(&destination)? {
                return Err(
                    format!("{destination} already exists -- rename or delete it first").into(),
                );
            }
            moved += 1;
            change.absences.push(jails_protocol::render::ManagedPath {
                path: source.clone(),
                resource: ResourceKey::WholeFile(source.clone()),
                // Never the caller's `--force`, which means "do not ask me".
                // This one overrides the *preimage guard*, and a rename that
                // skipped it could delete a source somebody changed while the
                // plan was being made -- while recreating stale bytes under
                // the new name. Two different meanings for one word is how
                // that gets shipped.
                force: false,
            });
        }
        // A file the store already owns keeps its owner, at its new key.
        // The maintenance tag is never the contributor -- what carries
        // forward is the entity that owned it, under the renamed identity.
        let claim = owner_of(&store, source, &destination, &renamed_entities);
        if let Some((key, owners)) = &claim {
            change.resources.push(DesiredResource::new(
                key.clone(),
                owners.clone(),
                ResourceValue::WholeFile,
            )?);
        }
        change.files.push(DesiredFile {
            path: destination,
            body: DesiredBody::Bytes(updated.into_bytes().into()),
            mode: None,
            resource: claim.map(|(key, _)| key),
            renderer: None,
        });
    }

    if change.files.is_empty() {
        return Err(format!(
            "no .java file under src/ mentions `{old}` -- check the spelling, or the type may \
             live outside this module"
        )
        .into());
    }
    // The rename's known blind spot, said out loud. An unmentioned exception
    // is indistinguishable from a bug.
    if in_literals > 0 {
        println!(
            "{in_literals} mention(s) inside string literals were left alone. Check them by hand:"
        );
        println!("  grep -rn '\"[^\"]*{old}' src/");
    }
    println!(
        "{} file(s), {occurrences} occurrence(s), {moved} file rename(s).",
        change.files.len()
    );

    let set = DesiredChangeSet {
        ledger_intent: LedgerIntent {
            generation_before: store.generation(),
            // The renamed identities arrive and the old ones leave, in the
            // same intent. Removing the old is what drops its rows -- a
            // resource whose last owner has gone has lost its last owner --
            // so every row the renamed entity keeps has to be re-declared
            // under the new owner, not only the ones whose path moved.
            entities_after: renamed_entities
                .values()
                .map(
                    |(id, spec, owners)| jails_protocol::plan::DesiredAppliedEntity {
                        id: id.clone(),
                        spec: spec.clone(),
                        owners: owners.clone(),
                    },
                )
                .collect(),
            one_shots_after: Vec::new(),
            resources_after: change.resources.clone(),
            entities_removed: renamed_entities.keys().cloned().collect(),
        },
        ordered: vec![change],
        subject: PlannedSubject::Rename {
            from: from.clone(),
            to: to.clone(),
            force,
        },
    };
    set.validate()?;
    commit_set(
        run,
        set,
        &reads,
        &Asked::new(
            CanonicalMutationRequest::Rename {
                from: from.clone(),
                to: to.clone(),
                force,
            },
            &["rename"],
            vec![old.to_string(), new.to_string()],
            BTreeMap::new(),
            match force {
                true => BTreeSet::from(["force".to_string()]),
                false => BTreeSet::new(),
            },
        ),
    )
}

/// One simple Java type name, in and out.
fn validate(old: &str, new: &str) -> Result<()> {
    for (label, name) in [("old", old), ("new", new)] {
        if name.is_empty() {
            return Err(format!("the {label} name is empty").into());
        }
        if !name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        {
            return Err(format!(
                "`{name}` is not a Java identifier -- the {label} name must start with a letter"
            )
            .into());
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(format!(
                "`{name}` is not a Java identifier. `jails rename` renames one type, not a \
                 package path -- pass the simple name (`Reward`, not `com.example.Reward`)"
            )
            .into());
        }
    }
    if old == new {
        return Err("the old and new names are the same".into());
    }
    Ok(())
}

/// The claim a moved file carries forward, if the store had one.
///
/// Keyed at the *destination*, owned by the *renamed* entity: the row moves
/// with the file, which is what keeps `destroy` able to find it.
fn owner_of(
    store: &ObservedStore,
    source: &ProjectPath,
    destination: &ProjectPath,
    renamed: &BTreeMap<EntityId, (EntityId, EntitySpec, BTreeSet<OwnerId>)>,
) -> Option<(ResourceKey, BTreeSet<ResourceOwner>)> {
    let row = store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.resources.iter())
        .find(|row| row.key == ResourceKey::WholeFile(source.clone()))?;
    let owners: BTreeSet<ResourceOwner> = row
        .owners
        .iter()
        .map(|owner| match owner {
            ResourceOwner::Entity(id) => match renamed.get(id) {
                Some((renamed, _, _)) => ResourceOwner::Entity(renamed.clone()),
                None => owner.clone(),
            },
            other => other.clone(),
        })
        .collect();
    Some((ResourceKey::WholeFile(destination.clone()), owners))
}

/// Where a source ends up, as a project path.
fn destination_of(source: &ProjectPath, old: &str, new: &str) -> Result<ProjectPath> {
    let renamed =
        jails_java::identifier::renamed_path(std::path::Path::new(&source.to_string()), old, new);
    ProjectPath::parse(
        renamed
            .to_str()
            .ok_or_else(|| format!("`{}` is not valid UTF-8", renamed.display()))?,
    )
}

/// Every directory the walk passed through, so its membership is guarded too.
///
/// Derived from the files rather than collected during the walk: a directory
/// with no `.java` file in it contributes nothing this rename could have
/// missed, and declaring it would guard a listing no decision depended on.
fn walked_directories(sources: &[ProjectPath]) -> BTreeSet<ProjectPath> {
    let mut out = BTreeSet::new();
    for source in sources {
        let text = source.to_string();
        if let Some((directory, _)) = text.rsplit_once('/')
            && let Ok(path) = ProjectPath::parse(directory)
        {
            out.insert(path);
        }
    }
    out
}
