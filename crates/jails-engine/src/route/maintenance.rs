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
        change.files.push(DesiredFile {
            path: destination,
            body: DesiredBody::Bytes(updated.into_bytes().into()),
            mode: None,
            // A rename moves bytes between paths; it does not claim them.
            // Which entity owns a renamed file is the identity transition
            // plan.md §R2.5 reserves for a later format -- recording an owner
            // here would be this maintenance tag claiming authority it does
            // not have.
            resource: None,
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
        subject: PlannedSubject::Rename { from, to, force },
    };
    set.validate()?;
    commit_set(project, set, &reads, "jails rename")
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
