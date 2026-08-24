//! The mutations whose subject is the project itself.
//!
//! §R6.2 groups `app init`, `rename`, `adopt layout`, `adopt legacy` and
//! `format` under one rule: *"plan one typed maintenance subject; never invent
//! a desired entity to carry it."* That rule is the whole module. None of
//! these produces something jails then owns and reconciles -- seeding a
//! manifest hands the file to the reader, a rename moves what is already
//! there, adoption records what was found, and formatting rewrites bytes
//! without changing what any of them mean. Giving any of them an entity would
//! put a row in the store that the next reconciliation would have to decide
//! about, and there is nothing to decide.

use super::*;

/// Seed an application manifest, through the protocol.
///
/// It is one file with fixed bytes, which is exactly why routing it is worth
/// the paragraph: V1 refuses when the path exists by calling `Path::exists`
/// and then writes, and anything may happen between those two statements.
/// Here the path is a **declared read**, so the refusal is a precondition
/// §R4.3 step 2 rechecks under the lock.
pub fn app_init(run: &Run, manifest: Option<&str>) -> Result<Outcome> {
    let project = run.project();
    const SKELETON: &str = "\
# Generic application intent. Add capabilities, then one [[generate]] table per slice.
schema = 1
capabilities = []

# [[generate]]
# kind = \"scaffold\"
# name = \"Note\"
# fields = [\"id:uuid@pk\", \"title:string!\"]
# timestamps = true
";
    let target = ProjectPath::parse(manifest.unwrap_or(".jails/app.toml"))?;

    // Seeding is not regeneration, which is why an existing manifest is a
    // refusal rather than a three-way merge. The bytes below are a skeleton
    // nobody keeps; a manifest that exists is a document somebody has been
    // writing, and merging one into the other produces a file neither of them
    // meant.
    let reads = capture::capability_reads()?.file(target.clone());
    let (snapshot, _) = capture::projected(project, &reads)?;
    if let jails_protocol::snapshot::Captured::Present(_) = snapshot.read(&target)? {
        return Err(format!(
            "application manifest already exists: {target}.\n       fix: edit it, or pass \
             --manifest with a new path."
        ));
    }

    let manifest_target = target.clone();
    let mut change = DesiredChange::maintenance(MaintenanceAttribution::AppInit);
    change.files.push(DesiredFile {
        path: target.clone(),
        body: DesiredBody::Bytes(SKELETON.as_bytes().into()),
        mode: None,
        // No resource, and that is the point. A resource is a claim that jails
        // owns these bytes and will decide about them again; this file belongs
        // to the reader the moment it lands.
        resource: None,
        renderer: None,
    });

    let observed = observed(project)?;
    let set = DesiredChangeSet {
        ledger_intent: LedgerIntent {
            // Nothing enters the store. `app init` seeds a file and stops --
            // what the manifest goes on to declare is `app apply`'s business,
            // and recording a row here would be a claim on a document jails
            // does not write again.
            generation_before: observed.generation(),
            entities_after: Vec::new(),
            one_shots_after: Vec::new(),
            resources_after: Vec::new(),
            entities_removed: Vec::new(),
            legacy_after: Vec::new(),
        },
        ordered: vec![change],
        subject: PlannedSubject::AppInit { target },
    };
    set.validate()?;
    commit_set(
        run,
        set,
        &reads,
        &Asked::plain(
            CanonicalMutationRequest::AppInit {
                target: manifest_target,
            },
            &["app", "init"],
            &[],
        ),
    )
}

