//! What every new project gets, whichever path created it.
//!
//! `mise.toml`, `AGENTS.md`, the fixtures directory, `.gitignore`, `git init`
//! — and `--app`, which seeds `.jails/app.toml` and applies it as an ordinary
//! V2 transition before the tree is published.
//!
//! `pending.md` §8.1's split, and the reason this is its own file rather than
//! a section of either: neither [`super::spring`] nor [`super::plain`] owns
//! these, and a helper that both call from a file one of them owns is a helper
//! that will grow a special case for its owner.

use super::*;

/// Port of the `spring-init` bash function: wraps start.spring.io's
/// starter.zip API. baseDir wraps the archive in a `$name/` folder
/// server-side, so extracting to "." lands the project at `./$name`.
/// Seed a freshly created project with a manifest and apply it.
///
/// plan.md §18 ends by asking which two commands should have been one, and
/// answers itself: `new` + `mkdir .jails` + `cp app.toml` + `app apply` is four
/// steps that only ever appear together. The manifest path is resolved against
/// the directory the *user* is standing in, not the project just created,
/// because that is where they are pointing from.
pub fn seed_manifest(
    tree: &publish::Tree<'_>,
    manifest: &Path,
    no_start: bool,
    debug: bool,
) -> Result<crate::app::Applied> {
    let source = std::fs::read_to_string(manifest).map_err(|error| {
        format!(
            "failed to read the application manifest {}: {error}\n       \
             fix: pass `--app <path>` pointing at a readable `.jails/app.toml`.",
            manifest.display()
        )
    })?;
    tree.put(".jails/app.toml", &source)?;
    println!("  manifest {}", manifest.display());
    crate::app::apply_in(tree.root(), no_start, debug)
}

/// Whether the manifest run left a failure the caller still has to report.
///
/// `jails new --app` publishes by rename, so an error thrown out of the apply
/// discards the whole scratch tree -- which is right for a manifest that could
/// not be applied and wrong for one that *was*. A compose service that will
/// not start, on a machine where an unrelated container already holds `:5432`,
/// used to leave the report saying `ledger create` and the destination
/// directory absent: no `jails:` line, no project, and no way to tell which of
/// the two had happened. The effect is explicitly post-*commit*; it must not
/// be able to unmake the commit, still less the project the commit is in.
pub(super) fn reported(applied: crate::app::Applied) -> Result<()> {
    match applied {
        crate::app::Applied::Clean => Ok(()),
        crate::app::Applied::CommittedThenReported => Err(jails_support::Failure::Reported),
    }
}

/// Apply `--app <manifest>` to the project being created, before it is
/// published.
///
/// plan.md §R6.5 puts the manifest inside the publication rather than after
/// it: `new --app` is one command, and a destination holding a project whose
/// manifest half-applied is exactly the state publication-by-rename exists to
/// remove. The manifest path is resolved against the directory the *user* is
/// standing in, which is why it is read before anything is written.
pub(super) fn seed(
    publication: &publish::Publication,
    app: Option<&Path>,
    no_start: bool,
    debug: bool,
) -> Result<crate::app::Applied> {
    match app {
        Some(manifest) => seed_manifest(&publication.tree(), manifest, no_start, debug),
        None => Ok(crate::app::Applied::Clean),
    }
}

/// What `--app` does under `--pretend`: nothing, and says so.
///
/// A preview created no project, so there is nothing to apply a manifest to,
/// and saying that beats failing to find a `pom.xml` that was never written.
pub(super) fn previewed(app: Option<&Path>) -> Result<()> {
    if let Some(manifest) = app {
        println!(
            "--pretend: no project was created, so {} was not applied.",
            manifest.display()
        );
    }
    Ok(())
}

/// Test fixtures land on the test classpath, so they belong under
/// `src/test/resources`. Git can't track an empty directory, so seed it with
/// a `.gitkeep` -- otherwise the folder vanishes on the first clone.
pub(super) fn write_fixtures_dir(tree: &publish::Tree<'_>) -> Result<()> {
    let dir = tree.root().join("src/test/resources/fixtures");
    tree.ensure_directory_at(&dir)?;
    tree.put_at(&dir.join(".gitkeep"), "")?;
    Ok(())
}

pub(super) fn write_mise(tree: &publish::Tree<'_>, java: &str) -> Result<()> {
    let path = tree.root().join("mise.toml");
    tree.put_at(&path, format!("[tools]\njava = \"{java}\"\n"))
}

