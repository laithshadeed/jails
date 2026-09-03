//! `jails rename <Old> <New>`: a type and everything that names it, textually.
//!
//! **Reach for the language server first.** Neovim's `grn` (jdt.ls rename)
//! understands scope, so it will not touch an unrelated `Reward` in another
//! package, and where it works it is strictly better than this. What this
//! exists for is the case jdt.ls cannot serve: the server is not attached, the
//! project does not currently compile -- jdt.ls degrades badly there, and a
//! rename is often exactly how you are trying to fix it -- or the rename has
//! to reach a file no buffer has opened.
//!
//! It is textual, and two properties are what keep textual honest.
//! [`jails_support::identifier`] holds both: `Reward` never matches inside
//! `RewardHistory`, so the classic sed disaster cannot happen; and string
//! literals are left alone, because a literal is data and silently rewriting
//! `"Reward not found"` is a change nobody asked for. A literal that genuinely
//! names the class -- a `Class.forName` argument -- is therefore missed, which
//! is the safe direction and is *reported* rather than hidden.
//!
//! **It touches the reader's own sources and nothing else.** A managed file
//! is a projection of the model, so a textual edit there would be undone by the
//! next compilation and is not offered; a declared entity is renamed with
//! `jails rename resource`, which moves the model, the table and the managed
//! tree together. Renaming a declared type textually is refused by name for
//! exactly that reason -- it would leave the Java saying one thing and the
//! model another, with both oracles reporting health.

use crate::Invocation;
use jails_support::{Failure, Result, apply};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(crate) fn run(old: &str, new: &str, force: bool, invocation: Invocation) -> Result<()> {
    validate(old, new)?;
    let root = crate::model_command::root()?;
    refuse_declared(&root, old, new)?;

    // **The reader's files only.** Managed sources sit beside theirs under
    // `src/`, and the lock says which is which; a managed file naming the
    // old type is renamed by the model, not by this.
    let managed = jails_project::capture::managed_paths(&root)
        .map_err(|error| Failure::Told(format!("could not read the compiler lock: {error}")))?;
    let mut rewrites: BTreeMap<PathBuf, (String, PathBuf)> = BTreeMap::new();
    let mut occurrences = 0_usize;
    let mut in_literals = 0_usize;
    for absolute in jails_project::java::source_files(&root.join("src")) {
        let relative = absolute.strip_prefix(&root).ok().and_then(|relative| {
            jails_contracts::ProjectPath::parse(relative.to_string_lossy().replace('\\', "/")).ok()
        });
        if relative.is_some_and(|relative| managed.contains(&relative)) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&absolute) else {
            continue;
        };
        let (updated, hits) = jails_support::identifier::replace_identifier(&text, old, new);
        let destination = jails_support::identifier::renamed_path(&absolute, old, new);
        if hits == 0 && destination == absolute {
            continue;
        }
        occurrences += hits;
        in_literals += jails_support::identifier::literal_mentions(&text, old);
        if destination != absolute && destination.exists() {
            return Err(Failure::Told(format!(
                "`{}` already exists, so this rename would overwrite it.\n       fix: rename or delete that file first",
                display(&root, &destination)
            )));
        }
        rewrites.insert(absolute, (updated, destination));
    }

    if rewrites.is_empty() {
        return Err(Failure::Told(format!(
            "nothing under `src/` names `{old}`.\n       fix: check the spelling, or pass the simple type name"
        )));
    }
    let moved = rewrites
        .iter()
        .filter(|(source, (_, destination))| *source != destination)
        .count();

    for (source, (_, destination)) in &rewrites {
        if source == destination {
            println!("  edit    {}", display(&root, source));
        } else {
            println!(
                "  move    {} -> {}",
                display(&root, source),
                display(&root, destination)
            );
        }
    }
    println!(
        "{occurrences} identifier{} in {} file{}, {moved} moved",
        plural(occurrences),
        rewrites.len(),
        plural(rewrites.len())
    );
    // Reported rather than rewritten, and reported even at zero: "no literal
    // mentions `Order`" is the answer that lets a reader stop looking.
    println!(
        "{in_literals} mention{} left in string literals",
        plural(in_literals)
    );

    if invocation.pretend {
        println!("nothing was written.");
        return Ok(());
    }
    if !force {
        return Err(Failure::Told(
            "a textual rename cannot be undone by jails, and jdt.ls understands scope where this does not.\n       fix: re-run with `--force` once the list above is what you meant".to_string(),
        ));
    }

    // Every write lands before any removal, so an interrupted rename leaves
    // the file readable under both names rather than under neither.
    for (updated, destination) in rewrites.values() {
        apply::put_one_shot(destination, updated)?;
    }
    for (source, (_, destination)) in &rewrites {
        if source != destination {
            apply::remove_one_shot(source)?;
        }
    }
    println!("renamed {old} to {new}.");
    Ok(())
}