/// Rename a Java type across the project, as one transition.
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
                return Err(format!(
                    "{destination} already exists -- rename or delete it first"
                ));
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
        ));
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
            legacy_after: Vec::new(),
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
            return Err(format!("the {label} name is empty"));
        }
        if !name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        {
            return Err(format!(
                "`{name}` is not a Java identifier -- the {label} name must start with a letter"
            ));
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(format!(
                "`{name}` is not a Java identifier. `jails rename` renames one type, not a \
                 package path -- pass the simple name (`Reward`, not `com.example.Reward`)"
            ));
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

/// Record what an existing project already calls its layers, as one commit.
///
/// V1 writes `jails.toml` once per adopted layer, so a project with four
/// renamed directories is four separate rewrites of one file. Here the splices
/// are composed against the captured text and land as one operation -- and the
/// captured text is what makes the composition sound, since splicing against a
/// re-read file is how the second edit comes to be written over the first.
///
/// It is `resource: None`, deliberately. `jails.toml` is a file the reader
/// owns and edits, and `[layout]` is configuration jails reads rather than a
/// thing jails owns and would later reconcile. Claiming it would make a
/// removal somewhere else able to take it away.
///
/// **`[project] capabilities` cannot be touched from here**, and that is not a
/// promise -- it is the type. What the classification produces is
/// `(layer, directory)` pairs and nothing else, so there is no path by which a
/// directory listing could reach the list `jails sync` acts on.
pub fn adopt_layout(run: &Run) -> Result<Outcome> {
    let project = run.project();
    if project.base().is_empty() {
        return Err(
            "no Java sources found under src/main/java, so there is no package to read.\n       \
             fix: run this from a project with sources, or `jails new <name>` to create one."
                .to_string(),
        );
    }
    let base = ProjectPath::parse(&format!(
        "src/main/java/{}",
        project.base().replace('.', "/")
    ))?;
    let config = ProjectPath::parse(jails_project::config::FILE)?;
    // A subpackage is found by the Java in it, not by listing the base
    // directory, and the difference is not pedantry. A listing returns names
    // without kinds, so a *file* called `controllers` would be adopted as the
    // web layer's package; and a directory holding no Java is not a package
    // anybody can be in, so recording a layout for it would point every later
    // command at an empty tree. A `.java` file's parent is neither.
    //
    // The walk itself is unguarded -- something has to look first -- but every
    // file it finds is declared, so §R4.3 rechecks them under the lock and a
    // source appearing in a new subpackage mid-transition refuses rather than
    // being silently left out of the layout.
    let mut reads = capture::capability_reads()?.file(config.clone());
    let mut names = BTreeSet::new();
    let prefix = format!("{base}/");
    for absolute in jails_java::java::source_files(&project.root().join(base.to_string())) {
        let relative = super::relative_path(project, &absolute)?;
        reads = reads.file(relative.clone());
        if let Some(rest) = relative.to_string().strip_prefix(&prefix)
            && let Some((name, _)) = rest.split_once('/')
        {
            names.insert(name.to_string());
        }
    }
    let names: Vec<String> = names.into_iter().collect();
    // Captured rather than read: the layout edit rewrites `jails.toml`, so
    // its preimage has to be under the recheck even though nothing here looks
    // at the bytes -- the splice happens in the projection.
    capture::projected(project, &reads)?;
    let readings = jails_project::synonyms::readings(&names);
    let resolved = jails_project::synonyms::resolve(&readings);

    for reading in &readings {
        if let jails_project::synonyms::Reading::Conventional(layer) = reading {
            println!("  keep    {layer:<10} already jails' own name");
        }
    }
    for (layer, dir) in &resolved.writes {
        println!("  layout  {layer:<10} = \"{dir}\"");
    }
    for (layer, dirs) in &resolved.ambiguous {
        println!(
            "  ask     {layer:<10} matches {} -- a [layout] table can only name one, so none \
             is written",
            dirs.join(", ")
        );
    }
    for name in &resolved.unknown {
        println!("  ignore  {name:<10} not a layer jails knows -- left alone");
    }
    if resolved.writes.is_empty() {
        return Err(
            "nothing to adopt: no package under the base package needs a different name."
                .to_string(),
        );
    }
    // Said out loud because it is the rule that makes this command safe to
    // run at all, and true in both modes: nothing below can reach that list.
    println!("[project] capabilities is not touched: `jails sync` acts on that list.");

    // One keyed edit per layer, not one rewrite of the file. `jails.toml` has
    // more than one contributor -- `[project] capabilities` is a set of owned
    // resources spliced by `add` -- and a whole-file body would be a claim to
    // decide every byte of a file this change speaks for only one table of.
    // The splices compose in the projection, in order, so the reader's
    // comments and capability list survive untouched.
    let mut change = DesiredChange::maintenance(MaintenanceAttribution::AdoptLayout);
    for (layer, directory) in &resolved.writes {
        let named = jails_spec::spec::layout::Layer::by_package(layer).ok_or_else(|| {
            format!("`{layer}` is not a layer jails knows, which the synonym table should not                      have been able to produce")
        })?;
        change.edits.push(SemanticEdit::HumanConfigLayout {
            layer: named,
            directory: (*directory).to_string(),
        });
    }

    let observed = observed(project)?;
    let set = DesiredChangeSet {
        ledger_intent: LedgerIntent {
            generation_before: observed.generation(),
            entities_after: Vec::new(),
            one_shots_after: Vec::new(),
            resources_after: Vec::new(),
            entities_removed: Vec::new(),
            legacy_after: Vec::new(),
        },
        ordered: vec![change],
        subject: PlannedSubject::AdoptLayout,
    };
    set.validate()?;
    commit_set(
        run,
        set,
        &reads,
        &Asked::plain(CanonicalMutationRequest::AdoptLayout, &["adopt"], &[]),
    )
}

/// Reformat the project's sources, without letting the formatter near them.
///
/// §R6.4's row is explicit: *"scratch-format and commit exact changed sources.
/// Do not let Maven/Spotless mutate the live source tree directly."* V1 runs
/// `mvn spotless:apply` against the real project, so a formatter that fails
/// halfway leaves some files rewritten and some not, with nothing to say which
/// -- and a formatter that decides to rewrite something outside `src/` has
/// already done it by the time anybody notices.
///
/// Here Spotless runs against a scratch tree synthesised from the projection,
/// its output is diffed against what went in, and what it changed enters the
/// plan as ordinary file operations. Two things fall out that V1 cannot have:
/// anything the formatter touches outside its declared `mutable_scopes` is a
/// refusal rather than a fait accompli, and a run that changes nothing is a
/// transition with no operations rather than a rewrite of every file to
/// identical bytes.
///
/// `resource: None` throughout. Formatting rewrites bytes without changing
/// what any of them mean, so nothing here claims ownership it did not have --
/// a generated file stays its entity's, and a hand-written one stays the
/// reader's.
pub fn format(run: &Run) -> Result<Outcome> {
    let project = run.project();
    let scope = ProjectPath::parse("src")?;
    let mut reads = capture::capability_reads()?;
    let mut sources = Vec::new();
    for absolute in jails_java::java::source_files(&project.root().join("src")) {
        let relative = super::relative_path(project, &absolute)?;
        reads = reads.file(relative.clone());
        sources.push(relative);
    }
    if sources.is_empty() {
        return Err(
            "no .java file under src/ to format.\n       fix: run this from a project with \
             sources."
                .to_string(),
        );
    }
    // The pom is read because the formatter is *invoked through it* -- the
    // Spotless plugin's configuration is what decides the result, so a plan
    // made against one pom must not be committed against another.
    reads = reads.file(ProjectPath::parse("pom.xml")?);
    let (snapshot, _) = capture::projected(project, &reads)?;

    // Synthesised from the projection, not copied from disk: a formatter must
    // see the bytes this transaction will write, or the plan carries a diff
    // against something nobody will commit.
    let mut laid_out = Vec::new();
    for path in sources
        .iter()
        .chain(std::iter::once(&ProjectPath::parse("pom.xml")?))
    {
        if let jails_protocol::snapshot::Captured::Present(file) = snapshot.read(path)? {
            laid_out.push(jails_prepare::sandbox::SandboxFile {
                path: path.clone(),
                bytes: file.bytes.to_vec(),
                mode: file.mode,
            });
        }
    }
    // Plain Maven, never the daemon, and not because the daemon is slow. mvnd
    // keeps a registry under the Maven user home and reuses a long-lived
    // process across invocations; a one-shot run in a throwaway tree that is
    // deleted the moment it finishes is the one case where that buys nothing
    // and can leave a daemon holding a directory that no longer exists.
    let program = jails_project::maven::plain(project);
    let identity = jails_prepare::tool::ToolIdentityFingerprint {
        key: jails_prepare::tool::ToolInvocationKey {
            tool: jails_protocol::identity::ToolId::parse("spotless")?,
            // A project-wide formatter is not about one file.
            subject: None,
        },
        executable_sha256: ObjectId::from_bytes(jails_support::codec::sha256(
            program.to_string_lossy().as_bytes(),
        )),
        version_stdout_sha256: ObjectId::from_bytes(jails_support::codec::sha256(b"spotless")),
        runner_schema: 1,
        timeout_ms: 300_000,
        // The whole point of the fingerprint: a formatter that writes outside
        // this is refused, and widening it changes the identity, so a policy
        // cannot be loosened without the change being visible.
        mutable_scopes: BTreeSet::from([scope]),
        offline_inputs: Vec::new(),
    };

    let sandbox = jails_prepare::sandbox::Sandbox::lay_out(laid_out)?;
    let (_, diff) = sandbox.run(
        &identity,
        program,
        vec!["-q".to_string(), "spotless:apply".to_string()],
        // Minimal, not empty. Maven's own launcher is a shell script that
        // shells out to `uname`, `dirname` and `expr`, so a run with no `PATH`
        // dies with "command not found" before Maven starts -- and without
        // `HOME` it cannot find the local repository that holds the plugin.
        // Everything else is deliberately absent: the fewer keys reach the
        // tool, the fewer ways one machine's environment can change what it
        // produces.
        ["PATH", "HOME", "JAVA_HOME"]
            .into_iter()
            .filter_map(|key| {
                std::env::var(key)
                    .ok()
                    .map(|value| (key.to_string(), value))
            })
            .collect(),
    )?;
    let mut change = DesiredChange::maintenance(MaintenanceAttribution::Format);
    let mut scopes = BTreeSet::new();
    for (path, file) in &diff.changed {
        scopes.insert(path.clone());
        change.files.push(DesiredFile {
            path: path.clone(),
            body: DesiredBody::Bytes(file.bytes.clone().into()),
            mode: Some(file.mode),
            resource: None,
            renderer: None,
        });
    }
    // A formatter that deletes a source is not formatting. Refusing here
    // rather than committing the deletion is the difference between a policy
    // and a description of what happened.
    if !diff.removed.is_empty() {
        sandbox.close()?;
        return Err(format!(
            "the formatter removed {} file(s), which formatting does not do.\n       fix: this \
             is a formatter or configuration problem; nothing was written.",
            diff.removed.len()
        ));
    }
    sandbox.close()?;

    if change.files.is_empty() {
        println!("already formatted -- nothing to change.");
    } else {
        println!("{} file(s) reformatted.", change.files.len());
    }

    let observed = observed(project)?;
    let set = DesiredChangeSet {
        ledger_intent: LedgerIntent {
            generation_before: observed.generation(),
            entities_after: Vec::new(),
            one_shots_after: Vec::new(),
            resources_after: Vec::new(),
            entities_removed: Vec::new(),
            legacy_after: Vec::new(),
        },
        ordered: vec![change],
        subject: PlannedSubject::Format {
            scopes: scopes.clone(),
        },
    };
    set.validate()?;
    commit_set(
        run,
        set,
        &reads,
        &Asked::plain(CanonicalMutationRequest::Format { scopes }, &["fmt"], &[]),
    )
}

/// Claim one schema-1 row as a named owner.
///
/// §R2.5: *"the only route from unknowable legacy manifest origin to a named
/// owner, and every row is handled explicitly rather than heuristically
/// joined."* Migration deliberately claims nothing -- the old format never
/// recorded who asked for a row -- so this is how a reader says which row is
/// which, one at a time and by a stable key rather than by "the first thing
/// that matches".
///
/// The safety rule is the whole feature. jails re-renders the intent and
/// requires the files on disk to be **byte-identical** to what it would write
/// now. Only then does the row become an owned entity with a truthful
/// renderer stamp. That is what stops adoption claiming that an arbitrary
/// legacy byte was renderer-produced -- a claim every later three-way merge
/// would measure from, so a wrong one silently corrupts every future update
/// of that file.
///
/// A mismatch refuses and says which file differs -- unless the caller has
/// said `--replace --force`, which is the destructive choice made explicitly.
/// `replace` then installs the freshly rendered candidate over whatever is
/// there, with the current bytes as guarded preimages, so the row still ends
/// up with a base jails really produced. §R6.4 makes `--force` mandatory
/// alongside it and refuses either flag alone: adopting is a repair, and a
/// repair that silently overwrote an edit nobody looked at would be the same
/// corruption by a different route.
pub fn adopt_legacy(
    run: &Run,
    key: &str,
    kind: ArtifactKind,
    name: &str,
    replace: bool,
) -> Result<Outcome> {
    let project = run.project();
    let wanted = jails_protocol::envelope::LegacyKey::parse_label(key)?;
    let store = observed(project)?;

    let mut found = None;
    for row in store.ledger.iter().flat_map(|ledger| ledger.legacy.iter()) {
        if row.legacy_key(jails_protocol::envelope::LegacySourceKind::Schema1Applied)? == wanted {
            found = Some(row.clone());
            break;
        }
    }
    let row = found.ok_or_else(|| {
        format!(
            "no legacy row has the key `{key}`.\n       fix: `jails doctor` lists every key this \
             project has. A key names one exact decoded row, never the first one that looks \
             similar."
        )
    })?;

    // The intent the caller says this row is. Checked against the row rather
    // than trusted: adopting `record Reward` onto a row that says `service
    // Reward` would put an owner on files that recipe never wrote.
    if row.recipe != label(kind) || row.name != name {
        return Err(format!(
            "`{key}` is `{} {}`, not `{} {name}`.\n       fix: pass the intent the row actually \
             names. Adopting one row under another's identity would put an owner on files that \
             recipe never wrote.",
            row.recipe,
            row.name,
            label(kind)
        ));
    }

    let fields: Vec<String> = row.fields.clone();
    let package = match row.package.is_empty() {
        true => None,
        false => Some(row.package.as_str()),
    };
    let change = with_test_support(
        project,
        jails_generate::generate::plan_recipe(
            project,
            kind,
            name,
            &fields,
            package,
            &row.indexes,
            (!row.on.is_empty()).then_some(row.on.as_str()),
            (!row.yields.is_empty()).then_some(row.yields.as_str()),
        )?,
    );

    // Every file the re-render produces must already be there, byte for byte.
    let id = super::intent(
        project,
        kind,
        name,
        package,
        &fields,
        &row.indexes,
        (!row.on.is_empty()).then_some(row.on.as_str()),
        (!row.yields.is_empty()).then_some(row.yields.as_str()),
    )?;
    let spec = super::spec(
        project,
        kind,
        &fields,
        &row.indexes,
        (!row.on.is_empty()).then_some(row.on.as_str()),
        (!row.yields.is_empty()).then_some(row.yields.as_str()),
    )?;
    let owner = ResourceOwner::Entity(EntityId::Intent(id.clone()));
    let mut desired = desire::contribution(&owner, &change, project)?;

    // Compared against the *projection*, not against the recipe's artifacts
    // and not against the contribution either. Import order is normalised and
    // blank lines are tidied on the way to disk -- CLAUDE.md keeps both out of
    // the templates deliberately -- so a recipe's output is not what
    // `generate` writes. Comparing anything earlier refuses every row jails
    // itself produced, which is the only kind adoption is for.
    let mut reads = capture::capability_reads()?;
    for file in &desired.files {
        reads = reads.file(file.path.clone());
    }
    //
    // `--replace` is the one case that skips it, and skips only this: the
    // transition it commits is the same one, so the differing bytes leave as
    // guarded preimages rather than as an unrecorded overwrite.
    if !replace {
        let (snapshot, mut projection) = capture::projected(project, &reads)?;
        projection.advance(&desired)?;
        let mut differ = Vec::new();
        for (path, entry) in projection.overlay() {
            let jails_project::projection::ProjectedEntry::File(projected) = entry else {
                continue;
            };
            match snapshot.read(path)? {
                jails_protocol::snapshot::Captured::Present(live)
                    if live.bytes == projected.bytes => {}
                jails_protocol::snapshot::Captured::Present(_) => {
                    differ.push(format!("         {path} differs"))
                }
                jails_protocol::snapshot::Captured::Absent => {
                    differ.push(format!("         {path} is missing"))
                }
            }
        }
        if !differ.is_empty() {
            return Err(format!(
                "`{key}` cannot be adopted as it stands: what jails would render now is not what \
                 is on disk.\n{}\n       fix: adoption records the rendered bytes as this row's \
                 base, and every later update measures from it -- so claiming bytes jails did \
                 not produce would corrupt every future merge of these files. Reconcile them by \
                 hand first, or overwrite them with\n         jails adopt --legacy-key {key} \
                 --intent {}:{name} --replace --force",
                differ.join("\n"),
                label(kind)
            ));
        }
    }

    let entity = DesiredEntity {
        id: EntityId::Intent(id.clone()),
        spec: EntitySpec::Intent(spec),
        // The row said nothing about who asked for it, and this command is the
        // reader saying so. `DirectCli` is the truthful answer: they asked,
        // just now, through this command.
        owners: BTreeSet::from([OwnerId::DirectCli]),
    };
    provenance::stamp_files(
        &mut desired,
        project,
        RendererId::Recipe(kind),
        Some(RenderedSubjectContext::Entity {
            id: entity.id.clone(),
            spec: entity.spec.clone(),
        }),
    )?;
    // What `--replace` claims: exactly the files this row's re-render produces,
    // and nothing else. §R5.3 refuses to write over a file jails never wrote,
    // and this is the command that has been told the decision was made -- said
    // about these paths rather than about the transition, so an unrelated file
    // the same commit happens to touch keeps the protection.
    let claimed: BTreeSet<jails_protocol::identity::ProjectPath> = match replace {
        true => desired.files.iter().map(|file| file.path.clone()).collect(),
        false => BTreeSet::new(),
    };
    let reads = declaration(project, &change, &desired)?;
    let request = Request {
        scope: ReconcileScope::DirectEntity(EntityId::Intent(id)),
        declared: BTreeMap::from([(entity.id.clone(), entity)]),
        changes: vec![desired],
    };
    commit(
        &run.claiming(claimed),
        request,
        &reads,
        &Asked::new(
            CanonicalMutationRequest::AdoptLegacy {
                legacy_key: wanted,
                intent: super::intent(
                    project,
                    kind,
                    name,
                    package,
                    &fields,
                    &row.indexes,
                    (!row.on.is_empty()).then_some(row.on.as_str()),
                    (!row.yields.is_empty()).then_some(row.yields.as_str()),
                )?,
                // §R3's validation row: `replace` implies `force`, so the
                // two move together and neither is reachable alone.
                replace,
                force: replace,
            },
            &["adopt"],
            vec![label(kind).to_string(), name.to_string()],
            BTreeMap::from([("legacy-key".to_string(), vec![key.to_string()])]),
            BTreeSet::new(),
        ),
    )
}
