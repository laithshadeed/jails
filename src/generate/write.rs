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

use crate::model::{Artifact, Change, Project};
use jails_support::Result;
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

/// Every generated test is written against AssertJ, so the project has to
/// have it.
///
/// The same rule as `ensure_failsafe` and for the same reason: handing the
/// reader `cannot find symbol: method assertThat` for a file they did not
/// write is the plumbing this tool exists to remove. `jails new` and
/// `new-cli` put AssertJ in the pom, which is why this went unnoticed --
/// **the projects that need it are the ones jails did not create**, and
/// `jails add`/`jails g` on an existing plain Maven project is the whole
/// point of §12.
///
/// Under a Spring Boot parent the version is left to the BOM; without one it
/// is pinned, since a versionless dependency is a pom Maven refuses to read
/// (plan.md §8.1).
pub(crate) fn ensure_assertj(project: &Project, writes_a_test: bool) -> Result<()> {
    if !writes_a_test || project.build() != crate::build::Build::Maven {
        return Ok(());
    }
    let pom = project.pom().to_string();
    // A Spring Boot project gets AssertJ transitively through the test
    // starter, and jails' own `new` declares it outright. Adding a second
    // declaration would be noise in a file the reader owns.
    if crate::pom::has_dependency(&pom, "org.assertj", "assertj-core")
        || pom.contains("spring-boot-starter-test")
        || pom.contains("spring-boot-starter-webmvc-test")
    {
        return Ok(());
    }
    ensure_dependency(project.root(), &crate::pom::assertj(project.flavor()))
}

/// Did this batch write anything under `src/test`?
pub(crate) fn writes_a_test(artifacts: &[Artifact]) -> bool {
    artifacts
        .iter()
        .any(|a| a.path.to_string_lossy().contains("src/test/java"))
}

/// Splice a dependency into pom.xml unless it is already there.
///
/// Comment-preserving, like every other pom edit jails makes: the file
/// belongs to the reader, and a generator that reformats it has taken more
/// than it was asked for.
pub(crate) fn ensure_dependency(root: &Path, dep: &crate::pom::Dependency) -> Result<()> {
    // Nothing to splice into, and jails will not write a foreign build file.
    // `generate::report_degraded_shape` has already named this dependency for
    // the reader to add, which is the honest half of the trade.
    if crate::build::detect(root) != crate::build::Build::Maven {
        return Ok(());
    }
    let pom = crate::pom::read(root)?;
    match crate::pom::add_dependency(&pom, dep)? {
        Some(updated) => {
            crate::apply::put_named(root.join("pom.xml"), updated, "pom.xml")?;
            println!("     dep {}:{}", dep.group_id, dep.artifact_id);
            Ok(())
        }
        None => Ok(()),
    }
}

/// Apply the POM portion of a planned change in memory and write it once.
pub(crate) fn apply_build_change(root: &Path, pom: &str, change: &Change) -> Result<()> {
    if crate::build::detect(root) != crate::build::Build::Maven {
        return Ok(());
    }
    let mut updated = pom.to_string();
    let mut changed = false;

    // Preserve the historical insertion order (plugin, AssertJ, recipe
    // dependencies) while collapsing all edits into one filesystem write.
    for (artifact_id, body) in &change.plugins {
        if let Some(next) = crate::pom::add_plugin(&updated, artifact_id, body)? {
            updated = next;
            changed = true;
            println!("  plugin {artifact_id}");
        }
    }
    for dep in &change.deps {
        if let Some(next) = crate::pom::add_dependency(&updated, dep)? {
            updated = next;
            changed = true;
            println!("  dep {}:{}", dep.group_id, dep.artifact_id);
        }
    }
    if changed {
        crate::apply::put_named(root.join("pom.xml"), updated, "pom.xml")?;
    }
    Ok(())
}

/// Write a file jails is creating, into a project whose root the caller
/// names.
///
/// `root` is a parameter rather than something this rediscovers, because it
/// cannot be rediscovered correctly: the side effect below needs the project
/// being *written to*, and process CWD is not it. `new-cli` writes into a
/// directory that does not contain the CWD, so the lookup either found the
/// surrounding project (wrong pom, wrong package) or found nothing -- which
/// is why a `new-cli` project's own base package never got the
/// `package-info.java` every other package gets.
pub(crate) fn write_new_file(root: &Path, path: &Path, contents: &str) -> Result<()> {
    // The refusal stays here rather than in `crate::apply::create`, because this is
    // the one a person reads: it names the three ways forward. `crate::apply::create`
    // repeats the check underneath, which costs nothing and closes the window
    // between the two.
    if path.exists() {
        return Err(format!(
            "{} already exists.\n       fix: choose a different name, destroy the generated artifact first, or use `jails g field` to evolve an existing model.",
            path.display()
        ));
    }
    let contents = if path.extension().is_some_and(|e| e == "java") {
        ensure_package_info(root, path)?;
        tidy_blank_lines(&normalize_imports(contents))
    } else {
        contents.to_string()
    };
    crate::apply::create(path, &contents)
}

