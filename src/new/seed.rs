//! What every new project gets, whichever path created it.
//!
//! `mise.toml`, `AGENTS.md`, the fixtures directory, `.gitignore`, `git init`
//! — and `--app`, which seeds `.jails/app.toml` and replays it into the model
//! before the tree is published.
//!
//! Its own file rather than a section of either: neither [`super::spring`]
//! nor [`super::plain`] owns these, and a helper that both call from a file
//! one of them owns is a helper that will grow a special case for its owner.

use super::*;

/// Seed a freshly created project with a manifest and apply it.
///
/// `new` + `mkdir .jails` + `cp app.toml` + `app apply` is four steps that
/// only ever appear together. The manifest path is resolved against the
/// directory the *user* is standing in, not the project just created, because
/// that is where they are pointing from.
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
    // A seeded model means the manifest replays into it through the same
    // frontends `jails g` uses, at the root of the tree being published --
    // never the process directory, which is this project's parent.
    // `no_start` is not passed on because the canonical path has no external
    // service effects to suppress; `sync` refuses the flag by name for the
    // same reason.
    let _ = no_start;
    crate::app::replay(
        None,
        crate::Invocation::for_new(tree.root().to_path_buf(), debug),
    )?;
    Ok(crate::app::Applied::Clean)
}

/// Whether the manifest run left a failure the caller still has to report.
///
/// `jails new --app` publishes by rename, so an error thrown out of the apply
/// discards the whole scratch tree -- which is right for a manifest that could
/// not be applied and wrong for one that *was*. A compose service that will
/// not start, on a machine where an unrelated container already holds `:5432`,
/// is a post-*commit* effect; it must not be able to unmake the commit, still
/// less the project the commit is in, or the reader is left with no project
/// and no way to tell which of the two happened.
/// The files a reader did not ask for, named once at creation.
///
/// **`Created ./demo` and nothing else leaves a reader to find the rest with
/// `ls`.** A new project holds an `AGENTS.md`, a `mise.toml` and a `.jails/`
/// that nothing in the command mentioned, and the moment to say so is the
/// moment they appear. `src/` and the build file are what the reader asked
/// for by running `new`, so they are counted rather than listed: naming
/// forty files teaches nothing.
pub(super) fn report_unasked_files(staged: &[String]) {
    let asked = |path: &str| {
        path.starts_with("src/")
            || matches!(path, "pom.xml" | "build.gradle" | "build.gradle.kts")
            || path.starts_with("gradle/")
            || path.starts_with("mvnw")
            || path.starts_with("gradlew")
    };
    let source = staged
        .iter()
        .filter(|path| path.starts_with("src/"))
        .count();
    if source > 0 {
        println!("  {source} source file(s) under src/");
    }
    // The executor's own scratch, which `.jails/.gitignore` names for the
    // same reason: it is state, not a file the reader has anything to do
    // with. It is not removed here -- a lock file removed while a holder has
    // it open is a lock the next opener cannot see.
    let scratch = |path: &str| path == ".jails/apply.lock" || path.starts_with(".jails/run/");
    for path in staged.iter().filter(|path| !asked(path) && !scratch(path)) {
        println!("  {path}");
    }
}

pub(super) fn reported(applied: crate::app::Applied) -> Result<()> {
    match applied {
        crate::app::Applied::Clean => Ok(()),
    }
}

/// Apply `--app <manifest>` to the project being created, before it is
/// published.
///
/// The manifest is inside the publication rather than after it: `new --app`
/// is one command, and a destination holding a project whose manifest
/// half-applied is exactly the state publication-by-rename exists to remove.
/// The manifest path is resolved against the directory the *user* is standing
/// in, which is why it is read before anything is written.
pub(super) fn seed(
    publication: &publish::Publication,
    app: Option<&Path>,
    no_start: bool,
    debug: bool,
) -> Result<crate::app::Applied> {
    match app {
        // A model was already seeded as the project's own
        // `.jails/model.jdl` and compiled by `seed_canonical_model`, which
        // is the whole of "a copy and one sync". Only a manifest has a
        // second phase, because its rows are commands rather than a model.
        Some(supplied) if Supplied::of(supplied)? == Supplied::Model => {
            Ok(crate::app::Applied::Clean)
        }
        Some(manifest) => seed_manifest(&publication.tree(), manifest, no_start, debug),
        None => Ok(crate::app::Applied::Clean),
    }
}