/// Refuse a type the model declares, naming the command that moves both halves.
///
/// The textual rename carries the Java and nothing else. On a declared entity
/// that is not a partial success but a divergence: the next compilation renders
/// the old name back, and on a stored one the adapter would read
/// `select ... from readers` while the schema history still creates `members`.
fn refuse_declared(root: &Path, old: &str, new: &str) -> Result<()> {
    if !crate::model_command::owns(root) {
        return Ok(());
    }
    let manifest = crate::model_command::resolve_manifest(None)?;
    // A model that does not currently parse is not a reason to refuse: this
    // command is one of the ways somebody fixes a project that is broken.
    let Ok((_, model)) = crate::model_command::load_model(root, &manifest, crate::Output::Human)
    else {
        return Ok(());
    };
    let Some(entity) = model
        .entities
        .values()
        .find(|entity| entity.active && entity.names.java_type == old)
    else {
        return Ok(());
    };
    // **Name the table when there is one.** `jails destroy` says "backed by
    // table `members`" for the same situation, and the concrete noun is what
    // tells the reader what the textual rename would leave behind: an adapter
    // reading `select ... from readers` over a schema history that still
    // creates `members`.
    let backing = match model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db")
        && entity.facets.contains(&jails_model::Facet::Repository)
    {
        true => format!(" and is backed by table `{}`", entity.names.sql_table),
        false => String::new(),
    };
    Err(Failure::Told(format!(
        "`{old}` is declared in this project's application model{backing}, and this rename carries only the Java.\n       fix: move the declaration, the table and the managed tree together with `jails rename resource {old} {new} --strategy preserve-table`"
    )))
}

/// One simple Java type name each side, refused before anything is read.
///
/// A qualified `com.example.Reward` would otherwise be read as the simple name
/// and matched textually against every source, which is not what the caller
/// asked for at all.
fn validate(old: &str, new: &str) -> Result<()> {
    for (label, name) in [("old", old), ("new", new)] {
        if name.is_empty() {
            return Err(Failure::Told(format!(
                "the {label} name is empty.\n       fix: pass one simple Java type name"
            )));
        }
        if !name
            .chars()
            .next()
            .is_some_and(|first| first.is_alphabetic() || first == '_')
        {
            return Err(Failure::Told(format!(
                "`{name}` is not a Java identifier -- the {label} name must start with a letter.\n       fix: pass one simple Java type name"
            )));
        }
        if !name
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_')
        {
            return Err(Failure::Told(format!(
                "`{name}` is not a Java identifier. `jails rename` renames one type, not a package path.\n       fix: pass the simple name (`Reward`, not `com.example.Reward`)"
            )));
        }
    }
    if old == new {
        return Err(Failure::Told(
            "the old and new names are the same.\n       fix: choose a distinct target name"
                .to_string(),
        ));
    }
    Ok(())
}

fn display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}