pub(super) fn write_agents(tree: &publish::Tree<'_>, java: &str) -> Result<()> {
    let path = tree.root().join("AGENTS.md");
    if path.exists() {
        return Ok(());
    }
    let package = crate::generate::base_package(tree.root())
        .unwrap_or_else(|_| "the package declared by the application entry point".to_string());
    let rules = crate::lint::agents_rules();
    let body = format!(
        r#"# Working on this project

This project targets Java {java}. Its base package is `{package}`.

## Commands

- Run one test with `jails test <Name>` or place the cursor in it and pass `path:line`.
- Run `jails check` before handing work off. It performs a clean Maven verify so stale class files cannot hide a regression.
- Run `jails doctor` before debugging the machine, container runtime, ports, or dependency injection.
- Use `jails g <kind> --pretend` and `jails add <capability> --pretend` to inspect changes.

## Design

- Domain values are immutable records. Use no ORM and no Lombok.
- Persistence is a repository port plus an explicit JDBC adapter and forward-only SQL migrations.
- Field specs use `name:type`, `name:type!` for non-blank text, `name:type?` for nullable values, and `@pk`, `@unique`, `@index`, or numeric constraints where applicable.
- Keep domain, application, service, adapter, web/API, messaging, job, and testkit packages separate. `jails about --json` reports the configured names.
- Generated applications must remain operable without jails installed.

## Checked stale APIs

`jails lint` enforces this exact list:

{rules}
"#
    );
    tree.put_at(&path, body)
}

pub(super) const GITIGNORE: &str = "target/\n*.class\n.idea/\n*.iml\n.DS_Store\n";

/// Best-effort: a missing/broken git shouldn't fail project creation, just
/// skip repo setup with a warning.
pub(super) fn git_init(tree: &publish::Tree<'_>, debug: bool) {
    let mut cmd = Command::new("git");
    cmd.args(["init", "-q"]).current_dir(tree.root());
    if debug {
        jails_support::debug_cmd(&cmd);
    }
    match cmd.status() {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("jails: git init exited with {status}, skipping"),
        Err(e) => eprintln!("jails: failed to run git init: {e}"),
    }
}

/// The `.jails/model.jdl` a freshly created project starts with, and the first
/// canonical plan applied to it.
///
/// **This is what makes `jails new` produce a canonical project.**
/// `model_command::owns` is "does `.jails/model.jdl` exist", so before this
/// every project jails created took the legacy path and the compiler could
/// only be reached by a model somebody wrote by hand.
///
/// The plan is applied here, inside the scratch tree, rather than left for the
/// reader's first command. The lock it writes is what records which property
/// keys and dependency coordinates the model owns; without it the next command
/// reads every one of them as reader-owned text and refuses to reconcile it.
pub(super) fn seed_canonical_model(
    tree: &publish::Tree<'_>,
    app: Option<&Path>,
    source: String,
) -> Result<()> {
    // **`--app` keeps the project legacy, and the reason has changed.** It
    // used to be that `app apply` refused a canonical project outright, so a
    // seeded model would have left the manifest unappliable. That is no longer
    // true: a manifest replays into the model row by row, through the same
    // frontends `jails g` uses.
    //
    // What blocks it now is where those frontends look. `model_command::root`
    // walks up from the *process* directory, and `jails new` stands in the
    // parent of the project it is creating -- the same edge `compile_at`,
    // `load_model_at`, `resolve_manifest_at` and `materialize_seed` exist to
    // stop, one layer lower. Seeding a model here without extending that
    // containment to the generate frontends would replay every row against
    // whatever encloses the destination directory.
    //
    // And threading one is the wrong shape, which is worth recording so the
    // next attempt does not start there: `read_source` and `finish_generation`
    // are called from seventeen places across `model_generate_jdl` and its
    // four submodules, so the parameter would reach roughly nine entry points
    // -- every one of them a `root: &Path` the `abstract.md` §7 ladder counts,
    // against a ceiling of 79. That rung exists to discourage exactly this,
    // and the `_at` family is a *containment boundary* rather than a pattern
    // to extend downward.
    //
    // The shape that fits is to lift declaration rendering out of the
    // frontends, so a manifest's rows can be appended to the seed source and
    // `materialize_seed` compiles the whole thing at the one root that is
    // already explicit. That is the last thing holding `new --app` on the
    // legacy engine, and it is a piece of work rather than a parameter.
    if app.is_some() {
        return Ok(());
    }
    tree.put_named(".jails/model.jdl", source, ".jails/model.jdl")?;
    crate::model_command::materialize_seed(tree.root())
}

/// One `app` node, with whatever declarations the caller appends after it.
///
/// `storage none` because a new project has no schema yet; `add db` is what
/// changes that, and it is a model patch like any other.
pub(super) fn app_node(
    name: &str,
    package: &str,
    java: &str,
    platform: &str,
    build: &str,
) -> String {
    format!(
        "jdl 1\n\napp {name} {{\n  pkg {package}\n  java {java}\n  platform {platform}\n  build {build}\n  storage none\n}}\n"
    )
}
