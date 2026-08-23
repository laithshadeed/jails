//! `jails destroy`: taking back exactly what `generate` wrote.
//!
//! Two sources, in this order. The **record** (`.jails/ledger.toml`) is the
//! truth where there is one -- `plan.md` §11.2: a recomputed path gives you
//! today's answer for yesterday's file, so a project upgraded across a layout
//! change would have `destroy` name paths that were never written and strand
//! the ones that were.
//!
//! **Recomputation** is what a project predating `.jails/` has, and it is the
//! whole of `abstract.md` rungs 4-5. `destroy` is given a kind and a name and
//! never the arguments, and most generators refuse without them -- but those
//! arguments decide the file *contents*, not the file *names*. So this offers
//! each generator a short list of argument shapes and keeps the paths of the
//! first it accepts, in place of the 672-line `KIND_FILES` table that used to
//! transcribe by hand what `artifacts_for` already computes.
//!
//! A kind no shape satisfies yields nothing, and `destroy` says so naming the
//! command that would record it. Under-naming is the safe failure: the reader
//! keeps a file and deletes it by hand, where over-naming loses work.

use crate::Result;
use crate::model::Project;
use clap::ValueEnum;
use std::path::{Path, PathBuf};

use super::*;

pub fn destroy(
    kind: ArtifactKind,
    name: &str,
    force: bool,
    package: Option<&str>,
    pretend: bool,
) -> Result<()> {
    let project = Project::discover()?;
    let root = project.root().to_path_buf();
    let place = |default: &str| project.package_named(default, package);
    // `cases` is addressed by the markdown path it was generated from, which
    // must not be run through capitalize like a class name.
    let raw_name = name.to_string();
    let name = strip_redundant_suffix(kind, &capitalize(name));
    let kind_key = kind
        .to_possible_value()
        .expect("every ArtifactKind has a clap value")
        .get_name()
        .to_string();
    let recorded = crate::generated_files::paths(&root, &kind_key, &name, package)?;

    let paths: Vec<PathBuf> = match kind {
        // The implementations are read back off disk rather than rebuilt from
        // a variant list destroy is not given. That also makes it a real
        // inverse of what is *there*: an implementation added by hand after
        // the generate call is still one of this strategy's classes, and
        // leaving it behind implementing a deleted interface would stop the
        // project compiling.
        ArtifactKind::Strategy => {
            let domain = place(layout::DOMAIN);
            let mut paths = vec![main_dir(&root, &domain).join(format!("{name}.java"))];
            for path in crate::java::source_files(&main_dir(&root, &domain)) {
                let Ok(source) = fs::read_to_string(&path) else {
                    continue;
                };
                let Some(info) = crate::java::type_info(&source) else {
                    continue;
                };
                if info.name == name || !info.supertypes.iter().any(|s| s == &name) {
                    continue;
                }
                paths.push(path);
                paths.push(test_dir(&root, &domain).join(format!("{}Test.java", info.name)));
            }
            paths
        }
        // `cases` derives its class from a markdown file's name, so destroy
        // takes that same path and resolves it the same way generate did.
        ArtifactKind::Cases => {
            vec![
                test_dir(&root, &place(""))
                    .join(format!("{}.java", cases_class_name(Path::new(&raw_name))?)),
            ]
        }
        ArtifactKind::Migration | ArtifactKind::Association | ArtifactKind::Field => {
            return Err(
                "migrations, associations, and field changes are forward-only; create a new migration instead of destroying one"
                    .to_string(),
            );
        }
        // Everything else is the table: one row per file, `{name}`
        // substituted, `--package` honoured wherever the generator honours
        // it.
        _ if recorded.is_some() => recorded.clone().unwrap_or_default(),
        // Nothing recorded: this artifact predates `.jails/`, so ask the
        // generator what it *would* write today and take the paths off that.
        //
        // Recomputation is second, not first, on purpose (`plan.md` §11.2): a
        // recomputed path gives you today's answer for yesterday's file, so a
        // project upgraded across a layout change would have `destroy` name
        // paths that were never written and strand the ones that were. The
        // record is the truth where there is one; this is what there is when
        // there is not.
        //
        // Fields are gone with the record, and most kinds do not need them --
        // a path is `{name}Controller.java` whichever fields it holds. The
        // ones that *do* need them are the kinds that read a record off disk,
        // which is still there. A generator that refuses without them yields
        // no paths rather than a guess: "nothing to destroy" is recoverable by
        // hand, and deleting the wrong file is not.
        // Nothing recorded: this artifact predates `.jails/`, so ask the
        // generator what it *would* write today and take the paths off that.
        //
        // Recomputation is second, not first, on purpose (`plan.md` §11.2): a
        // recomputed path gives you today's answer for yesterday's file, so a
        // project upgraded across a layout change would have `destroy` name
        // paths that were never written and strand the ones that were. The
        // record is the truth where there is one; this is what there is when
        // there is not.
        _ => recomputed_paths(&project, kind, &name, package),
    };

    let existing: Vec<&PathBuf> = paths.iter().filter(|p| p.exists()).collect();
    if existing.is_empty() {
        // A command's files can already be gone while the dispatcher still
        // calls it -- a half-finished delete by hand is exactly when the
        // registration most needs taking out.
        if matches!(kind, ArtifactKind::Command) && !pretend {
            unregister_command(&root, &name)?;
        }
        if !pretend {
            crate::generated_files::forget(&root, &kind_key, &name, package)?;
        }
        // Two very different situations, and saying "nothing to destroy" over
        // both is how a reader concludes the files are gone when they are not.
        if paths.is_empty() && recorded.is_none() {
            println!(
                "nothing to destroy: no record of `{kind_key} {name}` in .jails/ledger.toml, \n                        and its paths cannot be recomputed without the arguments it was generated with.\n                        fix: re-run the `jails g {kind_key} {name} ...` that created it so jails records \
                 the paths, then destroy -- or delete the files by hand."
            );
        } else {
            println!("nothing to destroy");
        }
        return Ok(());
    }

    if !force && !pretend {
        println!("about to delete:");
        for p in &existing {
            println!("  {}", p.display());
        }
        print!("proceed? [y/N] ");
        std::io::stdout().flush().ok();
        let mut answer = String::new();
        std::io::stdin()
            .read_line(&mut answer)
            .map_err(|e| format!("failed to read confirmation: {e}"))?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            println!("aborted");
            return Ok(());
        }
    }

    if pretend {
        for p in existing {
            println!("would remove {}", p.display());
        }
        if matches!(kind, ArtifactKind::Command) {
            println!("would unregister {name}Command from its dispatcher");
        }
        println!();
        println!("--pretend: nothing was deleted.");
        return Ok(());
    }

    for p in existing {
        fs::remove_file(p).map_err(|e| format!("failed to remove {}: {e}", p.display()))?;
        println!("removed {}", p.display());
    }
    // After the files, not before: an unregistration that succeeded over a
    // failed delete would leave a class nothing dispatches to.
    if matches!(kind, ArtifactKind::Command) {
        unregister_command(&root, &name)?;
    }
    crate::generated_files::forget(&root, &kind_key, &name, package)?;
    Ok(())
}

