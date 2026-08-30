//! `jails adopt layout` — record what a foreign project already calls its
//! layers.

use super::*;

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
        return Err(jails_support::Failure::Told(
            "no Java sources found under src/main/java, so there is no package to read.\n       \
             fix: run this from a project with sources, or `jails new <name>` to create one."
                .to_string(),
        ));
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
            // **The whole package-relative directory, not its first segment.**
            // `split_once` took the first, so a class in `infra/jdbc` was
            // recorded as `adapters = "infra"` -- a package holding no Java at
            // all, only the subpackage the class is really in. That is exactly
            // what the comment above says this walk exists to prevent: every
            // later command would have been pointed at an empty tree, and
            // `Config::layers()` honours a nested layout perfectly well, so
            // nothing downstream would have reported it.
            //
            // `rsplit_once` also keeps the file-at-the-base case out: a
            // `.java` directly under the base package has no `/` and is in no
            // subpackage.
            && let Some((package, _)) = rest.rsplit_once('/')
        {
            names.insert(package.replace('/', "."));
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
        return Err(jails_support::Failure::Told(
            "nothing to adopt: no package under the base package needs a different name."
                .to_string(),
        ));
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