/// Collapse the blank lines a template leaves behind when an optional section
/// renders empty, and end the file with exactly one newline.
///
/// Here for the same reason `normalize_imports` is: **palantir-java-format
/// removes both**, so leaving them in means `add format` -- which jails
/// installs itself -- fails `jails check` on a project whose every line jails
/// wrote. That is not hypothetical. It is what App D (`examples/ledger-cli`)
/// hit on its first gate run, in four files, because it is the first proof
/// application to ask for `format` at all: `class NoteTest {` followed by two
/// blank lines wherever the sample block was omitted, and a
/// `package-info.java` ending on a blank line after its import.
///
/// Fixing it in each template is the rule-twenty-templates-must-remember that
/// this write path exists to avoid.
///
/// **Text blocks are left alone.** A `"""` block is the one Java literal that
/// can span lines, so a blank line inside one is data -- SQL, JSON, an
/// expected message -- and collapsing it would change what the program says.
/// Counting the delimiters is enough to know which side of one a line is on.
pub(crate) fn tidy_blank_lines(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut in_text_block = false;
    let mut previous_blank = false;
    for line in source.lines() {
        let blank = line.trim().is_empty();
        if !in_text_block && blank && previous_blank {
            continue;
        }
        out.push_str(line);
        out.push('\n');
        if line.matches("\"\"\"").count() % 2 == 1 {
            in_text_block = !in_text_block;
        }
        previous_blank = blank && !in_text_block;
    }
    // A file that ends on a blank line is the same violation at the bottom.
    let trimmed = out.trim_end_matches('\n');
    if trimmed.is_empty() {
        return String::new();
    }
    format!("{trimmed}\n")
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
pub(crate) fn ensure_package_info(root: &Path, class_path: &Path) -> Result<()> {
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
    if !jspecify_available(&crate::pom::read(root).unwrap_or_default()) {
        return Ok(());
    }
    let Some(pkg) = package_of_dir(root, dir) else {
        return Ok(());
    };
    crate::apply::put(&info, package_info_java(&pkg))?;
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
        if !artifact.path.extension().is_some_and(|e| e == "java") {
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

/// Rewrite a generated file's import block into the order
/// palantir-java-format produces: static imports first, a blank line, then
/// everything else sorted.
///
/// Done here, once, rather than by hand in each of the twenty-odd templates.
/// Hand-ordering is a rule that decays -- the next template gets it wrong and
/// nobody notices until `jails add format` makes `mvn verify` fail on a
/// freshly generated project, which is a bad first impression for a scaffold
/// to make.
pub(crate) fn normalize_imports(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();

    let Some(package_at) = lines
        .iter()
        .position(|l| l.trim_start().starts_with("package "))
    else {
        return source.to_string();
    };

    // Imports are only ever between the package declaration and the first
    // other construct, so scanning stops at the first line that is neither an
    // import nor blank -- a Javadoc block, an annotation, the type itself.
    let mut statics: Vec<&str> = Vec::new();
    let mut plain: Vec<&str> = Vec::new();
    let mut end = package_at + 1;
    for (offset, line) in lines[package_at + 1..].iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("import ") {
            if rest.starts_with("static ") {
                statics.push(trimmed);
            } else {
                plain.push(trimmed);
            }
            end = package_at + 1 + offset + 1;
            continue;
        }
        break;
    }

    if statics.is_empty() && plain.is_empty() {
        return source.to_string();
    }

    statics.sort_unstable();
    statics.dedup();
    plain.sort_unstable();
    plain.dedup();

    let mut out = String::with_capacity(source.len() + 32);
    for line in &lines[..=package_at] {
        out.push_str(line);
        out.push('\n');
    }
    out.push('\n');
    for group in [&statics, &plain] {
        if group.is_empty() {
            continue;
        }
        for line in group.iter() {
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }
    // Whatever followed the imports, with any blank lines it was padded with
    // already consumed above.
    for line in lines[end..].iter().skip_while(|l| l.trim().is_empty()) {
        out.push_str(line);
        out.push('\n');
    }
    out
}
