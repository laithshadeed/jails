//! Getting a generated file onto disk in the shape jails guarantees.
//!
//! One secret, and it is the reason this is a module rather than thirteen
//! helpers loose in `generate.rs`: **every rule here is one a template would
//! otherwise have to remember**, and a rule twenty templates must remember is
//! a rule that decays. Import order is normalised at write time, not in
//! templates. `package-info.java` is planned from here, not per kind. Failsafe
//! and AssertJ are ensured from the write path, not from the generator that
//! happened to emit the first `*IT`.
//!
//! `CLAUDE.md` records what each of those cost when it was not true: jails
//! generated integration tests for months that `mvn verify` never ran, and
//! `--pretend` named two files where the real run wrote three.

use crate::model::Artifact;
use jails_support::Result;
use jails_support::apply::Tree;
use std::path::Path;

/// Whether this change writes an integration test and therefore needs the
/// Failsafe plugin. Derived from the planned files so a recipe cannot forget.
pub(crate) fn writes_an_it(artifacts: &[Artifact]) -> bool {
    artifacts.iter().any(|a| {
        a.path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("IT.java"))
    })
}

/// The Boot 4 package `@WebMvcTest` and `@AutoConfigureMockMvc` moved to.
///
/// Matched in the *bytes about to be written* rather than per generator, for
/// the same reason import normalisation and `package-info.java` planning live
/// on the write path: a rule twenty recipes have to remember is a rule that
/// decays. A Boot 3 project renders the legacy package, so this is false
/// there and the dependency is not added -- which is right, since on Boot 3
/// the class is in `spring-boot-test-autoconfigure` and already present.
pub(crate) const WEBMVC_TEST_PACKAGE: &str = "org.springframework.boot.webmvc.test.autoconfigure";

/// Did this batch write a test that needs Boot 4's servlet test slice?
pub fn writes_a_webmvc_test(artifacts: &[Artifact]) -> bool {
    artifacts
        .iter()
        .any(|artifact| artifact.contents.contains(WEBMVC_TEST_PACKAGE))
}

/// Did this batch write anything under `src/test`?
pub(crate) fn writes_a_test(artifacts: &[Artifact]) -> bool {
    artifacts
        .iter()
        .any(|a| a.path.to_string_lossy().contains("src/test/java"))
}

/// Write a file jails is creating, into the staging tree of a project being
/// published.
///
/// **The tree is a parameter, and its type is the point.** It was `root:
/// &Path`, which cannot be rediscovered correctly -- the side effect below
/// needs the project being *written to*, and the process CWD is not it;
/// `new-cli` writes into a directory that does not contain the CWD, which is
/// why a `new-cli` project's own base package never got the
/// `package-info.java` every other package gets.
///
/// A path, though, says nothing about what it is. Every one of this function's
/// nine callers is on the `jails new` path, where the destination is a
/// reserved scratch published by a single `rename`; taking an
/// [`apply::Tree`](jails_support::apply::Tree) is what says so in the signature
/// rather than in a comment, and it makes the claim checkable -- a write
/// outside the staging tree is refused. `pending.md` §7.7.
pub fn write_new_file(tree: Tree<'_>, path: &Path, contents: &str) -> Result<()> {
    // The refusal stays here rather than in the write layer, because this is
    // the one a person reads: it names the three ways forward. `Tree::create_at`
    // repeats the check underneath, which costs nothing and closes the window
    // between the two.
    if path.exists() {
        return Err(format!(
            "{} already exists.\n       fix: choose a different name, destroy the generated artifact first, or use `jails g field` to evolve an existing model.",
            path.display()
        ).into());
    }
    let contents = if path.extension().is_some_and(|e| e == "java") {
        ensure_package_info(tree, path)?;
        jails_java::tidy::tidy_blank_lines(&jails_java::tidy::normalize_imports(contents))
    } else {
        contents.to_string()
    };
    tree.create_at(path, &contents)
}

