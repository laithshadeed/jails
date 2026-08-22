use crate::Result;
use crate::model::{Artifact, Change, Layer, Project};
use clap::ValueEnum;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

mod field;
pub(crate) use field::*;

mod migration;
pub(crate) use migration::*;

mod web;
pub(crate) use web::*;

mod cli;
pub(crate) use cli::*;

mod domain;
pub(crate) use domain::*;

mod repository;
use repository::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum ArtifactKind {
    /// A REST resource that runs: record, port, JDBC + in-memory adapters,
    /// DTOs, service, controller, migration, fixture and tests
    Scaffold,
    /// A Spring `@RestController` stub and its test
    Controller,
    /// A `@Component` stub and its test
    Service,
    /// A plain `final class` and its test, in the base package
    Class,
    /// A plain Java interface
    Interface,
    /// An immutable record with compact-constructor validation, plus a test
    Record,
    /// Add one component to an existing record and safely refresh unchanged
    /// derived files; edited files are reported, never overwritten
    Field,
    /// A fluent test-data builder for an existing record
    Factory,
    /// A record whose fields are all validated as a value object
    Value,
    /// An enum and its test -- the one type jails can build a sample of
    Enum,
    /// A sealed interface with one record per variant; adding one breaks the build
    Sealed,
    /// An open set: one port interface and a bean per implementation, which
    /// Spring collects into a `List<Port>`. The counterpart to `sealed`.
    #[value(alias = "rule")]
    Strategy,
    /// Repository port, a derived JDBC adapter, and a real-database IT
    #[value(alias = "repository")]
    Repo,
    /// The next `VNNN__description.sql` under db/migration; forward-only
    #[value(alias = "mig")]
    Migration,
    /// An `HttpHandler` on the JDK's own server -- no framework
    Handler,
    /// A CLI subcommand, registered in the project's dispatcher
    Command,
    /// A second CLI dispatcher, separate from App.java
    Cli,
    /// A test class per scenario in a markdown file
    Cases,
    /// A declarative HTTP client: `@HttpExchange` interface, group
    /// registration, and a test against a real socket (Spring only)
    Client,
    /// A bounded outbound HTTP fetch port with redirect revalidation, DNS
    /// pinning, SSRF protection, metrics, and real-socket adversarial tests
    /// (Spring only)
    Fetcher,
    /// Scheduled work: a `@Scheduled` component that cannot cancel its own
    /// schedule by throwing (Spring only)
    Job,
    /// A durable, bounded HTTP graph walk composed with an existing safe
    /// fetcher. Generates a PostgreSQL frontier, robots policy, canonical
    /// exact-origin traversal, status/pages/cancel API, and adversarial IT.
    /// `--on` names the fetcher; limits are request/configuration data.
    #[value(name = "http-workflow", alias = "hflow")]
    HttpWorkflow,
    /// A validated relational invariant between two existing scaffolds.
    /// `--on` names the child, `--yields` the parent, and each field is an
    /// explicit `childField=parentField` mapping. Composite mappings enforce
    /// tenant-safe ownership in PostgreSQL instead of trusting HTTP checks.
    #[value(alias = "fk")]
    Association,
    /// An HTTP delivery sink attached to an existing transactional outbox.
    /// `--on` names the use case and `--yields` its typed event. Delivery uses
    /// the event id as an idempotency key and inherits the outbox's leases,
    /// bounded retries, and terminal diagnostics.
    #[value(name = "http-sink", alias = "webhook")]
    HttpSink,
    /// At-most-once execution with a *retained result*: a scoped receipt keyed
    /// by request hash, so a retry replays the first response instead of being
    /// answered 409 by a unique constraint. Needs `jails add db`.
    #[value(alias = "idempotent")]
    Idempotency,
    /// PostgreSQL-backed, leased, bounded-retry work that invokes an existing
    /// generated create use case. `--on` names the use case and `--yields`
    /// names its resource; fields include the stable resource `id`.
    #[value(name = "durable-job", alias = "djob")]
    DurableJob,
    /// Request/response records for a domain type, with the mapping and a
    /// round-trip test (Spring only)
    Dto,
    /// An executable create operation over an existing scaffold: typed
    /// command, use-case port and implementation, HTTP adapter, and tests
    /// (Spring only). `--on` names the target resource.
    #[value(alias = "uc")]
    Usecase,
    /// A typed read operation over an existing scaffold: query record, port,
    /// JDBC adapter, HTTP adapter, and a real-database test. `--on` names the
    /// target resource and fields become equality filters (Spring only).
    Query,
    /// An optimistic, scope-aware update over an existing scaffold. `id`,
    /// `@scope` fields, and `version` identify the row; every other field is
    /// updated and the stored version is incremented atomically (Spring only).
    Transition,
    /// A Kafka slice: payload record, publisher, listener, and an IT against
    /// a real broker (Spring only)
    Event,
    /// A `<Name>Test` skeleton
    Test,
    /// A disabled `<Name>IT` skeleton for a real boundary test; also splices
    /// the Failsafe plugin, without which no `*IT` ever runs
    #[value(name = "integration-test", alias = "it")]
    IntegrationTest,
}

/// Walk up from the current directory looking for pom.xml.
pub(crate) fn find_project_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().map_err(|e| format!("failed to get cwd: {e}"))?;
    loop {
        if dir.join("pom.xml").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err("no pom.xml found in this or any parent directory".to_string());
        }
    }
}

/// Same logic as springgen.nvim's base_package(): read the package line off
/// the project's *Application.java entry point rather than configuring it.
pub(crate) fn base_package(root: &Path) -> Result<String> {
    let src_root = root.join("src/main/java");
    // Spring projects have a *Application.java entry point; `new-cli` ones
    // have App.java, so fall back to whatever source file sits closest to the
    // source root rather than failing on plain Maven projects.
    let entry = find_application_file(&src_root)
        .or_else(|| shallowest_java_file(&src_root))
        .ok_or_else(|| {
            "could not find a .java file under src/main/java to infer the base package".to_string()
        })?;
    let contents = fs::read_to_string(&entry)
        .map_err(|e| format!("failed to read {}: {e}", entry.display()))?;
    for line in contents.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("package ")
            && let Some(pkg) = rest.trim().strip_suffix(';')
        {
            return Ok(pkg.trim().to_string());
        }
    }
    Err(format!(
        "no package declaration found in {}",
        entry.display()
    ))
}

fn find_application_file(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_application_file(&path) {
                return Some(found);
            }
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("Application.java"))
        {
            return Some(path);
        }
    }
    None
}

/// The .java file with the fewest path segments below `dir`, i.e. the one in
/// the outermost package -- for a plain Maven project that is the base package
/// by construction.
fn shallowest_java_file(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(usize, PathBuf)> = None;
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((current, depth)) = stack.pop() {
        for entry in fs::read_dir(&current).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push((path, depth + 1));
            } else if path.extension().is_some_and(|e| e == "java") {
                let better = best.as_ref().is_none_or(|(d, _)| depth < *d);
                if better {
                    best = Some((depth, path));
                }
            }
        }
    }
    best.map(|(_, path)| path)
}

/// Spring Boot 4 moved `@AutoConfigureMockMvc` from
/// `org.springframework.boot.test.autoconfigure.web.servlet` to
/// `org.springframework.boot.webmvc.test.autoconfigure` with no back-compat
/// shim, so the scaffolded controller test needs to import the right one.
/// `@WebMvcTest` moved in Spring Boot 4 the same way `@AutoConfigureMockMvc`
/// did, and for the same reason -- the web slice became its own module.
/// `@WebMvcTest`'s package, which Boot 4 moved with no back-compat shim.
///
/// Reached through [`crate::model::Project::webmvc_test_import`]: the Boot
/// major is a project fact, resolved once, not something a renderer re-reads
/// off disk.
pub(crate) fn webmvc_test_import_for(boot_major: u32) -> &'static str {
    const LEGACY: &str = "org.springframework.boot.test.autoconfigure.web.servlet.WebMvcTest";
    const CURRENT: &str = "org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest";
    if boot_major >= 4 { CURRENT } else { LEGACY }
}

/// The Spring Boot major version from the parent pom, defaulting to 3 when it
/// cannot be read -- the conservative choice, since the pre-4 package names
/// still exist as deprecated aliases in some builds while the 4 ones simply
/// do not exist before 4.
/// The same decision, taken from a pom already in hand.
///
/// `Project` caches the pom once; re-reading it per renderer is exactly the
/// information leakage abstract.md §4.3 names.
pub(crate) fn spring_boot_major_of(pom: &str) -> u32 {
    let Some(idx) = pom.find("spring-boot-starter-parent") else {
        return 3;
    };
    let after = &pom[idx..];
    let Some(vstart) = after.find("<version>").map(|i| i + "<version>".len()) else {
        return 3;
    };
    let Some(vend) = after[vstart..].find("</version>") else {
        return 3;
    };
    after[vstart..vstart + vend]
        .split('.')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3)
}

/// `@AutoConfigureMockMvc`'s package, moved in the same Boot 4 change.
///
/// Reached through [`crate::model::Project::mockmvc_autoconfigure_import`],
/// for the same reason as its `@WebMvcTest` sibling above.
pub(crate) fn mockmvc_autoconfigure_import_for(boot_major: u32) -> &'static str {
    const LEGACY: &str =
        "org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc";
    const CURRENT: &str = "org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc";
    if boot_major >= 4 { CURRENT } else { LEGACY }
}

/// Where each kind of artifact lives, relative to the project's base package.
///
/// A generated project should look like one a person laid out, and nobody
/// lays out thirty classes as siblings of `App.java`. The names are the ones
/// the Java ecosystem already uses, so the layout reads as conventional rather
/// than as jails' invention -- and every one of them is a package a human
/// would have created by hand on about the third file.
///
/// This is a default, not a policy: `--package` overrides it, and `--package
/// ''` puts everything back in the base package for a project small enough not
/// to want the ceremony.
pub(crate) mod layout {
    pub const DOMAIN: &str = "domain";
    /// Ports -- the interfaces the application depends on, kept free of the
    /// technology that implements them.
    pub const APP: &str = "app";
    pub const SERVICE: &str = "service";
    pub const WEB: &str = "web";
    pub const CLI: &str = "cli";
    pub const ADAPTERS: &str = "adapters";
    pub const API: &str = "api";
    pub const TESTKIT: &str = "testkit";
    /// Outbound HTTP: interfaces this application calls, kept apart from
    /// `api` (what it serves) so the direction of a dependency is visible
    /// from the package name alone.
    pub const CLIENTS: &str = "clients";
    /// Scheduled work.
    pub const JOBS: &str = "jobs";
    /// Events published to and consumed from a broker.
    pub const MESSAGING: &str = "messaging";
}

