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
pub fn app_init(project: &Project, manifest: Option<&str>) -> Result<CommitResult> {
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
    commit_set(project, set, &reads, "jails app init")
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
pub fn rename(project: &Project, old: &str, new: &str, force: bool) -> Result<CommitResult> {
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
                force,
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
        subject: PlannedSubject::Rename { from, to, force },
    };
    set.validate()?;
    commit_set(project, set, &reads, "jails rename")
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
pub fn adopt_layout(project: &Project) -> Result<CommitResult> {
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
    // The listing is declared, so a directory appearing under the base package
    // between planning and committing refuses the adoption rather than
    // recording a layout that was already out of date when it landed.
    let reads = capture::capability_reads()?
        .directory(base.clone())
        .file(config.clone());
    let (snapshot, _) = capture::projected(project, &reads)?;

    let names: Vec<String> = snapshot
        .list(&base)?
        .iter()
        .filter_map(|entry| {
            entry
                .to_string()
                .strip_prefix(&format!("{base}/"))
                .map(str::to_string)
        })
        .collect();
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
            "nothing to adopt: no directory under the base package needs a different name."
                .to_string(),
        );
    }

    // Composed against the captured bytes, one splice after another, so the
    // whole `[layout]` table is decided before anything is written. Every
    // other byte of the file -- comments, `[project]`, ordering -- survives,
    // which is the same rule `pom.rs` and `compose.rs` follow for the other
    // files the reader owns.
    let mut text = match snapshot.read(&config)? {
        jails_protocol::snapshot::Captured::Present(file) => String::from_utf8(file.bytes.to_vec())
            .map_err(|_| format!("{config} is not valid UTF-8"))?,
        jails_protocol::snapshot::Captured::Absent => String::new(),
    };
    for (layer, directory) in &resolved.writes {
        text = jails_project::config::with_layout(&text, layer, directory)?;
    }

    let mut change = DesiredChange::maintenance(MaintenanceAttribution::AdoptLayout);
    change.files.push(DesiredFile {
        path: config,
        body: DesiredBody::Bytes(text.into_bytes().into()),
        mode: None,
        resource: None,
        renderer: None,
    });

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
    commit_set(project, set, &reads, "jails adopt")
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
pub fn format(project: &Project) -> Result<CommitResult> {
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
        subject: PlannedSubject::Format { scopes },
    };
    set.validate()?;
    commit_set(project, set, &reads, "jails fmt")
}