/// Give a package a null-marked `package-info.java` the first time jails puts
/// a class in it.
///
/// JSpecify's `@NullMarked` is a *package-level* opt-in: without it every
/// reference type in the package is "unspecified nullness" and a nullness
/// checker has nothing to check. `java.md` calls this the standard rather
/// than a proposal, and jails generated seven packages in one real project
/// without a single one.
///
/// Done here rather than per-kind for the same reason import ordering is: a
/// rule that each of twenty templates has to remember is a rule that decays.
/// Writing it at the moment a package first receives a class also means it
/// lands exactly once, with no bookkeeping about which packages exist.
///
/// Only for `src/main/java` -- a nullness contract on test sources buys
/// nothing and would put a file in every test package.
///
/// **This is best-effort on purpose.** A project that has not added the
/// `org.jspecify:jspecify` dependency would not compile with the annotation,
/// so nothing is written unless the annotation is actually available. That is
/// checked by the caller chain rather than here; see `jspecify_available`.
pub(crate) fn ensure_package_info(tree: Tree<'_>, class_path: &Path) -> Result<()> {
    let Some(dir) = class_path.parent() else {
        return Ok(());
    };
    if !dir.to_string_lossy().contains("src/main/java") {
        return Ok(());
    }
    let info = dir.join("package-info.java");
    // The file being written may *be* the package-info: it is an artifact in
    // its own right, so `write_new_file` is called for it like any other. Left
    // unguarded this writes the path and then returns to a caller that writes
    // it again -- harmless while the second write was a bare overwrite, and an
    // "already exists" refusal the moment the write path started refusing to
    // clobber. A latent double-write, surfaced by giving `create` real meaning.
    if class_path == info || info.exists() {
        return Ok(());
    }
    // Read here rather than threaded in: `write_new_file`'s nine callers
    // include `new`, which is creating the pom this would be read from. The
    // pure `jspecify_available` above is the half that matters -- this is the
    // belt to `planned_package_infos`' braces, and it runs only for a path
    // that has no `package-info.java` yet.
    if !jspecify_available(&crate::pom::read(tree.root()).unwrap_or_default()) {
        return Ok(());
    }
    let Some(pkg) = package_of_dir(tree.root(), dir) else {
        return Ok(());
    };
    tree.put_at(&info, package_info_java(&pkg))?;
    Ok(())
}

pub(crate) fn package_info_java(pkg: &str) -> String {
    format!(
        r#"/**
 * Every reference type in this package is non-null unless it is explicitly
 * annotated {{@code @Nullable}}.
 *
 * <p>This is a package-level opt-in because that is the only level JSpecify
 * offers: without it the package is "unspecified nullness" and a nullness
 * checker has nothing to check.
 */
@NullMarked
package {pkg};

import org.jspecify.annotations.NullMarked;
"#
    )
}

/// The `package-info.java` files this artifact list would cause to be
/// written, as artifacts in their own right.
///
/// `write_new_file` creates these as a side effect of writing a class, which
/// made them **invisible**: `--pretend` listed two files and `generate` then
/// wrote three. A preview that does not name every write is not a preview,
/// and it is the one command whose entire job is to tell you what will
/// happen.
///
/// Planning them here rather than teaching the preview to predict the side
/// effect is the point -- a second piece of code guessing what the first will
/// do is exactly the drift this costs elsewhere. They are prepended to the
/// plan so each lands before the class that needed it, at which point
/// `ensure_package_info` finds the file present and does nothing.
pub(crate) fn planned_package_infos(
    root: &Path,
    pom: &str,
    artifacts: &[Artifact],
) -> Vec<Artifact> {
    if !jspecify_available(pom) {
        return Vec::new();
    }
    let mut planned = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for artifact in artifacts {
        if artifact.path.extension().is_none_or(|e| e != "java") {
            continue;
        }
        let Some(dir) = artifact.path.parent() else {
            continue;
        };
        // Main sources only: a nullness contract on tests buys nothing and
        // would put one of these in every test package.
        if !dir.to_string_lossy().contains("src/main/java") {
            continue;
        }
        let info = dir.join("package-info.java");
        if info.exists() || !seen.insert(info.clone()) {
            continue;
        }
        let Some(pkg) = package_of_dir(root, dir) else {
            continue;
        };
        planned.push(Artifact {
            kind: "package-info",
            path: info,
            contents: package_info_java(&pkg),
        });
    }
    planned
}

/// Whether `org.jspecify:jspecify` is a declared dependency.
///
/// Annotating a package that cannot resolve `@NullMarked` would hand the
/// reader a compile error for a file they did not ask for, which is the exact
/// opposite of what a scaffold is for.
pub(crate) fn jspecify_available(pom: &str) -> bool {
    crate::pom::has_dependency(pom, "org.jspecify", "jspecify")
}

/// The package name for a directory under `src/main/java`.
pub(crate) fn package_of_dir(root: &Path, dir: &Path) -> Option<String> {
    let src_root = root.join("src/main/java");
    let rel = dir.strip_prefix(&src_root).ok()?;
    let pkg = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join(".");
    (!pkg.is_empty()).then_some(pkg)
}