/// Whether this change writes an integration test and therefore needs the
/// Failsafe plugin. Derived from the planned files so a recipe cannot forget.
fn writes_an_it(artifacts: &[Artifact]) -> bool {
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
    if !writes_a_test {
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
fn writes_a_test(artifacts: &[Artifact]) -> bool {
    artifacts
        .iter()
        .any(|a| a.path.to_string_lossy().contains("src/test/java"))
}

/// Splice a dependency into pom.xml unless it is already there.
///
/// Comment-preserving, like every other pom edit jails makes: the file
/// belongs to the reader, and a generator that reformats it has taken more
/// than it was asked for.
fn ensure_dependency(root: &Path, dep: &crate::pom::Dependency) -> Result<()> {
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
fn apply_build_change(root: &Path, pom: &str, change: &Change) -> Result<()> {
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

/// Spring-only generator kinds refuse politely rather than writing code that
/// cannot compile.
fn require_spring_project(project: &Project, kind: &str) -> Result<()> {
    crate::spring::require_spring(project.flavor(), kind)
}

/// `com.example.demo` + `domain` -> `com.example.demo.domain`. An empty
/// subpackage leaves the base package alone.
pub(crate) fn subpackage(base: &str, sub: &str) -> String {
    if sub.is_empty() {
        base.to_string()
    } else {
        format!("{base}.{sub}")
    }
}

fn pkg_dir(pkg: &str) -> String {
    pkg.replace('.', "/")
}

pub(crate) fn main_dir(root: &Path, pkg: &str) -> PathBuf {
    root.join("src/main/java").join(pkg_dir(pkg))
}

pub(crate) fn test_dir(root: &Path, pkg: &str) -> PathBuf {
    root.join("src/test/java").join(pkg_dir(pkg))
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
    // The refusal stays here rather than in `apply::create`, because this is
    // the one a person reads: it names the three ways forward. `apply::create`
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
fn ensure_package_info(root: &Path, class_path: &Path) -> Result<()> {
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
    if !jspecify_available(root) {
        return Ok(());
    }
    let Some(pkg) = package_of_dir(root, dir) else {
        return Ok(());
    };
    crate::apply::put(&info, package_info_java(&pkg))?;
    Ok(())
}

fn package_info_java(pkg: &str) -> String {
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
fn planned_package_infos(root: &Path, artifacts: &[Artifact]) -> Vec<Artifact> {
    if !jspecify_available(root) {
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
fn jspecify_available(root: &Path) -> bool {
    crate::pom::read(root)
        .map(|pom| crate::pom::has_dependency(&pom, "org.jspecify", "jspecify"))
        .unwrap_or(false)
}

/// The package name for a directory under `src/main/java`.
fn package_of_dir(root: &Path, dir: &Path) -> Option<String> {
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

pub fn generate_with_timestamps(
    kind: ArtifactKind,
    name: &str,
    fields: &[String],
    timestamps: bool,
    package: Option<&str>,
    indexes: &[String],
    strategy_on: Option<&str>,
    strategy_yields: Option<&str>,
    pretend: bool,
) -> Result<()> {
    let project = Project::discover()?;
    generate_in_project(
        &project,
        kind,
        name,
        fields,
        timestamps,
        package,
        indexes,
        strategy_on,
        strategy_yields,
        pretend,
    )
}

/// Generate against an explicitly resolved project.
///
/// App reconciliation uses this to render old and new intents in isolated
/// project copies without mutating process-global cwd. The ordinary CLI path
/// resolves the same value once in [`generate_with_timestamps`].
pub(crate) fn generate_in_project(
    project: &Project,
    kind: ArtifactKind,
    name: &str,
    fields: &[String],
    timestamps: bool,
    package: Option<&str>,
    indexes: &[String],
    strategy_on: Option<&str>,
    strategy_yields: Option<&str>,
    pretend: bool,
) -> Result<()> {
    let root = project.root().to_path_buf();
    let base = project.base().to_string();

    let expanded_fields;
    let fields = if timestamps {
        if !matches!(kind, ArtifactKind::Scaffold) {
            return Err(
                "--timestamps belongs to scaffold, where the record, DDL, adapter, and HTTP contracts can evolve together.\n       \
                 fix: use `jails g scaffold <Name> ... --timestamps`."
                    .to_string(),
            );
        }
        let parsed = parse_fields(fields)?;
        for conventional in ["createdAt", "updatedAt"] {
            if parsed.iter().any(|field| field.name == conventional) {
                return Err(format!(
                    "--timestamps would duplicate `{conventional}`.\n       \
                     fix: remove the hand-declared timestamp or omit --timestamps."
                ));
            }
        }
        expanded_fields = fields
            .iter()
            .cloned()
            .chain([
                "createdAt:instant".to_string(),
                "updatedAt:instant".to_string(),
            ])
            .collect::<Vec<_>>();
        expanded_fields.as_slice()
    } else {
        fields
    };

    if matches!(kind, ArtifactKind::Field) {
        if !indexes.is_empty() || strategy_on.is_some() || strategy_yields.is_some() {
            return Err(
                "field accepts one `name:type` component; --index/--on/--yields do not apply.\n       \
                 fix: put @index on the field itself, for example `createdAt:instant@index`."
                    .to_string(),
            );
        }
        return generate_field(project, &capitalize(name), fields, package, pretend);
    }

    // These kinds use NAME as a path/description rather than a Java class
    // name. Handle them before the shared capitalisation below.
    if matches!(kind, ArtifactKind::Cases) {
        return generate_cases(
            &root,
            &subpackage(&base, package.unwrap_or("")),
            Path::new(name),
            pretend,
        );
    }
    if matches!(kind, ArtifactKind::Migration) {
        return generate_migration(&root, name, pretend);
    }

    let name = strip_redundant_suffix(kind, &capitalize(name));
    let artifacts = artifacts_for(
        project,
        &Recipe {
            kind,
            name: &name,
            fields,
            indexes,
            strategy_on,
            strategy_yields,
        },
        package,
    )?;

    // Every write this command performs, in one list, before any of it is
    // previewed or applied. `package-info.java` used to be created as a side
    // effect of writing a class, so `--pretend` named two files and the real
    // run wrote three.
    let mut artifacts = artifacts;
    let mut planned = planned_package_infos(&root, &artifacts);
    if !planned.is_empty() {
        planned.append(&mut artifacts);
        artifacts = planned;
    }

    let mut change = Change {
        files: artifacts,
        ..Change::default()
    };
    if writes_a_test(&change.files)
        && !crate::pom::has_dependency(project.pom(), "org.assertj", "assertj-core")
        && !project.pom().contains("spring-boot-starter-test")
        && !project.pom().contains("spring-boot-starter-webmvc-test")
    {
        change.deps.push(crate::pom::assertj(project.flavor()));
    }
    match kind {
        ArtifactKind::Dto | ArtifactKind::Scaffold => change
            .deps
            .push(*crate::spring::validation_dependency(project.flavor())),
        ArtifactKind::Client => change.deps.push(crate::spring::RESTCLIENT_STARTER),
        ArtifactKind::Fetcher => change.deps.extend([
            crate::spring::APACHE_HTTPCLIENT,
            crate::spring::ACTUATOR_STARTER,
        ]),
        ArtifactKind::Event => change.deps.extend([
            crate::spring::TESTCONTAINERS_KAFKA,
            crate::spring::SPRING_TESTCONTAINERS,
        ]),
        _ => {}
    }
    if writes_an_it(&change.files) {
        change.plugins.push((
            crate::spring::FAILSAFE_ARTIFACT,
            crate::spring::failsafe_plugin(project.flavor()).to_string(),
        ));
    }

    for artifact in &change.files {
        if artifact.path.exists()
            && !(artifact.kind == "scheduling"
                && fs::read_to_string(&artifact.path)
                    .is_ok_and(|source| source == artifact.contents))
        {
            return Err(format!(
                "{} already exists.\n       fix: choose a different name, destroy the generated artifact first, or use `jails g field` to evolve an existing model.",
                artifact.path.display()
            ));
        }
    }
    // `--pretend` still runs every check above, so a run that would have
    // collided reports the collision rather than a clean-looking plan.
    if pretend {
        for artifact in &change.files {
            println!("would create {} {}", artifact.kind, artifact.path.display());
        }
        if matches!(kind, ArtifactKind::Command) {
            println!("would register {name} in the project's command dispatcher");
        }
        for dep in &change.deps {
            println!(
                "would ensure dependency {}:{}",
                dep.group_id, dep.artifact_id
            );
        }
        for (artifact_id, _) in &change.plugins {
            println!("would ensure plugin {artifact_id}");
        }
        println!();
        println!("--pretend: nothing was written.");
        return Ok(());
    }
    let mut written = Vec::new();
    for artifact in &change.files {
        if artifact.path.exists() && artifact.kind == "scheduling" {
            println!("exists scheduling {}", artifact.path.display());
            continue;
        }
        write_new_file(&root, &artifact.path, &artifact.contents)?;
        println!("created {} {}", artifact.kind, artifact.path.display());
        written.push(artifact.path.clone());
    }

    let kind_key = kind
        .to_possible_value()
        .expect("every ArtifactKind has a clap value")
        .get_name()
        .to_string();
    crate::generated_files::record(&root, &kind_key, &name, package, &written)?;
    if matches!(kind, ArtifactKind::Record | ArtifactKind::Scaffold) && !fields.is_empty() {
        crate::generated_files::record_model(&root, &name, package, fields)?;
    }

    if matches!(kind, ArtifactKind::Command) {
        register_command(&root, &base, &name, strategy_on)?;
    }
    // A generator that emits code needing a dependency has to supply it.
    // The alternative is handing the reader a compile error for a line they
    // did not write, which is exactly the plumbing this tool exists to
    // remove. Splicing is idempotent -- pom.rs reports when it is already
    // there and changes nothing.
    apply_build_change(&root, project.pom(), &change)?;
    Ok(())
}

#[cfg(test)]
fn generate(
    kind: ArtifactKind,
    name: &str,
    fields: &[String],
    package: Option<&str>,
    indexes: &[String],
    strategy_on: Option<&str>,
    strategy_yields: Option<&str>,
    pretend: bool,
) -> Result<()> {
    generate_with_timestamps(
        kind,
        name,
        fields,
        false,
        package,
        indexes,
        strategy_on,
        strategy_yields,
        pretend,
    )
}

/// An `import` line for `{from}.{class}`, or nothing at all when the two
/// packages are the same -- importing a sibling is a compile error.
pub(crate) fn import_of(user: &str, owner: &str, class: &str) -> String {
    if user == owner {
        String::new()
    } else {
        format!("import {owner}.{class};\n")
    }
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

/// The files one `generate` call would write, computed without writing any.
///
/// `abstract.md` rungs 4-5, Separate Query from Modifier. `destroy` used to
/// read `KIND_FILES`: 672 lines transcribing by hand the paths this match
/// right next door already computes. Two transcriptions of one fact drift, and
/// `tests/agreement.rs` existed to police the drift -- which §9 calls a receipt
/// for a decision not made. The decision is made here: there is one list, and
/// `destroy` asks for it.
///
/// **Contents are computed too, and thrown away by `destroy`.** That is the
/// cost, and it is the right trade: a query that returned only paths would be
/// a *second* traversal of the same match, which is the duplication this
/// removes wearing a different hat.
fn artifacts_for(
    project: &Project,
    recipe: &Recipe<'_>,
    package: Option<&str>,
) -> Result<Vec<Artifact>> {
    let root = project.root().to_path_buf();
    let base = project.base().to_string();
    let config = project.layers();
    // `--package` replaces the conventional home for every artifact in this
    // call; without it each kind goes where its convention says.
    let place = |default: &str| project.package_named(default, package);
    let Recipe {
        name,
        fields,
        indexes,
        strategy_on,
        strategy_yields,
        ..
    } = *recipe;

    let artifacts = match recipe.kind {
        ArtifactKind::Scaffold => scaffold_artifacts(
            &crate::model::Slice::new(project, package),
            name,
            fields,
            indexes,
        )?,
        ArtifactKind::Controller => {
            let web = place(layout::WEB);
            vec![
                Artifact {
                    kind: "controller",
                    path: main_dir(&root, &web).join(format!("{name}Controller.java")),
                    contents: stub_controller(&web, name),
                },
                Artifact {
                    kind: "controller test",
                    path: test_dir(&root, &web).join(format!("{name}ControllerTest.java")),
                    contents: controller_stub_test(
                        &web,
                        name,
                        project.mockmvc_autoconfigure_import(),
                    ),
                },
            ]
        }
        ArtifactKind::Service => {
            let service = place(layout::SERVICE);
            vec![
                Artifact {
                    kind: "service",
                    path: main_dir(&root, &service).join(format!("{name}Service.java")),
                    contents: stub_service(&service, name),
                },
                Artifact {
                    kind: "service test",
                    path: test_dir(&root, &service).join(format!("{name}ServiceTest.java")),
                    contents: service_stub_test(&service, name),
                },
            ]
        }
        // The layer-less kind: a plain class and its test, in the base package
        // rather than a subpackage, because "a class" says nothing about which
        // layer owns it. Everything else here has a conventional home; this is
        // the one for ordinary Java -- an algorithm, a ring buffer, a parser.
        ArtifactKind::Class => {
            let pkg = place("");
            vec![
                Artifact {
                    kind: "class",
                    path: main_dir(&root, &pkg).join(format!("{name}.java")),
                    contents: stub_class(&pkg, name),
                },
                Artifact {
                    kind: "class test",
                    path: test_dir(&root, &pkg).join(format!("{name}Test.java")),
                    contents: class_test(&pkg, name),
                },
            ]
        }
        ArtifactKind::Interface => {
            let pkg = place("");
            vec![Artifact {
                kind: "interface",
                path: main_dir(&root, &pkg).join(format!("{name}.java")),
                contents: interface_java(&pkg, name),
            }]
        }
        // Spring-only kinds. The templates live in spring.rs, next to the
        // capabilities that share their Spring Boot 4 assumptions.
        ArtifactKind::Client => {
            require_spring_project(project, "client")?;
            crate::spring::client_files(&crate::model::Slice::new(project, package), name)
        }
        ArtifactKind::Fetcher => {
            require_spring_project(project, "fetcher")?;
            if !fields.is_empty() || strategy_on.is_some() || strategy_yields.is_some() {
                return Err(
                    "fetcher takes only a name; limits and policy are external configuration"
                        .to_string(),
                );
            }
            crate::spring::fetcher_files(&crate::model::Slice::new(project, package), name)
        }
        ArtifactKind::Job => {
            require_spring_project(project, "job")?;
            crate::spring::job_files(&crate::model::Slice::new(project, package), name)
        }
        ArtifactKind::HttpWorkflow => {
            require_spring_project(project, "http-workflow")?;
            if !fields.is_empty() || strategy_yields.is_some() {
                return Err(
                    "http-workflow takes a name and `--on <Fetcher>`; bounds are request/configuration data"
                        .to_string(),
                );
            }
            let fetcher = strategy_on.ok_or_else(|| {
                format!(
                    "http-workflow {name} needs the safe fetcher it composes.\n       fix: pass `--on <Fetcher>`, for example `--on Page`."
                )
            })?;
            crate::spring::http_workflow_files(
                &crate::model::Slice::new(project, package),
                name,
                &strip_redundant_suffix(ArtifactKind::Fetcher, &capitalize(fetcher)),
            )?
        }
        ArtifactKind::Association => {
            require_spring_project(project, "association")?;
            if fields.is_empty() {
                return Err(format!(
                    "association {name} needs at least one `childField=parentField` mapping"
                ));
            }
            let child = strategy_on.ok_or_else(|| {
                format!(
                    "association {name} needs its child resource.\n       fix: pass `--on <Child>`."
                )
            })?;
            let parent = strategy_yields.ok_or_else(|| {
                format!(
                    "association {name} needs its parent resource.\n       fix: pass `--yields <Parent>`."
                )
            })?;
            crate::spring::association_files(
                &crate::model::Slice::new(project, package),
                name,
                &capitalize(child),
                &capitalize(parent),
                fields,
            )?
        }
        ArtifactKind::Idempotency => {
            require_spring_project(project, "idempotency")?;
            if !fields.is_empty() || strategy_on.is_some() || strategy_yields.is_some() {
                return Err(
                    "idempotency takes only a name; the scope, key and request bytes are \
                     runtime values the caller supplies, not generation-time ones"
                        .to_string(),
                );
            }
            crate::spring::idempotency_files(&crate::model::Slice::new(project, package), name)?
        }
        ArtifactKind::HttpSink => {
            require_spring_project(project, "http-sink")?;
            if !fields.is_empty() {
                return Err(
                    "http-sink payloads come from the typed outbox event; do not repeat fields"
                        .to_string(),
                );
            }
            let usecase = strategy_on.ok_or_else(|| {
                format!(
                    "http-sink {name} needs its transactional outbox use case.\n       fix: pass `--on <UseCase>`."
                )
            })?;
            let event = strategy_yields.ok_or_else(|| {
                format!(
                    "http-sink {name} needs the typed event it delivers.\n       fix: pass `--yields <Event>`."
                )
            })?;
            crate::spring::http_sink_files(
                &crate::model::Slice::new(project, package),
                name,
                &capitalize(usecase),
                &capitalize(event),
            )?
        }
        ArtifactKind::DurableJob => {
            require_spring_project(project, "durable-job")?;
            let usecase = strategy_on.ok_or_else(|| {
                format!(
                    "durable-job {name} needs the create use case it invokes.\n       fix: pass `--on <UseCase>`, for example `--on ProcessTask`."
                )
            })?;
            let target = strategy_yields.ok_or_else(|| {
                format!(
                    "durable-job {name} needs the resource that proves completion.\n       fix: pass `--yields <Resource>`, for example `--yields Task`."
                )
            })?;
            let slice = crate::model::Slice::new(project, package);
            let parsed = parse_fields(fields)?;
            crate::spring::durable_job_files(
                &slice,
                name,
                &capitalize(usecase),
                &capitalize(target),
                &parsed,
            )?
        }
        ArtifactKind::Usecase => {
            require_spring_project(project, "usecase")?;
            let target = strategy_on.ok_or_else(|| {
                format!(
                    "usecase {name} needs the resource it creates.\n       fix: pass `--on <Resource>`, for example `jails g usecase {name} title:string --on Task`."
                )
            })?;
            // `--package` places the operation itself. The target resource
            // already exists in the project's configured scaffold layers;
            // moving the operation must not make Jails look for a second copy
            // of that resource in the override package. `Slice` owns that rule
            // now, so no call site restates it.
            let slice = crate::model::Slice::new(project, package);
            let parsed = parse_fields(fields)?;
            let mut files =
                crate::spring::usecase_files(&slice, name, &capitalize(target), &parsed)?;
            if let Some(event) = strategy_yields {
                files.extend(crate::spring::outbox_files(
                    &slice,
                    name,
                    &capitalize(target),
                    &capitalize(event),
                    &parsed,
                )?);
            }
            files
        }
        ArtifactKind::Query => {
            require_spring_project(project, "query")?;
            let target = strategy_on.ok_or_else(|| {
                format!(
                    "query {name} needs the resource it reads.\n       fix: pass `--on <Resource>`, for example `jails g query {name} status:TaskStatus --on Task`."
                )
            })?;
            if strategy_yields.is_some() {
                return Err(
                    "`--yields` is not valid for a query; queries return the target resource"
                        .to_string(),
                );
            }
            let slice = crate::model::Slice::new(project, package);
            let parsed = parse_fields(fields)?;
            crate::spring::query_files(&slice, name, &capitalize(target), &parsed)?
        }
        ArtifactKind::Transition => {
            require_spring_project(project, "transition")?;
            let target = strategy_on.ok_or_else(|| {
                format!(
                    "transition {name} needs the resource it updates.\n       fix: pass `--on <Resource>`, for example `jails g transition {name} id:uuid tenantId:uuid@scope status:TaskStatus version:long --on Task`."
                )
            })?;
            if strategy_yields.is_some() {
                return Err(
                    "`--yields` is not valid for a transition; transitions return the updated target resource"
                        .to_string(),
                );
            }
            let slice = crate::model::Slice::new(project, package);
            let parsed = parse_fields(fields)?;
            crate::spring::transition_files(&slice, name, &capitalize(target), &parsed)?
        }
        ArtifactKind::Event => {
            require_spring_project(project, "event")?;
            let parsed = parse_fields(fields)?;
            crate::spring::event_files(&crate::model::Slice::new(project, package), name, &parsed)?
        }
        ArtifactKind::Dto => {
            let domain = place(layout::DOMAIN);
            let (components, _) = fields_from_spec_or_record(&root, &domain, name, fields)?;
            crate::spring::dto_files(
                &crate::model::Slice::new(project, package),
                name,
                &components,
            )
        }
        ArtifactKind::Record => {
            let parsed = parse_fields(fields)?;
            let domain = place(layout::DOMAIN);
            vec![
                Artifact {
                    kind: "record",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: record_java(&domain, name, &parsed),
                },
                Artifact {
                    kind: "record test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: record_test(&root, &domain, name, &parsed),
                },
            ]
        }
        ArtifactKind::Field => unreachable!("handled above -- it updates an existing model"),
        ArtifactKind::Factory => {
            if !fields.is_empty() {
                return Err(format!(
                    "factory {name} reads the existing record and takes no field spec.\n       \
                     fix: run `jails g factory {name}`."
                ));
            }
            let domain = subpackage(&base, config.layer(layout::DOMAIN));
            let testkit = place(layout::TESTKIT);
            let components = fields_from_record(&root, &domain, name).ok_or_else(|| {
                format!(
                    "no {name} record found under {domain}.\n       \
                     fix: generate the record/scaffold first, then run `jails g factory {name}`."
                )
            })?;
            vec![Artifact {
                kind: "test factory",
                path: test_dir(&root, &testkit).join(format!("{name}Factory.java")),
                contents: factory_java(&root, &testkit, &domain, name, &components),
            }]
        }
        ArtifactKind::Value => {
            let parsed = parse_fields(fields)?;
            if parsed.is_empty() {
                return Err("a value type needs at least one field, e.g. `generate value Money amount:long`".to_string());
            }
            let domain = place(layout::DOMAIN);
            vec![
                Artifact {
                    kind: "value",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: value_java(&domain, name, &parsed),
                },
                Artifact {
                    kind: "value test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: value_test(&root, &domain, name, &parsed),
                },
            ]
        }
        ArtifactKind::Enum => {
            let constants = parse_constants(fields)?;
            let domain = place(layout::DOMAIN);
            vec![
                Artifact {
                    kind: "enum",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: enum_java(&domain, name, &constants),
                },
                Artifact {
                    kind: "enum test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: enum_test(&domain, name, &constants),
                },
            ]
        }
        ArtifactKind::Repo => {
            let app = place(layout::APP);
            let adapters = place(layout::ADAPTERS);
            let domain = place(layout::DOMAIN);
            let mut artifacts = Vec::new();
            // One source rule shared with scaffold/dto: explicit fields,
            // otherwise the record on disk, otherwise a refusal that names
            // the fix. A TODO-shaped adapter silently loses data.
            let (record_fields, _) = fields_from_spec_or_record(&root, &domain, name, fields)?;

            // A repository of a type that does not exist is meaningless, and
            // the port would not compile. Rather than fail, lay down the
            // smallest record that could be one -- it is a starting point the
            // reader will obviously edit, the same way `scaffold` works.
            if !main_dir(&root, &domain)
                .join(format!("{name}.java"))
                .exists()
            {
                let id = if record_fields.is_empty() {
                    parse_fields(&["id:string!".to_string()])?
                } else {
                    record_fields.clone()
                };
                artifacts.push(Artifact {
                    kind: "record (placeholder for the repository)",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: record_java(&domain, name, &id),
                });
                artifacts.push(Artifact {
                    kind: "record test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: record_test(&root, &domain, name, &id),
                });
            }

            artifacts.push(Artifact {
                kind: "repository port",
                path: main_dir(&root, &app).join(format!("{name}Repository.java")),
                contents: repository_port(&app, name, &import_of(&app, &domain, name)),
            });
            artifacts.push(Artifact {
                kind: "JDBC adapter",
                path: main_dir(&root, &adapters).join(format!("Jdbc{name}Repository.java")),
                contents: jdbc_repository_for(
                    project,
                    &adapters,
                    name,
                    &format!(
                        "{}{}",
                        import_of(&adapters, &domain, name),
                        import_of(&adapters, &app, &format!("{name}Repository"))
                    ),
                    &crate::sql::columns(&record_fields, project, &domain, &lower_first(name)),
                    &domain,
                ),
            });
            artifacts.push(Artifact {
                kind: "JDBC adapter integration test",
                path: test_dir(&root, &adapters).join(format!("Jdbc{name}RepositoryIT.java")),
                contents: jdbc_repository_test(&adapters, name),
            });
            artifacts
        }
        ArtifactKind::Handler => {
            let api = place(layout::API);
            let domain = place(layout::DOMAIN);
            let mut artifacts = Vec::new();

            // Every handler renders failures through the same envelope, so the
            // first one lays it down and the rest reuse it.
            if !main_dir(&root, &domain).join("ApiError.java").exists() {
                let fields = parse_fields(&[
                    "code:string!".to_string(),
                    "message:string!".to_string(),
                    "details:map<string,string>".to_string(),
                ])?;
                artifacts.push(Artifact {
                    kind: "error envelope",
                    path: main_dir(&root, &domain).join("ApiError.java"),
                    contents: value_java(&domain, "ApiError", &fields),
                });
                artifacts.push(Artifact {
                    kind: "error envelope test",
                    path: test_dir(&root, &domain).join("ApiErrorTest.java"),
                    contents: value_test(&root, &domain, "ApiError", &fields),
                });
            }

            artifacts.push(Artifact {
                kind: "handler",
                path: main_dir(&root, &api).join(format!("{name}Handler.java")),
                contents: handler_java(&api, name, &import_of(&api, &domain, "ApiError")),
            });
            artifacts.push(Artifact {
                kind: "handler test",
                path: test_dir(&root, &api).join(format!("{name}HandlerTest.java")),
                contents: handler_test(&api, name),
            });
            artifacts
        }
        ArtifactKind::Sealed => {
            let variants = parse_variants(fields)?;
            let domain = place(layout::DOMAIN);
            vec![
                Artifact {
                    kind: "sealed type",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: sealed_java(&domain, name, &variants),
                },
                Artifact {
                    kind: "sealed type test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: sealed_test(&domain, name, &variants),
                },
            ]
        }
        ArtifactKind::Strategy => {
            let variants = parse_variants(fields)?;
            let domain = place(layout::DOMAIN);
            let on = strategy_on.ok_or_else(|| {
                format!(
                    "`generate strategy` needs the type the strategy examines, e.g. \
                     `jails g strategy {name} Coffee Large --on Transaction --yields Reward`.\n\n\
                     Without it jails would have to invent the one method every \
                     implementation overrides, and every implementation would then have \
                     to be rewritten."
                )
            })?;
            let spring = matches!(
                crate::pom::read(&root).map(|p| crate::pom::flavor(&p)),
                Ok(crate::pom::Flavor::SpringBoot)
            );
            // The generated signature names types jails did not write. If one
            // is not in the project yet, say so here rather than letting the
            // next `mvn` be what tells you -- a compile error for a line you
            // did not write is the plumbing this tool exists to remove.
            for missing in missing_types(&root, [Some(on), strategy_yields]) {
                println!(
                    "note: {missing} is not in this project yet -- \
                     `jails g record {missing} <field:type ...>` writes one"
                );
            }
            let mut artifacts = vec![Artifact {
                kind: "strategy",
                path: main_dir(&root, &domain).join(format!("{name}.java")),
                contents: strategy_interface_java(&domain, name, &variants, on, strategy_yields),
            }];
            for variant in &variants {
                let class = strategy_class(variant, name);
                artifacts.push(Artifact {
                    kind: "strategy implementation",
                    path: main_dir(&root, &domain).join(format!("{class}.java")),
                    contents: strategy_impl_java(
                        &domain,
                        name,
                        &class,
                        on,
                        strategy_yields,
                        spring,
                    ),
                });
                artifacts.push(Artifact {
                    kind: "strategy implementation test",
                    path: test_dir(&root, &domain).join(format!("{class}Test.java")),
                    contents: strategy_impl_test(&domain, name, &class, on, strategy_yields),
                });
            }
            artifacts
        }
        ArtifactKind::Command => {
            let cli = project.package(Layer::Cli, package);
            vec![
                Artifact {
                    kind: "command",
                    path: project
                        .main(Layer::Cli, package)
                        .join(format!("{name}Command.java")),
                    contents: command_java(&cli, name),
                },
                Artifact {
                    kind: "command test",
                    path: project
                        .test(Layer::Cli, package)
                        .join(format!("{name}CommandTest.java")),
                    contents: command_test(&cli, name),
                },
            ]
        }
        ArtifactKind::Cli => {
            let cli = project.package(Layer::Cli, package);
            vec![
                Artifact {
                    kind: "cli",
                    path: project
                        .main(Layer::Cli, package)
                        .join(format!("{name}Cli.java")),
                    contents: cli_java(&cli, &format!("{name}Cli"), &name.to_lowercase()),
                },
                Artifact {
                    kind: "cli test",
                    path: project
                        .test(Layer::Cli, package)
                        .join(format!("{name}CliTest.java")),
                    contents: cli_test(&cli, &format!("{name}Cli")),
                },
            ]
        }
        ArtifactKind::Cases => unreachable!("handled above -- its NAME is a path, not a class"),
        ArtifactKind::Migration => unreachable!("handled above -- its NAME is a SQL description"),
        ArtifactKind::Test => {
            let pkg = place("");
            vec![Artifact {
                kind: "test",
                path: test_dir(&root, &pkg).join(format!("{name}Test.java")),
                contents: stub_test(&pkg, name),
            }]
        }
        ArtifactKind::IntegrationTest => {
            let pkg = place("");
            vec![Artifact {
                kind: "integration test",
                path: test_dir(&root, &pkg).join(format!("{name}IT.java")),
                contents: integration_test_java(&pkg, name),
            }]
        }
    };

    Ok(artifacts)
}

/// One `generate` invocation, as a value.
///
/// The loose arguments this replaces were `abstract.md` §2's Long Parameter
/// List at its worst: `generate`, `destroy` and `app apply` each passed the
/// same ones in the same order, so two `Option<&str>` slots swapped by mistake
/// still compiled.
pub(crate) struct Recipe<'a> {
    pub(crate) kind: ArtifactKind,
    pub(crate) name: &'a str,
    pub(crate) fields: &'a [String],
    pub(crate) indexes: &'a [String],
    pub(crate) strategy_on: Option<&'a str>,
    pub(crate) strategy_yields: Option<&'a str>,
}

/// The one command that spans layers, and so the only place that has to say
/// out loud which package each half of a vertical slice lives in -- and add
/// the imports that crossing those boundaries now costs.
fn scaffold_artifacts(
    slice: &crate::model::Slice,
    name: &str,
    fields: &[String],
    indexes: &[String],
) -> Result<Vec<Artifact>> {
    let root: &Path = slice.project().root();
    let domain = slice.placed(Layer::Domain);
    let (parsed, reusing_record) = fields_from_spec_or_record(root, &domain, name, fields)?;
    // The unmapped-component refusal deliberately lives in
    // `scaffold_artifacts_from_fields`, which reads the referenced record's
    // stored `@pk` and names the two commands that do the job. A generic
    // refusal here would shadow that one and teach nothing.
    scaffold_artifacts_from_fields(slice, name, &parsed, indexes, !reusing_record)
}

fn prepared_artifact_contents(path: &Path, contents: &str) -> String {
    if path
        .extension()
        .is_some_and(|extension| extension == "java")
    {
        tidy_blank_lines(&normalize_imports(contents))
    } else {
        contents.to_string()
    }
}

fn field_spec(field: &Field) -> String {
    let mut ty = field.java_type.clone();
    if let Some(inner) = ty
        .strip_prefix("List<")
        .and_then(|rest| rest.strip_suffix('>'))
    {
        ty = format!("list<{inner}>");
    } else if let Some(inner) = ty
        .strip_prefix("Map<")
        .and_then(|rest| rest.strip_suffix('>'))
    {
        ty = format!("map<{inner}>");
    }
    match field.optionality {
        Optionality::Nullable => ty.push('?'),
        Optionality::NonBlank => ty.push('!'),
        Optionality::Required => {}
    }
    format!("{}:{ty}", field.name)
}

fn stored_primary_key(
    root: &Path,
    type_name: &str,
    package: Option<&str>,
) -> Result<Option<Field>> {
    let spec = match crate::generated_files::model_fields(root, type_name, package)? {
        Some(spec) => Some(spec),
        None if package.is_some() => crate::generated_files::model_fields(root, type_name, None)?,
        None => None,
    };
    let Some(spec) = spec else {
        return Ok(None);
    };
    let fields = parse_fields(&spec)?;
    let mut keys = fields
        .into_iter()
        .filter(|field| field.constraints.primary_key);
    let first = keys.next();
    Ok(if first.is_some() && keys.next().is_none() {
        first
    } else {
        None
    })
}

fn generate_field(
    project: &Project,
    name: &str,
    fields: &[String],
    package: Option<&str>,
    pretend: bool,
) -> Result<()> {
    let root: &Path = project.root();
    let base: &str = project.base();
    let slice = crate::model::Slice::new(project, package);
    if fields.len() != 1 {
        return Err(format!(
            "field {name} needs exactly one `name:type` component, got {}.\n       \
             fix: run one field evolution at a time so each change gets its own migration.",
            fields.len()
        ));
    }

    let config = crate::config::Config::load(root)?;
    let domain = subpackage(base, package.unwrap_or(config.layer(layout::DOMAIN)));
    let stored = crate::generated_files::model_fields(root, name, package)?;
    let old_fields = if let Some(spec) = stored.as_ref() {
        parse_fields(spec)?
    } else {
        fields_from_record(root, &domain, name).ok_or_else(|| {
            format!(
                "no {name} record found under {domain}.\n       \
                 fix: generate the record/scaffold first, then run `jails g field {name} {}`.",
                fields[0]
            )
        })?
    };
    let mut added = parse_fields(fields)?;
    let field = added
        .pop()
        .ok_or_else(|| "field needs one non-empty field spec".to_string())?;
    if old_fields
        .iter()
        .any(|existing| existing.name == field.name)
    {
        return Err(format!(
            "{name} already has a `{}` component.\n       \
             fix: choose a new component name; removing or changing a field is a data migration and is not automated.",
            field.name
        ));
    }

    let new_column = crate::sql::columns(
        std::slice::from_ref(&field),
        project,
        &domain,
        &lower_first(name),
    )
    .pop()
    .expect("one field produces one SQL column");
    if !new_column.mapped() {
        return Err(format!(
            "{}:{} is a project type that cannot be persisted as one column.\n       \
             fix: generate an association when the type is another record, or use a built-in/enum type.",
            field.name, field.java_type
        ));
    }

    let mut new_fields = old_fields.clone();
    new_fields.push(field.clone());
    let old_artifacts = scaffold_artifacts_from_fields(&slice, name, &old_fields, &[], true)?;
    let new_artifacts = scaffold_artifacts_from_fields(&slice, name, &new_fields, &[], true)?;

    let mut updates: Vec<(&Path, String)> = Vec::new();
    let mut skipped: Vec<&Path> = Vec::new();
    for new in &new_artifacts {
        if new.kind == "migration" || !new.path.is_file() {
            continue;
        }
        let Some(old) = old_artifacts.iter().find(|old| old.path == new.path) else {
            continue;
        };
        let before = prepared_artifact_contents(&old.path, &old.contents);
        let after = prepared_artifact_contents(&new.path, &new.contents);
        if before == after {
            continue;
        }
        match fs::read_to_string(&new.path) {
            Ok(on_disk) if on_disk == before => updates.push((&new.path, after)),
            Ok(_) => skipped.push(&new.path),
            Err(error) => {
                return Err(format!("failed to read {}: {error}", new.path.display()));
            }
        }
    }

    let migration_dir = root.join("src/main/resources/db/migration");
    let migration = if migration_dir.is_dir() {
        let version = next_migration_version(&migration_dir)?;
        let path = migration_dir.join(format!(
            "V{version:03}__add_{}_to_{}.sql",
            new_column.name,
            crate::sql::table_name(name)
        ));
        if path.exists() {
            return Err(format!(
                "{} already exists.\n       fix: resolve the migration version collision and rerun the command.",
                path.display()
            ));
        }
        Some((path, crate::sql::add_column(name, &new_column)?))
    } else {
        None
    };

    for (path, _) in &updates {
        println!(
            "{} {}",
            if pretend { "would update" } else { "updated" },
            path.display()
        );
    }
    for path in &skipped {
        println!("skipped {} -- you have edited this file", path.display());
        if path
            .file_name()
            .is_some_and(|file| file.to_string_lossy() == format!("Jdbc{name}Repository.java"))
        {
            println!(
                "         add to the select/insert lists: {}",
                new_column.name
            );
            println!(
                "         bind: {}",
                new_column.write.as_deref().unwrap_or("the new component")
            );
        } else {
            println!(
                "         add component: {} {}",
                declared_type(&field),
                field.name
            );
        }
    }
    if let Some((path, _)) = &migration {
        println!(
            "{} migration {}",
            if pretend { "would create" } else { "created" },
            path.display()
        );
    }

    if pretend {
        println!();
        println!("--pretend: nothing was written.");
        return Ok(());
    }
    for (path, contents) in &updates {
        // `g field` only reaches here for derived files that still match what
        // jails would have written -- the ownership oracle already refused the
        // rest and printed a snippet. So this is jails replacing its own
        // output, which is what `replace` names.
        crate::apply::replace(path, contents)?;
    }
    let mut written = Vec::new();
    if let Some((path, contents)) = migration {
        write_new_file(root, &path, &contents)?;
        written.push(path);
    }

    let mut model_spec = stored.unwrap_or_else(|| old_fields.iter().map(field_spec).collect());
    model_spec.push(fields[0].clone());
    crate::generated_files::record_model(root, name, package, &model_spec)?;
    crate::generated_files::record(
        root,
        "field",
        &format!("{name}.{}", field.name),
        package,
        &written,
    )?;
    Ok(())
}

fn scaffold_artifacts_from_fields(
    slice: &crate::model::Slice,
    name: &str,
    parsed: &[Field],
    indexes: &[String],
    include_record: bool,
) -> Result<Vec<Artifact>> {
    let root: &Path = slice.project().root();
    let place = |layer| slice.placed(layer);
    let domain = place(Layer::Domain);
    let repository = place(Layer::App);
    let adapters = place(Layer::Adapters);
    let service = place(Layer::Service);
    let web = place(Layer::Web);

    crate::spring::require_scope_authorizer(slice, "scaffold", name, parsed)?;

    let domain_in = |user: &str| import_of(user, &domain, name);
    let columns = crate::sql::columns(parsed, slice.project(), &domain, &lower_first(name));
    for (field, column) in parsed.iter().zip(&columns) {
        if column.mapped() {
            continue;
        }
        if field.collection {
            return Err(format!(
                "scaffold {name} cannot persist `{}:{}`: a collection is not one column.\n       \
                 fix: generate a record for the element and model the relationship explicitly \
                 with `jails g association`.",
                field.name, field.java_type
            ));
        }
        if main_dir(root, &domain)
            .join(format!("{}.java", field.java_type))
            .is_file()
        {
            if let Some(key) = stored_primary_key(root, &field.java_type, slice.override_package())?
            {
                return Err(format!(
                    "{name}.{} has project record type {}, which cannot be persisted as one `{}` column.\n       \
                     fix: replace it with `{}:{}` and run `jails g association {name}{} {}={} --on {name} --yields {}`.",
                    field.name,
                    field.java_type,
                    column.sql_type,
                    field.name,
                    declared_type(&key),
                    capitalize(&field.name),
                    field.name,
                    key.name,
                    field.java_type
                ));
            }
            return Err(format!(
                "scaffold {name} cannot persist `{}:{}`: {} is a record in this project, but \
                 it has no single stored @pk to reference.\n       \
                 fix: give {} exactly one @pk component, or model the foreign key as a \
                 built-in scalar field and relate them with `jails g association`.",
                field.name, field.java_type, field.java_type, field.java_type
            ));
        }
        return Err(format!(
            "scaffold {name} cannot persist `{}:{}`: jails has no SQL/JDBC mapping for that \
             type.\n       \
             fix: use a built-in field type or an enum (stored by name), or generate {} as a \
             record and relate them with `jails g association`.",
            field.name, field.java_type, field.java_type
        ));
    }

    // The migration is emitted only when the project has somewhere to put
    // one -- `jails add db` creates db/migration, and a .sql file in a
    // project with no Flyway is dead weight nobody asked for. When it is
    // emitted it comes from the same column list as the adapter, which is
    // the point: a hand-written pair drifts (an `amount` column against an
    // `amount_minor` select), and one list cannot disagree with itself.
    let migration_dir = root.join("src/main/resources/db/migration");
    let mut artifacts = Vec::new();

    artifacts.push(Artifact {
        kind: "HTTP request collection",
        path: root
            .join("requests")
            .join(format!("{}.http", crate::sql::snake_case(name))),
        contents: scaffold_requests(root, &domain, name, parsed),
    });

    // A fixture file, on the same rule as the migration: only when the
    // project already has somewhere to put one. `new`/`new-cli` seed
    // src/test/resources/fixtures, and `add testkit` generates the
    // `Fixtures` loader that reads it -- so the file is live, not decoration.
    let fixtures_dir = root.join("src/test/resources/fixtures");
    if fixtures_dir.is_dir() && !columns.is_empty() {
        let table = crate::sql::table_name(name);
        let constant = |type_name: &str| first_enum_constant(root, &domain, type_name);
        artifacts.push(Artifact {
            kind: "fixture",
            path: fixtures_dir.join(format!("{table}.json")),
            contents: crate::sql::fixture_json(&columns, &constant),
        });
    }
    if migration_dir.is_dir() && !columns.is_empty() {
        let version = next_migration_version(&migration_dir)?;
        let table = crate::sql::table_name(name);
        // Checked before it is written: a typo here fails at `flyway migrate`
        // with "column does not exist", on whichever machine runs it first.
        for spec in indexes {
            crate::sql::validate_index(spec, &columns)?;
        }
        artifacts.push(Artifact {
            kind: "migration",
            path: migration_dir.join(format!("V{version:03}__create_{table}.sql")),
            contents: crate::sql::create_table(name, &columns, indexes),
        });
    }

    if include_record {
        artifacts.extend([
            Artifact {
                kind: "record",
                path: main_dir(root, &domain).join(format!("{name}.java")),
                contents: record_java(&domain, name, parsed),
            },
            Artifact {
                kind: "record test",
                path: test_dir(root, &domain).join(format!("{name}Test.java")),
                contents: record_test(root, &domain, name, parsed),
            },
        ]);
    }

    artifacts.extend(vec![
        Artifact {
            kind: "repository port",
            path: main_dir(root, &repository).join(format!("{name}Repository.java")),
            contents: repository_port(&repository, name, &domain_in(&repository)),
        },
        Artifact {
            kind: "JDBC adapter",
            path: main_dir(root, &adapters).join(format!("Jdbc{name}Repository.java")),
            contents: jdbc_repository_for(
                slice.project(),
                &adapters,
                name,
                &format!(
                    "{}{}",
                    domain_in(&adapters),
                    import_of(&adapters, &repository, &format!("{name}Repository"))
                ),
                // The record was just written from these same fields, so the
                // adapter and the type it maps cannot disagree.
                &crate::sql::columns(parsed, slice.project(), &domain, &lower_first(name)),
                &domain,
            ),
        },
        Artifact {
            kind: "JDBC adapter integration test",
            path: test_dir(root, &adapters).join(format!("Jdbc{name}RepositoryIT.java")),
            contents: jdbc_repository_test(&adapters, name),
        },
        Artifact {
            kind: "in-memory adapter",
            path: main_dir(root, &adapters).join(format!("InMemory{name}Repository.java")),
            contents: crate::spring::in_memory_repository_java(
                &adapters,
                name,
                &format!(
                    "{}{}",
                    domain_in(&adapters),
                    import_of(&adapters, &repository, &format!("{name}Repository"))
                ),
                parsed
                    .iter()
                    .find(|f| f.name == "id")
                    .map(|f| f.name.as_str()),
                repository_wiring(slice.project()) != RepositoryWiring::JdbcClientBean,
            ),
        },
        Artifact {
            kind: "request",
            path: main_dir(root, &web).join(format!("{name}Request.java")),
            contents: crate::spring::request_java_for(
                &web,
                name,
                parsed,
                &domain_in(&web),
                &domain,
            ),
        },
        Artifact {
            kind: "response",
            path: main_dir(root, &web).join(format!("{name}Response.java")),
            contents: crate::spring::response_java_for(
                &web,
                name,
                parsed,
                &domain_in(&web),
                &domain,
            ),
        },
        Artifact {
            kind: "service",
            path: main_dir(root, &service).join(format!("{name}Service.java")),
            contents: crate::spring::resource_service_java(
                &service,
                name,
                &format!(
                    "{}{}",
                    domain_in(&service),
                    import_of(&service, &repository, &format!("{name}Repository"))
                ),
            ),
        },
        Artifact {
            kind: "service test",
            path: test_dir(root, &service).join(format!("{name}ServiceTest.java")),
            contents: crate::spring::resource_service_test_java(
                &service,
                name,
                &import_of(&service, &repository, &format!("{name}Repository")),
            ),
        },
        Artifact {
            kind: "controller",
            path: main_dir(root, &web).join(format!("{name}Controller.java")),
            contents: crate::spring::resource_controller_java(
                slice,
                name,
                &format!(
                    "{}{}",
                    domain_in(&web),
                    import_of(&web, &service, &format!("{name}Service"))
                ),
                parsed.iter().any(|f| f.name == "id"),
                parsed,
            ),
        },
        Artifact {
            kind: "controller test",
            path: test_dir(root, &web).join(format!("{name}ControllerTest.java")),
            contents: crate::spring::resource_controller_test_java(
                slice,
                name,
                &import_of(&web, &service, &format!("{name}Service")),
                parsed,
            ),
        },
    ]);

    Ok(artifacts)
}

fn scaffold_requests(root: &Path, domain: &str, name: &str, fields: &[Field]) -> String {
    let body = fields
        .iter()
        .map(|field| {
            let value = if field.optionality == Optionality::Nullable {
                "null".to_string()
            } else if field.collection {
                if field.java_type.starts_with("Map") {
                    "{}".to_string()
                } else {
                    "[]".to_string()
                }
            } else {
                match field.java_type.as_str() {
                    "String" => format!("\"sample-{}\"", field.name),
                    "Integer" | "int" | "Long" | "long" | "Double" | "double" | "BigDecimal" => {
                        "1".to_string()
                    }
                    "Boolean" | "boolean" => "true".to_string(),
                    "UUID" => "\"00000000-0000-0000-0000-000000000001\"".to_string(),
                    "LocalDate" => "\"2026-01-01\"".to_string(),
                    "LocalDateTime" => "\"2026-01-01T00:00:00\"".to_string(),
                    "Instant" => "\"2026-01-01T00:00:00Z\"".to_string(),
                    other if field.owned => first_enum_constant(root, domain, other)
                        .map(|constant| format!("\"{constant}\""))
                        .unwrap_or_else(|| "null".to_string()),
                    _ => "null".to_string(),
                }
            };
            format!("  \"{}\": {value}", field.name)
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let route = resource_path(name);
    format!(
        "@baseUrl = http://localhost:8080\n\n\
         ### Create {name}\n\
         POST {{{{baseUrl}}}}{route}\n\
         Content-Type: application/json\n\n\
         {{\n{body}\n}}\n\n\
         ### List {name}\n\
         GET {{{{baseUrl}}}}{route}\n\
         Accept: application/json\n"
    )
}

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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::CWD_LOCK;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jails-generate-test-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The invariant that keeps a scaffold able to *start*: exactly one
    /// adapter is a bean. Two makes Spring refuse to choose; zero leaves the
    /// service with no repository at all.
    #[test]
    fn exactly_one_repository_adapter_carries_the_bean_annotation() {
        let columns = crate::sql::columns(
            &parse_fields(&["id:string!".to_string(), "title:string".to_string()]).unwrap(),
            &crate::model::Project::inspect(Path::new("/tmp/does-not-matter")).unwrap(),
            "com.example.app.domain",
            "note",
        );
        let jdbc_bean = jdbc_client_repository(
            "com.example.app.adapters",
            "Note",
            "",
            &columns,
            "com.example.app.domain",
        );
        let in_memory_fake = crate::spring::in_memory_repository_java(
            "com.example.app.adapters",
            "Note",
            "",
            Some("id"),
            false,
        );
        // The annotation on the declaration, not the word in the Javadoc.
        assert!(
            jdbc_bean.contains("@Component\npublic final class"),
            "{jdbc_bean}"
        );
        assert!(
            !in_memory_fake.contains("@Component\npublic class"),
            "the JDBC adapter is the bean here, so this one must not be: {in_memory_fake}"
        );
        assert!(
            !in_memory_fake.contains("import org.springframework.stereotype.Component;"),
            "an unused import would fail a strict build: {in_memory_fake}"
        );

        // ...and the other way round, before `add db` has run.
        let in_memory_bean = crate::spring::in_memory_repository_java(
            "com.example.app.adapters",
            "Note",
            "",
            Some("id"),
            true,
        );
        assert!(
            in_memory_bean.contains("@Component\npublic class"),
            "{in_memory_bean}"
        );
    }

    /// `spring.md` calls a positional `?` list in a multi-column insert a
    /// silent-swap bug waiting for a schema change, and the generator used to
    /// emit exactly that.
    #[test]
    fn the_spring_adapter_binds_by_name_and_shares_one_column_list() {
        let columns = crate::sql::columns(
            &parse_fields(&[
                "id:uuid".to_string(),
                "amount:long".to_string(),
                "currency:string".to_string(),
            ])
            .unwrap(),
            &crate::model::Project::inspect(Path::new("/tmp/does-not-matter")).unwrap(),
            "com.example.app.domain",
            "reward",
        );
        let src = jdbc_client_repository(
            "com.example.app.adapters",
            "Reward",
            "",
            &columns,
            "com.example.app.domain",
        );
        assert!(src.contains("JdbcClient"), "{src}");
        assert!(!src.contains("PreparedStatement"), "{src}");
        // Named, not positional.
        assert!(src.contains(".param(\"amount\""), "{src}");
        assert!(src.contains(":amount"), "{src}");
        assert!(!src.contains("setObject("), "{src}");
        // One column list, interpolated into the reads.
        assert!(src.contains("private static final String COLUMNS"), "{src}");
        assert!(src.contains(".formatted(COLUMNS)"), "{src}");
    }

    /// Typing the name the type will actually have is the obvious thing to
    /// do, and it used to produce `RewardHistoryServiceService.java`. A real
    /// project renamed four generated files by hand because of this.
    #[test]
    fn a_name_that_already_carries_its_kinds_suffix_does_not_get_it_twice() {
        for (kind, given, want) in [
            (
                ArtifactKind::Service,
                "RewardHistoryService",
                "RewardHistory",
            ),
            (ArtifactKind::Controller, "RewardController", "Reward"),
            (ArtifactKind::Repo, "RewardRepository", "Reward"),
            (ArtifactKind::Test, "MoneyTest", "Money"),
            (ArtifactKind::IntegrationTest, "QueueIT", "Queue"),
            (ArtifactKind::Job, "CleanupJob", "Cleanup"),
            (ArtifactKind::Client, "CatalogClient", "Catalog"),
            (ArtifactKind::Cli, "AdminCli", "Admin"),
        ] {
            assert_eq!(strip_redundant_suffix(kind, given), want, "{given}");
        }
    }

    #[test]
    fn a_name_without_the_suffix_is_left_alone() {
        assert_eq!(
            strip_redundant_suffix(ArtifactKind::Service, "RewardHistory"),
            "RewardHistory"
        );
        // `Repository` is matched whole -- `Rewards` does not lose its `s`.
        assert_eq!(
            strip_redundant_suffix(ArtifactKind::Repo, "Rewards"),
            "Rewards"
        );
    }

    /// Stripping the whole name would leave nothing to name the file after,
    /// so `g service Service` means a type called `Service`.
    #[test]
    fn a_name_that_is_only_the_suffix_survives() {
        assert_eq!(
            strip_redundant_suffix(ArtifactKind::Service, "Service"),
            "Service"
        );
        assert_eq!(strip_redundant_suffix(ArtifactKind::Test, "Test"), "Test");
    }

    /// `scaffold` spans Controller, Service and Repository at once; stripping
    /// any one of them would corrupt the other two.
    #[test]
    fn scaffold_and_record_use_the_name_verbatim() {
        assert_eq!(
            strip_redundant_suffix(ArtifactKind::Scaffold, "RewardService"),
            "RewardService"
        );
        assert_eq!(
            strip_redundant_suffix(ArtifactKind::Record, "RewardResponse"),
            "RewardResponse"
        );
    }

    #[test]
    fn capitalize_uppercases_first_letter_only() {
        assert_eq!(capitalize("post"), "Post");
        assert_eq!(capitalize("Post"), "Post");
        assert_eq!(capitalize(""), "");
    }

    #[test]
    fn field_type_maps_known_tokens() {
        assert_eq!(field_type("string").unwrap().0, "String");
        assert_eq!(field_type("text").unwrap(), ("String", None));
        assert_eq!(field_type("int").unwrap().0, "Integer");
        assert_eq!(field_type("integer").unwrap().0, "Integer");
        assert_eq!(field_type("long").unwrap().0, "Long");
        assert_eq!(field_type("boolean").unwrap().0, "Boolean");
        assert_eq!(field_type("double").unwrap().0, "Double");
        assert_eq!(
            field_type("uuid").unwrap(),
            ("UUID", Some("java.util.UUID"))
        );
        assert_eq!(
            field_type("currency").unwrap(),
            ("Currency", Some("java.util.Currency"))
        );
        assert_eq!(
            field_type("date").unwrap(),
            ("LocalDate", Some("java.time.LocalDate"))
        );
        assert_eq!(
            field_type("datetime").unwrap(),
            ("LocalDateTime", Some("java.time.LocalDateTime"))
        );
    }

    #[test]
    fn field_type_rejects_unknown_tokens() {
        assert!(field_type("nope").is_err());
    }

    #[test]
    fn column_markers_parse_in_any_order_and_combine() {
        let fields = parse_fields(&[
            "transactionId:uuid@pk".to_string(),
            "amount:long@positive@index".to_string(),
            "email:string!@unique".to_string(),
            "workspaceId:uuid@scope@index".to_string(),
        ])
        .unwrap();
        assert!(fields[0].constraints.primary_key);
        assert_eq!(fields[1].constraints.check, Some(NumericCheck::Positive));
        assert!(fields[1].constraints.indexed);
        assert!(fields[2].constraints.unique);
        assert!(fields[3].constraints.scoped);
        assert!(fields[3].constraints.indexed);
        // The markers do not disturb the type or the optionality suffix.
        assert_eq!(fields[0].java_type, "UUID");
        assert_eq!(fields[2].java_type, "String");
        assert_eq!(fields[2].optionality, Optionality::NonBlank);
    }

    /// A marker typo that parsed as "no constraint" would produce a schema
    /// quietly missing the primary key someone thought they had asked for --
    /// the exact failure this feature exists to prevent.
    #[test]
    fn an_unknown_column_marker_is_an_error_listing_the_real_ones() {
        let err = parse_fields(&["id:uuid@primary".to_string()]).unwrap_err();
        assert!(err.contains("@primary"), "{err}");
        assert!(err.contains("@pk"), "{err}");
    }

    /// `check (name > 0)` on a text column fails at `flyway migrate`, which is
    /// a slow and remote way to learn about a typo.
    #[test]
    fn a_numeric_check_on_a_non_numeric_column_is_rejected() {
        let err = parse_fields(&["name:string@positive".to_string()]).unwrap_err();
        assert!(err.contains("numeric"), "{err}");
        assert!(parse_fields(&["amount:long@positive".to_string()]).is_ok());
        assert!(parse_fields(&["price:decimal@nonnegative".to_string()]).is_ok());
    }

    #[test]
    fn a_nullable_primary_key_is_rejected() {
        let err = parse_fields(&["id:uuid?@pk".to_string()]).unwrap_err();
        assert!(err.contains("nullable"), "{err}");
    }

    #[test]
    fn a_field_with_no_markers_has_no_constraints() {
        let fields = parse_fields(&["title:string".to_string()]).unwrap();
        assert_eq!(fields[0].constraints, Constraints::default());
    }

    #[test]
    fn parse_fields_splits_name_and_type() {
        let fields = parse_fields(&["title:string".to_string(), "body:Text".to_string()]).unwrap();
        assert_eq!(fields[0].name, "title");
        assert_eq!(fields[0].java_type, "String");
        // Capitalised means "a type this project owns", so `Text` is no longer
        // the built-in -- that is the whole point of the rule.
        assert_eq!(fields[1].java_type, "Text");
        assert!(fields[1].owned);
        assert_eq!(
            parse_fields(&["body:text".to_string()]).unwrap()[0].java_type,
            "String"
        );
    }

    /// The Java spellings of the built-in types stay built-in: `id:String`
    /// must not be read as an unknown project type.
    #[test]
    fn parse_fields_treats_java_type_names_as_builtins() {
        let fields = parse_fields(&["id:String".to_string(), "on:LocalDate".to_string()]).unwrap();
        assert!(!fields[0].owned);
        assert_eq!(fields[0].java_type, "String");
        assert!(!fields[1].owned);
        assert!(fields[1].imports.contains(&"java.time.LocalDate"));
    }

    #[test]
    fn resource_path_is_kebab_case_and_plural() {
        assert_eq!(resource_path("WorkItem"), "/work-items");
        assert_eq!(resource_path("Import"), "/imports");
    }

    /// A handler binds, routes and maps outcomes to status codes -- and holds
    /// no rules, so the same service can be driven from the CLI.
    #[test]
    fn handler_maps_outcomes_to_status_codes() {
        let src = handler_java("com.example.demo.api", "WorkItem", "");

        assert!(src.contains("implements HttpHandler"), "{src}");
        assert!(src.contains(r#"PATH = "/work-items""#), "{src}");
        assert!(
            src.contains("private final Service service"),
            "the service is a dependency: {src}"
        );
        assert!(src.contains("error(404"), "{src}");
        assert!(
            src.contains("error(422"),
            "well-formed but rejected is not a 400: {src}"
        );
        assert!(
            src.contains("ApiError"),
            "failures share one envelope: {src}"
        );
        assert!(!src.contains("java.sql"), "no storage in a handler: {src}");
    }

    #[test]
    fn handler_test_drives_it_over_a_real_socket() {
        let test = handler_test("com.example.demo.api", "WorkItem");

        assert!(test.contains("java.net.http.HttpClient"), "{test}");
        assert!(
            test.contains("new InetSocketAddress(0)"),
            "an ephemeral port: {test}"
        );
        assert!(test.contains("isEqualTo(422)"), "{test}");
    }

    /// The whole point of a port: application code must be able to depend on
    /// it without dragging JDBC along -- including in the prose, since a
    /// reader grepping for java.sql should find only the adapter.
    #[test]
    fn repository_port_is_free_of_jdbc() {
        let src = repository_port(
            "com.example.demo.app",
            "Transaction",
            "import com.example.demo.domain.Transaction;\n",
        );

        assert!(
            src.contains("public interface TransactionRepository"),
            "{src}"
        );
        assert!(
            src.contains("Optional<Transaction> findById(String id)"),
            "{src}"
        );
        assert!(src.contains("List<Transaction> findAll()"), "{src}");
        assert!(!src.contains("java.sql"), "not even in a comment: {src}");
    }

    #[test]
    fn jdbc_adapter_uses_plain_jdbc_and_no_orm() {
        let src = jdbc_repository(
            "com.example.demo.adapters",
            "Transaction",
            "",
            &[],
            "com.example.demo.domain",
        );

        assert!(src.contains("implements TransactionRepository"), "{src}");
        assert!(src.contains("connection.prepareStatement"), "{src}");
        assert!(src.contains("try (var query"), "try-with-resources: {src}");
        assert!(
            src.contains("order by id"),
            "unordered findAll would flake a test: {src}"
        );
        assert!(
            src.contains("\"\"\""),
            "SQL should be visible in text blocks: {src}"
        );
        {
            let forbidden = "org.springframework";
            assert!(!src.contains(forbidden), "{forbidden} should not appear");
        }
    }

    /// jails cannot know the columns, so map/bind are TODOs -- and a test that
    /// asserts on a TODO is noise until they are written.
    #[test]
    fn jdbc_adapter_test_is_disabled_until_the_mapping_is_written() {
        let test = jdbc_repository_test("com.example.demo.adapters", "Transaction");

        assert!(test.contains("@Disabled"), "{test}");
        assert!(test.contains("class JdbcTransactionRepositoryIT"), "{test}");
        assert!(test.contains("roundTripsThroughTheRealDatabase"), "{test}");
    }

    #[test]
    fn sealed_emits_a_permits_clause_and_a_record_per_variant() {
        let variants = parse_variants(&["verified".to_string(), "timeout".to_string()]).unwrap();
        let src = sealed_java("com.example.demo", "VerificationResult", &variants);

        // Nested variants have to be named qualified in the permits clause.
        assert!(
            src.contains("permits VerificationResult.Verified, VerificationResult.Timeout"),
            "{src}"
        );
        assert!(
            src.contains("record Verified() implements VerificationResult"),
            "{src}"
        );
        assert!(
            src.contains("record Timeout() implements VerificationResult"),
            "{src}"
        );
    }

    /// The companion test switches without a `default`, so adding a variant
    /// breaks it at compile time -- which is the entire reason to seal a type.
    #[test]
    fn sealed_test_switches_exhaustively_without_a_default() {
        let variants = parse_variants(&["ok".to_string(), "failed".to_string()]).unwrap();
        let test = sealed_test("com.example.demo", "Result", &variants);

        assert!(test.contains("switch (result)"), "{test}");
        assert!(test.contains("case Result.Ok v ->"), "{test}");
        assert!(
            !test.contains("default ->"),
            "an exhaustive switch must not have a default: {test}"
        );
    }

    /// Typing the name the class will actually have is the obvious thing to
    /// do, and `g service RewardHistoryService` writing
    /// `RewardHistoryServiceService.java` is the bug that taught jails not to
    /// punish it. The same rule applies to a strategy's variants.
    #[test]
    fn a_strategy_variant_does_not_repeat_the_interface_name() {
        assert_eq!(strategy_class("Coffee", "RewardRule"), "CoffeeRewardRule");
        assert_eq!(
            strategy_class("CoffeeRewardRule", "RewardRule"),
            "CoffeeRewardRule"
        );
        // Never the whole name away: `g strategy Rule Rule` means a class
        // called `Rule`, not the empty string.
        assert_eq!(strategy_class("RewardRule", "RewardRule"), "RewardRule");
    }

    /// `--yields` is what decides the shape: with it the strategy answers
    /// "what does this earn?" and declines with an empty `Optional`, which is
    /// what lets every implementation see every input. Without it it is a
    /// predicate.
    #[test]
    fn a_strategy_yields_an_optional_and_a_bare_one_is_a_predicate() {
        let (ret, method, param) = strategy_method("Transaction", Some("Reward"));
        assert_eq!(ret, "Optional<Reward>");
        assert_eq!(method, "apply");
        assert_eq!(param, "Transaction transaction");

        let (ret, method, _) = strategy_method("Transaction", None);
        assert_eq!(ret, "boolean");
        assert_eq!(method, "matches");
    }

    /// The annotation is the whole reason the pattern works, and its absence
    /// is silent: without it the class is simply not in the `List<Port>`, so
    /// it never runs and nothing reports a problem. The generated Javadoc
    /// says so, because that is the only place a reader will find it.
    #[test]
    fn a_spring_strategy_implementation_is_a_bean_and_says_why() {
        let spring = strategy_impl_java(
            "com.example.demo.domain",
            "RewardRule",
            "CoffeeRewardRule",
            "Transaction",
            Some("Reward"),
            true,
        );
        assert!(spring.contains("@Component"), "{spring}");
        assert!(
            spring.contains("import org.springframework.stereotype.Component;"),
            "{spring}"
        );
        assert!(spring.contains("its absence is silent"), "{spring}");

        // A plain Maven project has no Spring on the classpath, so the
        // annotation would not resolve and the import would not compile.
        let plain = strategy_impl_java(
            "com.example.demo.domain",
            "RewardRule",
            "CoffeeRewardRule",
            "Transaction",
            Some("Reward"),
            false,
        );
        assert!(!plain.contains("@Component"), "{plain}");
        assert!(!plain.contains("springframework"), "{plain}");
    }

    /// `apply` + `s` reads `applys`. A generated test whose name is
    /// misspelled is the first thing anyone sees of the pattern.
    #[test]
    fn generated_strategy_test_names_are_english() {
        let yielding = strategy_impl_test(
            "d",
            "RewardRule",
            "CoffeeRewardRule",
            "Transaction",
            Some("Reward"),
        );
        assert!(
            yielding.contains("void grantsWhenTheTransactionQualifies()"),
            "{yielding}"
        );
        assert!(!yielding.contains("applys"), "{yielding}");

        let predicate =
            strategy_impl_test("d", "RewardRule", "CoffeeRewardRule", "Transaction", None);
        assert!(
            predicate.contains("void matchesWhenTheTransactionQualifies()"),
            "{predicate}"
        );

        // @Disabled, not a passing assertion over an unwritten class: it is
        // reported as skipped rather than counted green.
        assert!(yielding.contains("@Disabled"), "{yielding}");
    }

    #[test]
    fn parse_variants_rejects_unusable_names() {
        assert!(parse_variants(&[]).is_err());
        assert!(
            parse_variants(&["ok".to_string(), "Ok".to_string()]).is_err(),
            "duplicate after capitalising"
        );
        assert!(parse_variants(&["not a name".to_string()]).is_err());
    }

    #[test]
    fn parse_fields_resolves_collection_types() {
        let fields = parse_fields(&[
            "matched:list<Match>".to_string(),
            "ids:list<string>".to_string(),
            "rates:map<string,double>".to_string(),
            "at:instant".to_string(),
        ])
        .unwrap();

        assert_eq!(fields[0].java_type, "List<Match>");
        assert!(fields[0].collection);
        assert_eq!(fields[1].java_type, "List<String>");
        // Generics cannot hold a primitive, so the element is the wrapper.
        assert_eq!(fields[2].java_type, "Map<String, Double>");
        assert!(fields[2].imports.contains(&"java.util.Map"));
        assert_eq!(fields[3].java_type, "Instant");
        assert!(fields[3].imports.contains(&"java.time.Instant"));
    }

    #[test]
    fn parse_fields_rejects_malformed_collection_types() {
        // A bare `list` would otherwise become List<Object>, silently.
        assert!(parse_fields(&["items:list".to_string()]).is_err());
        assert!(parse_fields(&["items:list<nope>".to_string()]).is_err());
        assert!(parse_fields(&["items:map<string>".to_string()]).is_err());
        assert!(parse_fields(&["items:list<list<string>>".to_string()]).is_err());
        // A collection already models absence; `?` on one is a mistake.
        assert!(parse_fields(&["items:list<string>?".to_string()]).is_err());
    }

    /// A collection component must be copied (so the record is genuinely
    /// immutable) and default to empty (so no consumer has to null-check a
    /// bucket).
    #[test]
    fn collection_components_are_copied_and_default_to_empty() {
        let fields = parse_fields(&[
            "matched:list<Match>".to_string(),
            "rates:map<string,double>".to_string(),
        ])
        .unwrap();
        let src = value_java("com.example.demo", "Result", &fields);

        assert!(src.contains("List<Match> matched"), "{src}");
        assert!(
            src.contains("matched = matched == null ? List.of() : List.copyOf(matched);"),
            "{src}"
        );
        assert!(
            src.contains("rates = rates == null ? Map.of() : Map.copyOf(rates);"),
            "{src}"
        );
        assert!(
            !src.contains("requireNonNull(matched"),
            "a collection is defaulted, not rejected: {src}"
        );
    }

    #[test]
    fn parse_fields_reads_the_optionality_suffixes() {
        let fields = parse_fields(&[
            "id:string!".to_string(),
            "note:string?".to_string(),
            "name:string".to_string(),
            "source:SourceRef?".to_string(),
        ])
        .unwrap();
        assert_eq!(fields[0].optionality, Optionality::NonBlank);
        assert_eq!(fields[1].optionality, Optionality::Nullable);
        assert_eq!(fields[2].optionality, Optionality::Required);
        assert_eq!(fields[3].optionality, Optionality::Nullable);
        assert!(fields[3].owned);
        assert_eq!(fields[3].java_type, "SourceRef");
    }

    #[test]
    fn parse_fields_rejects_args_without_a_colon() {
        assert!(parse_fields(&["title".to_string()]).is_err());
    }

    #[test]
    fn find_project_root_walks_up_to_pom_xml() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("project-root");
        fs::write(root.join("pom.xml"), "<project/>").unwrap();
        let nested = root.join("src/main/java/com/example");
        fs::create_dir_all(&nested).unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&nested).unwrap();
        let found = find_project_root();
        std::env::set_current_dir(original_cwd).unwrap();

        assert_eq!(found.unwrap(), root);
    }

    #[test]
    fn base_package_reads_the_application_class_package() {
        let root = scratch("base-package");
        let src = root.join("src/main/java/com/example/blog");
        fs::create_dir_all(&src).unwrap();
        fs::write(
            src.join("BlogApplication.java"),
            "package com.example.blog;\n\npublic class BlogApplication {}\n",
        )
        .unwrap();

        assert_eq!(base_package(&root).unwrap(), "com.example.blog");
    }

    #[test]
    fn base_package_errors_without_an_application_class() {
        let root = scratch("no-application");
        fs::create_dir_all(root.join("src/main/java")).unwrap();
        assert!(base_package(&root).is_err());
    }

    #[test]
    fn mockmvc_import_picks_legacy_package_for_boot_3() {
        let root = scratch("boot3");
        fs::write(
            root.join("pom.xml"),
            "<parent><artifactId>spring-boot-starter-parent</artifactId><version>3.3.4</version></parent>",
        )
        .unwrap();
        assert_eq!(
            mockmvc_autoconfigure_import_for(spring_boot_major_of(
                &fs::read_to_string(root.join("pom.xml")).unwrap_or_default(),
            )),
            "org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc"
        );
    }

    #[test]
    fn mockmvc_import_picks_current_package_for_boot_4() {
        let root = scratch("boot4");
        fs::write(
            root.join("pom.xml"),
            "<parent><artifactId>spring-boot-starter-parent</artifactId><version>4.1.0</version></parent>",
        )
        .unwrap();
        assert_eq!(
            mockmvc_autoconfigure_import_for(spring_boot_major_of(
                &fs::read_to_string(root.join("pom.xml")).unwrap_or_default(),
            )),
            "org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc"
        );
    }

    #[test]
    fn mockmvc_import_defaults_to_legacy_when_pom_is_unreadable() {
        let root = scratch("no-pom");
        assert_eq!(
            mockmvc_autoconfigure_import_for(spring_boot_major_of(
                &fs::read_to_string(root.join("pom.xml")).unwrap_or_default(),
            )),
            "org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc"
        );
    }

    #[test]
    fn stub_class_emits_a_plain_final_class_with_no_framework_in_it() {
        let src = stub_class("gym", "MoneyMoved");

        assert_eq!(
            src, "package gym;\n\npublic final class MoneyMoved {\n}\n",
            "{src}"
        );
        for forbidden in ["@", "org.springframework", "record "] {
            assert!(
                !src.contains(forbidden),
                "{forbidden} should not appear in a plain class"
            );
        }
    }

    /// The companion test has to compile against the class jails just wrote,
    /// which means constructing it with the implicit no-arg constructor -- the
    /// only one a bare class has.
    #[test]
    fn class_test_constructs_the_class_it_accompanies() {
        let src = class_test("gym", "MoneyMoved");

        assert!(src.contains("class MoneyMovedTest {"), "{src}");
        assert!(
            src.contains("MoneyMoved moneyMoved = new MoneyMoved();"),
            "{src}"
        );
        assert!(src.contains("import org.junit.jupiter.api.Test;"), "{src}");
        // The three defects of the old `isNotNull()` body: it passed while
        // the class was broken, it counted as coverage, and it taught `null`
        // as a constructor argument.
        assert!(
            !src.contains("isNotNull"),
            "a test that passes over a broken class is worse than no test: {src}"
        );
        assert!(src.contains("@Disabled("), "{src}");
        assert!(
            src.contains("todo: state what MoneyMoved is supposed to do"),
            "the disabled reason has to say what to prove: {src}"
        );
    }

    #[test]
    fn record_java_emits_a_record_with_a_null_rejecting_compact_constructor() {
        let fields =
            parse_fields(&["amount:long".to_string(), "currency:string".to_string()]).unwrap();
        let src = record_java("com.example.demo", "Money", &fields);

        // Primitive components make null impossible for numeric/boolean values: a
        // `long` cannot be null, so it needs neither the box nor the check.
        assert!(
            src.contains("public record Money(long amount, String currency) {"),
            "{src}"
        );
        assert!(
            src.contains("public Money {"),
            "expected a compact constructor"
        );
        assert!(
            !src.contains("requireNonNull(amount"),
            "a primitive cannot be null"
        );
        assert!(src.contains(r#"Objects.requireNonNull(currency, "currency");"#));
        // Plain Java: no framework persistence annotations.
        for forbidden in ["@", "org.springframework"] {
            assert!(
                !src.contains(forbidden),
                "{forbidden} should not appear in a plain record"
            );
        }
    }

    /// A record whose components are all primitives cannot hold a null, so the
    /// compact constructor would be empty -- and an empty one is noise.
    #[test]
    fn record_java_omits_the_compact_constructor_when_every_component_is_primitive() {
        let fields = parse_fields(&["amount:long".to_string(), "count:int".to_string()]).unwrap();
        let src = record_java("com.example.demo", "Tally", &fields);

        assert!(
            src.contains("public record Tally(long amount, int count) {"),
            "{src}"
        );
        assert!(
            !src.contains("public Tally {"),
            "nothing to validate: {src}"
        );
        assert!(!src.contains("import java.util.Objects;"));
    }

    /// A no-field record has nothing to null-check, so the compact constructor
    /// (and the Objects import that only exists to serve it) must be omitted
    /// rather than emitted empty.
    #[test]
    fn record_java_omits_the_compact_constructor_when_there_are_no_fields() {
        let src = record_java("com.example.demo", "Marker", &[]);

        assert!(src.contains("public record Marker() {"));
        assert!(!src.contains("public Marker {"));
        assert!(!src.contains("import java.util.Objects;"));
    }

    #[test]
    fn record_java_sorts_time_imports_with_the_objects_import() {
        let fields = parse_fields(&["on:date".to_string()]).unwrap();
        let src = record_java("com.example.demo", "Entry", &fields);

        let time = src.find("import java.time.LocalDate;").unwrap();
        let objects = src.find("import java.util.Objects;").unwrap();
        assert!(time < objects, "java.time should sort before java.util");
    }

    /// The compact constructor's validation is real behaviour and can
    /// regress. An accessor round-trip cannot: it asserts that javac
    /// generated an accessor, which `java.md` §7 names as a thing not to
    /// test.
    #[test]
    fn record_test_pins_the_validation_and_not_the_accessors() {
        let fields =
            parse_fields(&["amount:long".to_string(), "currency:string".to_string()]).unwrap();
        let test = record_test(
            Path::new("/nonexistent"),
            "com.example.demo",
            "Money",
            &fields,
        );

        assert!(test.contains("class MoneyTest"));
        assert!(test.contains("assertThatNullPointerException()"));
        // `amount` is a primitive, so the null case has to target the first
        // *reference* component or the generated test would not compile.
        assert!(test.contains("new Money(1L, null)"), "{test}");

        assert!(
            !test.contains("accessorsReturnWhatWasConstructed"),
            "{test}"
        );
        assert!(
            !test.contains("assertThat(money.amount()).isEqualTo(1L);"),
            "testing the compiler: {test}"
        );
    }

    /// A record with nothing to validate has nothing honest to assert, so it
    /// says so rather than emitting a green tick over an unproven type.
    #[test]
    fn a_record_with_no_validation_gets_a_disabled_todo_rather_than_a_tick() {
        let fields = parse_fields(&["amount:long".to_string()]).unwrap();
        let test = record_test(
            Path::new("/nonexistent"),
            "com.example.demo",
            "Money",
            &fields,
        );
        assert!(test.contains("@Disabled("), "{test}");
        assert!(test.contains("todo: state what Money guarantees"), "{test}");
        assert!(
            test.contains("import org.junit.jupiter.api.Disabled;"),
            "{test}"
        );
        assert!(!test.contains("assertThatNullPointerException"), "{test}");
    }

    /// With no fields there is no null to reject, so the test that asserts the
    /// rejection would not compile -- it must not be emitted.
    #[test]
    fn record_test_skips_the_null_case_for_a_no_field_record() {
        let test = record_test(Path::new("/nonexistent"), "com.example.demo", "Marker", &[]);

        assert!(!test.contains("assertThatNullPointerException"));
        assert!(!test.contains(
            "import static org.assertj.core.api.Assertions.assertThatNullPointerException;"
        ));
        assert!(test.contains("new Marker()"));
    }

    #[test]
    fn command_java_returns_an_exit_code_and_never_exits_the_process() {
        let src = command_java("com.example.demo", "Greet");

        assert!(src.contains("public final class GreetCommand"));
        assert!(src.contains(r#"public static final String NAME = "greet";"#));
        assert!(
            src.contains("public static int run(PrintStream out, PrintStream err, String... args)")
        );
        // A CLI command has no business depending on Spring.
        assert!(!src.contains("org.springframework"));

        // The whole point: main owns the exit, so the command stays testable
        // in-process, and output goes to injected streams, not System.out.
        // Only the class body is checked -- the Javadoc deliberately shows a
        // `main` that does call System.exit, since that is where it belongs.
        let body = &src[src.find("public final class").unwrap()..];
        assert!(
            !body.contains("System.exit"),
            "run() must not exit the process"
        );
        assert!(
            !body.contains("System.out"),
            "output should go to the injected stream"
        );
    }

    #[test]
    fn command_test_drives_the_command_through_captured_streams() {
        let test = command_test("com.example.demo", "Greet");

        assert!(test.contains("class GreetCommandTest"));
        assert!(test.contains("ByteArrayOutputStream"));
        assert!(
            test.contains("GreetCommand.run(new PrintStream(out), new PrintStream(err), args)")
        );
        assert!(test.contains("GreetCommand.USAGE_ERROR"));
    }

    #[test]
    fn stub_templates_use_the_package_and_class_name() {
        assert!(stub_controller("com.example.blog", "Post").contains("class PostController"));
        // Package-private: Spring wires these by reflection, so `public` only
        // widens what other packages can compile against.
        assert!(
            stub_service("com.example.blog", "Post").contains("\n@Component\nclass PostService")
        );
        assert!(
            !stub_service("com.example.blog", "Post").contains("public class"),
            "spring.md §2: public only where the type is module API"
        );
        assert!(
            !stub_controller("com.example.blog", "Post").contains("public class"),
            "spring.md §2: public only where the type is module API"
        );
        assert!(
            interface_java("com.example.blog", "PostStore").contains("public interface PostStore")
        );
        assert!(stub_test("com.example.blog", "Post").contains("class PostTest"));
    }

    #[test]
    fn generate_scaffold_writes_all_five_files() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("scaffold");
        let src = root.join("src/main/java/com/example/blog");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(
            src.join("BlogApplication.java"),
            "package com.example.blog;\n\npublic class BlogApplication {}\n",
        )
        .unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let result = generate(
            ArtifactKind::Scaffold,
            "post",
            &["title:string".to_string()],
            None,
            &[],
            None,
            None,
            false,
        );
        std::env::set_current_dir(original_cwd).unwrap();
        result.unwrap();

        assert!(
            root.join("src/main/java/com/example/blog/domain/Post.java")
                .is_file()
        );
        assert!(
            root.join("src/test/java/com/example/blog/domain/PostTest.java")
                .is_file()
        );
        assert!(
            root.join("src/main/java/com/example/blog/app/PostRepository.java")
                .is_file()
        );
        assert!(
            root.join("src/main/java/com/example/blog/adapters/JdbcPostRepository.java")
                .is_file()
        );
        assert!(
            root.join("src/test/java/com/example/blog/adapters/JdbcPostRepositoryIT.java")
                .is_file()
        );
        assert!(
            root.join("src/main/java/com/example/blog/service/PostService.java")
                .is_file()
        );
        assert!(
            root.join("src/main/java/com/example/blog/web/PostController.java")
                .is_file()
        );
        assert!(
            root.join("src/test/java/com/example/blog/web/PostControllerTest.java")
                .is_file()
        );

        let adapter = fs::read_to_string(
            root.join("src/main/java/com/example/blog/adapters/JdbcPostRepository.java"),
        )
        .unwrap();
        assert!(
            adapter.contains("import com.example.blog.domain.Post;"),
            "{adapter}"
        );
        assert!(
            adapter.contains("import com.example.blog.app.PostRepository;"),
            "{adapter}"
        );
        assert!(!adapter.contains("org.springframework"), "{adapter}");
    }

    /// Regression test: standalone `generate controller` used to write only
    /// the bare stub, unlike Rails (`rails generate controller` always
    /// emits a matching test).
    #[test]
    fn generate_controller_also_creates_a_controller_test() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("controller-test-companion");
        let src = root.join("src/main/java/com/example/blog");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(
            src.join("BlogApplication.java"),
            "package com.example.blog;\n\npublic class BlogApplication {}\n",
        )
        .unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let result = generate(
            ArtifactKind::Controller,
            "health",
            &[],
            None,
            &[],
            None,
            None,
            false,
        );
        std::env::set_current_dir(original_cwd).unwrap();
        result.unwrap();

        assert!(
            root.join("src/main/java/com/example/blog/web/HealthController.java")
                .is_file()
        );
        let test_file = root.join("src/test/java/com/example/blog/web/HealthControllerTest.java");
        assert!(test_file.is_file(), "expected {}", test_file.display());
        assert!(
            fs::read_to_string(test_file)
                .unwrap()
                .contains("class HealthControllerTest")
        );
    }

    #[test]
    fn generate_service_also_creates_a_service_test() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("service-test-companion");
        let src = root.join("src/main/java/com/example/blog");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(
            src.join("BlogApplication.java"),
            "package com.example.blog;\n\npublic class BlogApplication {}\n",
        )
        .unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let result = generate(
            ArtifactKind::Service,
            "billing",
            &[],
            None,
            &[],
            None,
            None,
            false,
        );
        std::env::set_current_dir(original_cwd).unwrap();
        result.unwrap();

        assert!(
            root.join("src/main/java/com/example/blog/service/BillingService.java")
                .is_file()
        );
        assert!(
            root.join("src/test/java/com/example/blog/service/BillingServiceTest.java")
                .is_file()
        );
    }

    #[test]
    fn generate_repository_creates_no_companion_test() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("repository-no-test");
        let src = root.join("src/main/java/com/example/blog");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(
            src.join("BlogApplication.java"),
            "package com.example.blog;\n\npublic class BlogApplication {}\n",
        )
        .unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let result = generate(
            ArtifactKind::Repo,
            "widget",
            &["id:uuid".to_string()],
            None,
            &[],
            None,
            None,
            false,
        );
        std::env::set_current_dir(original_cwd).unwrap();
        result.unwrap();

        assert!(
            root.join("src/main/java/com/example/blog/app/WidgetRepository.java")
                .is_file()
        );
        assert!(
            root.join("src/main/java/com/example/blog/adapters/JdbcWidgetRepository.java")
                .is_file()
        );
        assert!(
            root.join("src/test/java/com/example/blog/adapters/JdbcWidgetRepositoryIT.java")
                .is_file()
        );
    }

    /// `record` and `command` target plain Maven projects, whose entry point
    /// is App.java rather than *Application.java -- the case base_package()
    /// falls back for.
    #[test]
    fn generate_record_and_command_work_in_a_plain_cli_project() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("plain-record-command");
        let src = root.join("src/main/java/com/example/demo");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(
            src.join("App.java"),
            "package com.example.demo;\n\npublic class App {}\n",
        )
        .unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let record = generate(
            ArtifactKind::Record,
            "money",
            &["amount:long".to_string()],
            None,
            &[],
            None,
            None,
            false,
        );
        let command = generate(
            ArtifactKind::Command,
            "greet",
            &[],
            None,
            &[],
            None,
            None,
            false,
        );
        std::env::set_current_dir(original_cwd).unwrap();
        record.unwrap();
        command.unwrap();

        assert!(
            root.join("src/main/java/com/example/demo/domain/Money.java")
                .is_file()
        );
        assert!(
            root.join("src/test/java/com/example/demo/domain/MoneyTest.java")
                .is_file()
        );
        assert!(
            root.join("src/main/java/com/example/demo/cli/GreetCommand.java")
                .is_file()
        );
        assert!(
            root.join("src/test/java/com/example/demo/cli/GreetCommandTest.java")
                .is_file()
        );
    }

    #[test]
    fn destroy_command_removes_both_of_its_files() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("destroy-command");
        let src = root.join("src/main/java/com/example/demo");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(
            src.join("App.java"),
            "package com.example.demo;\n\npublic class App {}\n",
        )
        .unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        generate(
            ArtifactKind::Command,
            "greet",
            &[],
            None,
            &[],
            None,
            None,
            false,
        )
        .unwrap();
        let result = destroy(ArtifactKind::Command, "greet", true, None, false);
        std::env::set_current_dir(original_cwd).unwrap();

        result.unwrap();
        assert!(!src.join("GreetCommand.java").exists());
        assert!(
            !root
                .join("src/test/java/com/example/demo/GreetCommandTest.java")
                .exists()
        );
        assert!(src.join("App.java").is_file());
    }

    /// The shape `is_dispatcher` looks for, which is what `new-cli` writes.
    fn dispatcher_java() -> &'static str {
        "package com.example.demo;\n\
         \n\
         import java.util.LinkedHashMap;\n\
         import java.util.SequencedMap;\n\
         \n\
         public class App {\n\
         \x20   static SequencedMap<String, Command> commands() {\n\
         \x20       SequencedMap<String, Command> commands = new LinkedHashMap<>();\n\
         \x20       return commands;\n\
         \x20   }\n\
         }\n"
    }

    /// `generate command` then `destroy command` must leave the dispatcher
    /// exactly as it was. Deleting the class while the dispatcher still calls
    /// it stops the project compiling -- on the one operation whose entire
    /// job is to leave no trace.
    #[test]
    fn destroy_command_unregisters_it_from_the_dispatcher() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("destroy-command-unregisters");
        let src = root.join("src/main/java/com/example/demo");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(src.join("App.java"), dispatcher_java()).unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        generate(
            ArtifactKind::Command,
            "greet",
            &[],
            None,
            &[],
            None,
            None,
            false,
        )
        .unwrap();
        let registered = fs::read_to_string(src.join("App.java")).unwrap();
        let result = destroy(ArtifactKind::Command, "greet", true, None, false);
        std::env::set_current_dir(original_cwd).unwrap();
        result.unwrap();

        // It really was registered, or the round trip proves nothing.
        assert!(
            registered.contains("commands.put(GreetCommand.NAME, GreetCommand::run);"),
            "generate did not register the command:\n{registered}"
        );
        let after = fs::read_to_string(src.join("App.java")).unwrap();
        assert!(
            !after.contains("GreetCommand"),
            "destroy left the dispatcher calling a class it deleted:\n{after}"
        );
        assert_eq!(
            after,
            dispatcher_java(),
            "destroy is not the inverse of generate"
        );
    }

    /// The registration can outlive the files when someone deletes the class
    /// by hand. That is precisely when the dangling call needs taking out.
    #[test]
    fn destroy_command_unregisters_even_when_the_files_are_already_gone() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("destroy-command-files-gone");
        let src = root.join("src/main/java/com/example/demo");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(src.join("App.java"), dispatcher_java()).unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        generate(
            ArtifactKind::Command,
            "greet",
            &[],
            None,
            &[],
            None,
            None,
            false,
        )
        .unwrap();
        fs::remove_file(src.join("cli/GreetCommand.java")).unwrap();
        fs::remove_file(root.join("src/test/java/com/example/demo/cli/GreetCommandTest.java"))
            .unwrap();
        let result = destroy(ArtifactKind::Command, "greet", true, None, false);
        std::env::set_current_dir(original_cwd).unwrap();
        result.unwrap();

        let after = fs::read_to_string(src.join("App.java")).unwrap();
        assert_eq!(after, dispatcher_java());
    }

    /// The dispatcher's own Javadoc carries an example `commands.put(...)`
    /// line. Unregistering must not reach into it -- that is documentation,
    /// not a registration.
    #[test]
    fn unsplice_registration_leaves_an_unregistered_command_alone() {
        let source = dispatcher_java();
        assert!(unsplice_registration(source, "GreetCommand").is_none());
    }

    #[test]
    fn duplicate_record_refuses_to_overwrite_the_first() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("duplicate-record-paths");
        let src = root.join("src/main/java/com/example/demo");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(
            src.join("App.java"),
            "package com.example.demo;\n\npublic class App {}\n",
        )
        .unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        generate(
            ArtifactKind::Record,
            "tag",
            &["name:string".to_string()],
            None,
            &[],
            None,
            None,
            false,
        )
        .unwrap();
        let clash = generate(
            ArtifactKind::Record,
            "tag",
            &["name:string".to_string()],
            None,
            &[],
            None,
            None,
            false,
        );
        let result = destroy(ArtifactKind::Record, "tag", true, None, false);
        std::env::set_current_dir(original_cwd).unwrap();

        assert!(
            clash.is_err(),
            "generate must not overwrite an existing record"
        );
        result.unwrap();
        assert!(!src.join("Tag.java").exists());
        assert!(
            !root
                .join("src/test/java/com/example/demo/TagTest.java")
                .exists()
        );
    }

    #[test]
    fn generate_refuses_to_overwrite_an_existing_file() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("no-overwrite");
        let src = root.join("src/main/java/com/example/blog");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(
            src.join("BlogApplication.java"),
            "package com.example.blog;\n\npublic class BlogApplication {}\n",
        )
        .unwrap();
        let web = root.join("src/main/java/com/example/blog/web");
        fs::create_dir_all(&web).unwrap();
        fs::write(web.join("CommentController.java"), "// already here").unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let result = generate(
            ArtifactKind::Controller,
            "comment",
            &[],
            None,
            &[],
            None,
            None,
            false,
        );
        std::env::set_current_dir(original_cwd).unwrap();

        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(web.join("CommentController.java")).unwrap(),
            "// already here"
        );
    }

    #[test]
    fn destroy_removes_only_files_that_generate_would_have_created() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("destroy");
        let src = root.join("src/main/java/com/example/blog");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(
            src.join("BlogApplication.java"),
            "package com.example.blog;\n\npublic class BlogApplication {}\n",
        )
        .unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        generate(
            ArtifactKind::Record,
            "tag",
            &["name:string".to_string()],
            None,
            &[],
            None,
            None,
            false,
        )
        .unwrap();
        let result = destroy(ArtifactKind::Record, "tag", true, None, false);
        std::env::set_current_dir(original_cwd).unwrap();

        result.unwrap();
        assert!(!src.join("Tag.java").is_file());
        assert!(
            !root
                .join("src/test/java/com/example/blog/TagTest.java")
                .exists()
        );
        assert!(src.join("BlogApplication.java").is_file());
    }
}