/// The paths `generate` would write for this intent, with the arguments gone.
///
/// `destroy` is given a kind and a name and nothing else, and most generators
/// refuse without their arguments -- an enum wants constants, a use case wants
/// `--on` and `--yields`. **Those arguments decide the file *contents*, not the
/// file *names*.** So this offers each generator arguments it will accept and
/// keeps the paths, which is why `PROBES` is three rows rather than a table of
/// every path every kind writes.
///
/// That is the whole of `abstract.md` rungs 4-5. The 672-line `KIND_FILES` this
/// replaces was a hand transcription of paths `artifacts_for` already computes,
/// and `tests/agreement.rs` existed to police the drift between them -- §9's
/// "a test that polices duplication is a receipt for a decision not made". The
/// agreement test now proves the *one* list is right in both directions,
/// including on a project with no record at all.
///
/// An intent nothing accepts yields no paths. Under-naming is the safe failure:
/// the reader keeps a file and deletes it by hand, where over-naming loses work.
fn recomputed_paths(
    project: &Project,
    kind: ArtifactKind,
    name: &str,
    package: Option<&str>,
) -> Vec<PathBuf> {
    // `--on`/`--yields` must name a record that is really there -- the
    // generator reads its components -- and the kinds that take one also want
    // their fields to *be* those components. Any record will do, because the
    // file names do not carry the target; `tests/agreement.rs` is what proves
    // that, in both directions, rather than this comment.
    let domain = project.package_named(layout::DOMAIN, package);
    let target = any_record_in(project, package);
    let components: Vec<String> = target
        .as_deref()
        .and_then(|ty| project.record_in(&domain, ty))
        .map(|fields| {
            fields
                .iter()
                .map(|field| format!("{}:{}", field.name, field.java_type))
                .collect()
        })
        .unwrap_or_default();
    let one = components.first().cloned().into_iter().collect::<Vec<_>>();
    // A kind whose fields name components of *its own* record rather than of a
    // `--on` target -- `search` is one, and its record is the intent's name.
    // Only the `String` components: `search` refuses a uuid, correctly, so the
    // whole component list is not a spec it would accept.
    let own_text: Vec<String> = project
        .record_in(&domain, name)
        .map(|fields| {
            fields
                .iter()
                .filter(|field| field.java_type == "String")
                .map(|field| field.name.clone())
                .collect()
        })
        .unwrap_or_default();
    let on = target.as_deref();
    let a_component = vec!["value:string".to_string()];
    let a_constant = vec!["VALUE".to_string()];

    // Least-committal first, so a generator that needs nothing is never handed
    // a spec it would have used. Seven rows against `KIND_FILES`'s 672 -- and
    // these are the shapes generators *demand*, not the paths they produce,
    // which is why this one cannot drift out of step with them.
    let probes: Vec<(&[String], Option<&str>, Option<&str>)> = vec![
        (&[], None, None),
        // A record, value, DTO or event: one component of any type.
        (&a_component, None, None),
        // Enum constants and sealed variants are bare identifiers, which the
        // `name:type` parser rejects and vice versa.
        (&a_constant, None, None),
        // A use case, query or transition over an existing resource. `yields`
        // is left off first: it turns on the outbox half, which demands
        // capabilities this project may not have.
        (&[], on, None),
        (&one, on, None),
        (&one, on, on),
        (&[], on, on),
        (&own_text, None, None),
    ];

    for (fields, strategy_on, strategy_yields) in &probes {
        let Ok(artifacts) = artifacts_for(
            project,
            &Recipe {
                kind,
                name,
                fields,
                indexes: &[],
                strategy_on: *strategy_on,
                strategy_yields: *strategy_yields,
            },
            package,
        ) else {
            continue;
        };
        return artifacts
            .into_iter()
            // `package-info.java` belongs to the package, not to one intent --
            // `ALLOWED_LEFTOVER` in `tests/agreement.rs` records the same rule
            // for the recorded path.
            .filter(|artifact| {
                artifact
                    .path
                    .file_name()
                    .is_some_and(|file| file != "package-info.java")
            })
            .map(|artifact| artifact.path)
            .collect();
    }
    Vec::new()
}

/// Any record in the project's domain package, by name.
///
/// Only ever used to satisfy a generator that demands `--on`/`--yields` while
/// `destroy` recomputes paths. Deterministic (sorted) so two runs on one
/// project agree.
fn any_record_in(project: &Project, package: Option<&str>) -> Option<String> {
    let domain = project.package_named(layout::DOMAIN, package);
    let mut names: Vec<String> = std::fs::read_dir(main_dir(project.root(), &domain))
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "java") {
                path.file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
            } else {
                None
            }
        })
        .collect();
    names.sort();
    names
        .into_iter()
        .find(|name| project.record_in(&domain, name).is_some())
}
