//! `jails adopt`: record what a foreign project already calls its layers.
//!
//! **Configuration, not machinery**, and that is why it is here rather than in
//! a transition engine. What it produces is `(layer, directory)` pairs and
//! nothing else, written into one `[layout]` table of `jails.toml`; everything
//! downstream already reads `Config::layers()`, so there is no code path to
//! change and nothing for a later command to reconcile.
//!
//! It is also one of the two commands that run *before* a project has a model
//! -- `jails model init` reads the layout this writes -- so it does not
//! initialise one, and a reader who runs it on a canonical project simply
//! updates a table their compiler already honours.
//!
//! Three rules, each load-bearing:
//!
//! - **An unrecognised directory is reported, not guessed.** A synonym table
//!   answers or it does not.
//! - **Two candidates for one layer writes neither**, because a `[layout]`
//!   table can only name one and picking would be silent.
//! - **`[project] capabilities` is never touched.** That is the list `jails
//!   sync` acts on, and it is unreachable from here by construction: nothing
//!   in this module produces a capability name.

use crate::Invocation;
use jails_project::model::Project;
use jails_project::synonyms::Reading;
use jails_support::{Failure, Result};
use std::collections::BTreeSet;

pub(crate) fn layout(invocation: Invocation) -> Result<()> {
    let project = Project::discover()?;
    if project.base().is_empty() {
        return Err(Failure::Told(
            "no Java sources found under src/main/java, so there is no package to read.\n       \
             fix: run this from a project with sources, or `jails new <name>` to create one."
                .to_string(),
        ));
    }
    let names = subpackages(&project);
    let readings = jails_project::synonyms::readings(&names);
    let resolved = jails_project::synonyms::resolve(&readings);

    for reading in &readings {
        if let Reading::Conventional(layer) = reading {
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
        return Err(Failure::Told(
            "nothing to adopt: no package under the base package needs a different name."
                .to_string(),
        ));
    }
    println!("[project] capabilities is not touched: `jails sync` acts on that list.");
    if invocation.pretend {
        println!("--pretend: nothing was written.");
        return Ok(());
    }

    // **Composed against one text and written once.** Splicing each layer
    // against a re-read file is how the second edit comes to be written over
    // the first, and `jails.toml` has more than one contributor: the
    // capability list `add` maintains lives in the same file.
    let path = project.root().join(jails_project::config::FILE);
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    for (layer, directory) in &resolved.writes {
        text = jails_project::config::with_layout(&text, layer, directory)?;
    }
    jails_support::apply::put_one_shot(&path, text)?;
    println!("wrote {}", jails_project::config::FILE);
    Ok(())
}

/// The subpackages of the base package that hold Java.
///
/// **Found by the Java in them, not by listing a directory**, and the
/// difference is not pedantry: a listing returns names without kinds, so a
/// *file* called `controllers` would be adopted as the web layer's package,
/// and a directory holding no Java is not a package anybody can be in.
///
/// The whole package-relative path, not its first segment: a class in
/// `infra/jdbc` recorded as `adapters = "infra"` names a package holding no
/// Java at all, and every later command would be pointed at an empty tree.
fn subpackages(project: &Project) -> Vec<String> {
    let base = format!("src/main/java/{}", project.base().replace('.', "/"));
    let root = project.root().join(&base);
    let mut names = BTreeSet::new();
    for absolute in jails_project::java::source_files(&root) {
        let Ok(relative) = absolute.strip_prefix(&root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        // A `.java` directly under the base package has no `/` and is in no
        // subpackage.
        if let Some((package, _)) = relative.rsplit_once('/') {
            names.insert(package.replace('/', "."));
        }
    }
    names.into_iter().collect()
}
