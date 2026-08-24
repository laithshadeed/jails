//! `jails adopt`: teach jails where an existing project already keeps things.
//!
//! `plan.md` §12. A project jails did not create keeps its controllers in
//! `controllers`, its repositories in `persistence`, its DTOs in `dto` — and
//! every jails command that reports or writes per layer then gets it wrong.
//! `stats` counted `Web 2` as `Other 4` until a `[layout]` table said
//! otherwise, and that was the whole fix: **this command writes configuration,
//! not new machinery.** Everything downstream already reads
//! `Config::layers()`.
//!
//! ## Three rules, and each is the point
//!
//! **A directory matching nothing is reported, not guessed.** A wrong `[layout]`
//! entry is worse than a missing one: jails would confidently write into the
//! wrong package and `destroy` would look in a third place. So the synonym
//! table is closed, and what it does not recognise is printed for the reader to
//! decide about.
//!
//! **It never writes `[project] capabilities`.** That list is what `jails sync`
//! applies, and inferring it from a directory listing would have `sync` install
//! things nobody asked for into a project jails did not create — the single
//! most destructive thing this command could do.
//!
//! **One layer, one directory.** If two directories both look like the web
//! layer, neither is written and both are reported: a `[layout]` table can only
//! say one thing, and picking the first alphabetically would be a coin toss the
//! reader never saw.

use crate::config;
use crate::generate::find_project_root;
use jails_project::synonyms::{Reading, readings, resolve};
use jails_support::Result;
use std::path::Path;

pub fn adopt(pretend: bool) -> Result<()> {
    let root = find_project_root()?;
    let project = crate::model::Project::inspect(&root)?;
    if project.base().is_empty() {
        return Err(
            "no Java sources found under src/main/java, so there is no package to read.\n       \
             fix: run this from a project with sources, or `jails new <name>` to create one."
                .to_string(),
        );
    }
    let base_dir = root
        .join("src/main/java")
        .join(project.base().replace('.', "/"));
    report(&root, &read_layout(&base_dir), pretend)
}

/// Classify every immediate subdirectory of the base package.
fn read_layout(base_dir: &std::path::Path) -> Vec<Reading> {
    let Ok(entries) = std::fs::read_dir(base_dir) else {
        return Vec::new();
    };
    readings(
        &entries
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
    )
}

fn report(root: &Path, readings: &[Reading], pretend: bool) -> Result<()> {
    let jails_project::synonyms::Resolved {
        writes,
        ambiguous,
        unknown,
    } = resolve(readings);

    for reading in readings {
        if let Reading::Conventional(layer) = reading {
            println!("  keep    {layer:<10} already jails' own name");
        }
    }
    for (layer, dir) in &writes {
        println!("  layout  {layer:<10} = \"{dir}\"");
    }
    for (layer, dirs) in &ambiguous {
        println!(
            "  ask     {layer:<10} matches {} -- a [layout] table can only name one, \
             so none is written",
            dirs.join(", ")
        );
    }
    for name in &unknown {
        println!("  ignore  {name:<10} not a layer jails knows -- left alone");
    }

    if writes.is_empty() {
        println!();
        println!("nothing to adopt: no directory under the base package needs a different name.");
        return Ok(());
    }
    if pretend {
        println!();
        println!("--pretend: nothing was written.");
        return Ok(());
    }
    for (layer, dir) in &writes {
        config::record_layout(root, layer, dir)?;
    }
    println!();
    println!(
        "wrote {} [layout] entr{} to {}. `jails stats` reports against them now.",
        writes.len(),
        if writes.len() == 1 { "y" } else { "ies" },
        root.join(config::FILE).display()
    );
    // Said out loud because it is the rule that makes this command safe to run.
    println!("[project] capabilities was not touched: `jails sync` acts on that list.");
    Ok(())
}