/// Which of the two things `--app`/`--model` was pointed at.
///
/// **Decided by name, and refused by name.** A manifest is replayed as
/// commands and a model is copied and compiled; they are not
/// interchangeable, and a file read as the wrong one fails somewhere deep
/// with a parse error about a syntax the reader never wrote.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum Supplied {
    /// `.jdl`: the one editable source, copied in and compiled.
    Model,
    /// `.toml`: a manifest of `[[generate]]` rows, replayed as commands.
    Manifest,
}

impl Supplied {
    pub(super) fn of(path: &Path) -> Result<Self> {
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("jdl") => Ok(Self::Model),
            Some("toml") => Ok(Self::Manifest),
            _ => Err(jails_support::Failure::Told(format!(
                "`{}` is neither a model nor a manifest\n       fix: point `--model` at a \
                 `.jdl` file, which is copied in and compiled, or `--app` at a `.toml` \
                 manifest, whose rows are replayed as commands",
                path.display()
            ))),
        }
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
    let package = jails_spec::spec::paths::base_package(tree.root())
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
/// `model_command::owns` is "does `.jails/model.jdl` exist".
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
    // A manifest is seeded like any other project: it replays into the model
    // rather than being refused beside it, and `seed_manifest` runs that
    // replay against this tree's root once the project exists. A model needs
    // no second phase -- it *is* the model, and the compile below is the
    // "one sync".
    let source = match app {
        Some(supplied) if Supplied::of(supplied)? == Supplied::Model => {
            println!("  model    {}", supplied.display());
            adopted(&source, supplied)?
        }
        _ => source,
    };
    tree.put_named(".jails/model.jdl", source, ".jails/model.jdl")?;
    crate::model_command::materialize_seed(tree.root())
}

/// The supplied model, under this project's own identity.
///
/// **A model file declares an application, and `jails new` just created a
/// different one.** Copying `crawler.jdl` into a project called `spider`
/// verbatim would leave a model naming a package no directory holds and a
/// build the command line did not choose -- and `--gradle` beside
/// `build maven` is not a disagreement to resolve silently in the file's
/// favour, because the reader typed the flag a second ago and wrote the file
/// last year.
///
/// So the identity lines come from the seed and everything else from the
/// file: `pkg`, `java`, `platform` and `build` are facts of the project that
/// was just written, and `storage` is a declaration the file is entitled to
/// make. Nothing else in the file is touched, which is what keeps this a
/// copy.
fn adopted(seeded: &str, supplied: &Path) -> Result<String> {
    let text = std::fs::read_to_string(supplied).map_err(|error| {
        jails_support::Failure::Told(format!(
            "could not read the model {}: {error}\n       fix: pass `--model <path>` \
             pointing at a readable `.jdl` file",
            supplied.display()
        ))
    })?;
    let identity = |source: &str, key: &str| -> Option<String> {
        source
            .lines()
            .take_while(|line| !line.starts_with('}'))
            .find(|line| line.trim_start().starts_with(&format!("{key} ")))
            .map(str::to_string)
    };
    let mut inside_app = false;
    let mut seen_app = false;
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        if !seen_app && line.starts_with("app ") {
            inside_app = true;
            seen_app = true;
            // The `app` line carries the application's name and may carry a
            // pinned `@id`; both belong to the project being created.
            out.push(
                identity(seeded, "app")
                    .ok_or_else(|| jails_support::Failure::Told(SEED_HAS_NO_APP.to_string()))?,
            );
            continue;
        }
        if inside_app {
            if line.starts_with('}') {
                inside_app = false;
                out.push(line.to_string());
                continue;
            }
            let key = line.trim_start().split(' ').next().unwrap_or("");
            if matches!(key, "pkg" | "java" | "platform" | "build") {
                if let Some(replacement) = identity(seeded, key) {
                    out.push(replacement);
                }
                continue;
            }
        }
        out.push(line.to_string());
    }
    if !seen_app {
        return Err(jails_support::Failure::Told(format!(
            "the model {} declares no `app` block\n       fix: give it one, or start from \
             what `jails new` writes -- every JDL source names the application it is about",
            supplied.display()
        )));
    }
    let mut source = out.join("\n");
    source.push('\n');
    Ok(source)
}

const SEED_HAS_NO_APP: &str = "the seeded model has no `app` line, which is a jails bug rather than anything \
     about your project\n       fix: report it with the `jails new` command you ran";

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
