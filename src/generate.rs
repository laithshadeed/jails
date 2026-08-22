use crate::Result;
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
pub(crate) use repository::*;

#[derive(Clone, Copy, Debug, ValueEnum)]
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
        if let Some(rest) = line.strip_prefix("package ") {
            if let Some(pkg) = rest.trim().strip_suffix(';') {
                return Ok(pkg.trim().to_string());
            }
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
pub(crate) fn webmvc_test_import(root: &Path) -> &'static str {
    const LEGACY: &str = "org.springframework.boot.test.autoconfigure.web.servlet.WebMvcTest";
    const CURRENT: &str = "org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest";
    if spring_boot_major(root) >= 4 {
        CURRENT
    } else {
        LEGACY
    }
}

/// The Spring Boot major version from the parent pom, defaulting to 3 when it
/// cannot be read -- the conservative choice, since the pre-4 package names
/// still exist as deprecated aliases in some builds while the 4 ones simply
/// do not exist before 4.
pub(crate) fn spring_boot_major(root: &Path) -> u32 {
    let Ok(pom) = fs::read_to_string(root.join("pom.xml")) else {
        return 3;
    };
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

pub(crate) fn mockmvc_autoconfigure_import(root: &Path) -> &'static str {
    const LEGACY: &str =
        "org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc";
    const CURRENT: &str = "org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc";

    if spring_boot_major(root) >= 4 {
        CURRENT
    } else {
        LEGACY
    }
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

/// Ensure Failsafe is configured, whenever jails writes an `*IT`.
///
/// Called from the write path rather than from each kind's arm, so a new
/// generator that emits an integration test cannot forget it. Without this
/// the generated `*IT` never executes and `mvn verify` still reports
/// success -- a test that silently does not run is worse than no test.
fn ensure_failsafe(root: &Path, artifacts: &[Artifact]) -> Result<()> {
    let writes_an_it = artifacts.iter().any(|a| {
        a.path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("IT.java"))
    });
    if !writes_an_it {
        return Ok(());
    }
    let pom = crate::pom::read(root)?;
    if let Some(updated) = crate::pom::add_plugin(
        &pom,
        crate::spring::FAILSAFE_ARTIFACT,
        crate::spring::FAILSAFE_PLUGIN,
    )? {
        fs::write(root.join("pom.xml"), updated)
            .map_err(|e| format!("failed to write pom.xml: {e}"))?;
        println!("  plugin {}", crate::spring::FAILSAFE_ARTIFACT);
    }
    Ok(())
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
            fs::write(root.join("pom.xml"), updated)
                .map_err(|e| format!("failed to write pom.xml: {e}"))?;
            println!("     dep {}:{}", dep.group_id, dep.artifact_id);
            Ok(())
        }
        None => Ok(()),
    }
}

/// Spring-only generator kinds refuse politely rather than writing code that
/// cannot compile.
fn require_spring_project(root: &Path, kind: &str) -> Result<()> {
    let pom = crate::pom::read(root)?;
    crate::spring::require_spring(crate::pom::flavor(&pom), kind)
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

struct Artifact {
    kind: &'static str,
    path: PathBuf,
    contents: String,
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
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    let contents = if path.extension().is_some_and(|e| e == "java") {
        ensure_package_info(root, path)?;
        normalize_imports(contents)
    } else {
        contents.to_string()
    };
    fs::write(path, &contents).map_err(|e| format!("failed to write {}: {e}", path.display()))
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
    if info.exists() {
        return Ok(());
    }
    if !jspecify_available(root) {
        return Ok(());
    }
    let Some(pkg) = package_of_dir(root, dir) else {
        return Ok(());
    };
    fs::write(&info, package_info_java(&pkg))
        .map_err(|e| format!("failed to write {}: {e}", info.display()))?;
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

pub fn generate(
    kind: ArtifactKind,
    name: &str,
    fields: &[String],
    package: Option<&str>,
    indexes: &[String],
    strategy_on: Option<&str>,
    strategy_yields: Option<&str>,
    pretend: bool,
) -> Result<()> {
    let root = find_project_root()?;
    let base = base_package(&root)?;

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
    // `--package` replaces the conventional home for every artifact in this
    // call; without it each kind goes where its convention says.
    let config = crate::config::Config::load(&root)?;
    let place = |default: &str| subpackage(&base, package.unwrap_or(config.layer(default)));

    let artifacts = match kind {
        ArtifactKind::Scaffold => {
            scaffold_artifacts(&root, &base, &name, fields, package, indexes)?
        }
        ArtifactKind::Controller => {
            let web = place(layout::WEB);
            vec![
                Artifact {
                    kind: "controller",
                    path: main_dir(&root, &web).join(format!("{name}Controller.java")),
                    contents: stub_controller(&web, &name),
                },
                Artifact {
                    kind: "controller test",
                    path: test_dir(&root, &web).join(format!("{name}ControllerTest.java")),
                    contents: controller_stub_test(
                        &web,
                        &name,
                        mockmvc_autoconfigure_import(&root),
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
                    contents: stub_service(&service, &name),
                },
                Artifact {
                    kind: "service test",
                    path: test_dir(&root, &service).join(format!("{name}ServiceTest.java")),
                    contents: service_stub_test(&service, &name),
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
                    contents: stub_class(&pkg, &name),
                },
                Artifact {
                    kind: "class test",
                    path: test_dir(&root, &pkg).join(format!("{name}Test.java")),
                    contents: class_test(&pkg, &name),
                },
            ]
        }
        ArtifactKind::Interface => {
            let pkg = place("");
            vec![Artifact {
                kind: "interface",
                path: main_dir(&root, &pkg).join(format!("{name}.java")),
                contents: interface_java(&pkg, &name),
            }]
        }
        // Spring-only kinds. The templates live in spring.rs, next to the
        // capabilities that share their Spring Boot 4 assumptions.
        ArtifactKind::Client => {
            require_spring_project(&root, "client")?;
            let pkg = place(layout::CLIENTS);
            crate::spring::client_files(&root, &pkg, &name)
                .into_iter()
                .map(|(path, contents, kind)| Artifact {
                    kind,
                    path,
                    contents,
                })
                .collect()
        }
        ArtifactKind::Fetcher => {
            require_spring_project(&root, "fetcher")?;
            if !fields.is_empty() || strategy_on.is_some() || strategy_yields.is_some() {
                return Err(
                    "fetcher takes only a name; limits and policy are external configuration"
                        .to_string(),
                );
            }
            let pkg = place(layout::CLIENTS);
            crate::spring::fetcher_files(&root, &pkg, &name)
                .into_iter()
                .map(|(path, contents, kind)| Artifact {
                    kind,
                    path,
                    contents,
                })
                .collect()
        }
        ArtifactKind::Job => {
            require_spring_project(&root, "job")?;
            let pkg = place(layout::JOBS);
            crate::spring::job_files(&root, &pkg, &name)
                .into_iter()
                .map(|(path, contents, kind)| Artifact {
                    kind,
                    path,
                    contents,
                })
                .collect()
        }
        ArtifactKind::HttpWorkflow => {
            require_spring_project(&root, "http-workflow")?;
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
            let jobs = place(layout::JOBS);
            let clients = subpackage(&base, config.layer(layout::CLIENTS));
            let web = subpackage(&base, config.layer(layout::WEB));
            crate::spring::http_workflow_files(
                &root,
                &jobs,
                &clients,
                &web,
                &name,
                &strip_redundant_suffix(ArtifactKind::Fetcher, &capitalize(fetcher)),
            )?
            .into_iter()
            .map(|(path, contents, kind)| Artifact {
                kind,
                path,
                contents,
            })
            .collect()
        }
        ArtifactKind::Association => {
            require_spring_project(&root, "association")?;
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
            let domain = subpackage(&base, config.layer(layout::DOMAIN));
            let adapters = place(layout::ADAPTERS);
            crate::spring::association_files(
                &root,
                &domain,
                &adapters,
                &name,
                &capitalize(child),
                &capitalize(parent),
                fields,
            )?
            .into_iter()
            .map(|(path, contents, kind)| Artifact {
                kind,
                path,
                contents,
            })
            .collect()
        }
        ArtifactKind::HttpSink => {
            require_spring_project(&root, "http-sink")?;
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
            let jobs = place(layout::JOBS);
            let messaging = subpackage(&base, config.layer(layout::MESSAGING));
            let adapters = subpackage(&base, config.layer(layout::ADAPTERS));
            crate::spring::http_sink_files(
                &root,
                &jobs,
                &messaging,
                &adapters,
                &name,
                &capitalize(usecase),
                &capitalize(event),
            )?
            .into_iter()
            .map(|(path, contents, kind)| Artifact {
                kind,
                path,
                contents,
            })
            .collect()
        }
        ArtifactKind::DurableJob => {
            require_spring_project(&root, "durable-job")?;
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
            let jobs = place(layout::JOBS);
            let web = subpackage(&base, config.layer(layout::WEB));
            let service = subpackage(&base, config.layer(layout::SERVICE));
            let app = subpackage(&base, config.layer(layout::APP));
            let domain = subpackage(&base, config.layer(layout::DOMAIN));
            let parsed = parse_fields(fields)?;
            crate::spring::durable_job_files(
                &root,
                &base,
                &jobs,
                &web,
                &service,
                &app,
                &domain,
                &name,
                &capitalize(usecase),
                &capitalize(target),
                &parsed,
            )?
            .into_iter()
            .map(|(path, contents, kind)| Artifact {
                kind,
                path,
                contents,
            })
            .collect()
        }
        ArtifactKind::Usecase => {
            require_spring_project(&root, "usecase")?;
            let target = strategy_on.ok_or_else(|| {
                format!(
                    "usecase {name} needs the resource it creates.\n       fix: pass `--on <Resource>`, for example `jails g usecase {name} title:string --on Task`."
                )
            })?;
            let service = place(layout::SERVICE);
            let web = place(layout::WEB);
            // `--package` places the operation itself. The target resource
            // already exists in the project's configured scaffold layers;
            // moving the operation must not make Jails look for a second copy
            // of that resource in the override package.
            let domain = subpackage(&base, config.layer(layout::DOMAIN));
            let app = subpackage(&base, config.layer(layout::APP));
            let adapters = subpackage(&base, config.layer(layout::ADAPTERS));
            let messaging = subpackage(&base, config.layer(layout::MESSAGING));
            let jobs = subpackage(&base, config.layer(layout::JOBS));
            let parsed = parse_fields(fields)?;
            let mut files = crate::spring::usecase_files(
                &root,
                &base,
                &service,
                &web,
                &domain,
                &app,
                &adapters,
                &name,
                &capitalize(target),
                &parsed,
            )?;
            if let Some(event) = strategy_yields {
                files.extend(crate::spring::outbox_files(
                    &root,
                    &service,
                    &domain,
                    &app,
                    &adapters,
                    &messaging,
                    &jobs,
                    &name,
                    &capitalize(target),
                    &capitalize(event),
                    &parsed,
                )?);
            }
            files
                .into_iter()
                .map(|(path, contents, kind)| Artifact {
                    kind,
                    path,
                    contents,
                })
                .collect()
        }
        ArtifactKind::Query => {
            require_spring_project(&root, "query")?;
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
            let service = place(layout::SERVICE);
            let web = place(layout::WEB);
            let domain = subpackage(&base, config.layer(layout::DOMAIN));
            let app = subpackage(&base, config.layer(layout::APP));
            let adapters = subpackage(&base, config.layer(layout::ADAPTERS));
            let parsed = parse_fields(fields)?;
            crate::spring::query_files(
                &root,
                &base,
                &service,
                &web,
                &domain,
                &app,
                &adapters,
                &name,
                &capitalize(target),
                &parsed,
            )?
            .into_iter()
            .map(|(path, contents, kind)| Artifact {
                kind,
                path,
                contents,
            })
            .collect()
        }
        ArtifactKind::Transition => {
            require_spring_project(&root, "transition")?;
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
            let service = place(layout::SERVICE);
            let web = place(layout::WEB);
            let domain = subpackage(&base, config.layer(layout::DOMAIN));
            let app = subpackage(&base, config.layer(layout::APP));
            let adapters = subpackage(&base, config.layer(layout::ADAPTERS));
            let parsed = parse_fields(fields)?;
            crate::spring::transition_files(
                &root,
                &base,
                &service,
                &web,
                &domain,
                &app,
                &adapters,
                &name,
                &capitalize(target),
                &parsed,
            )?
            .into_iter()
            .map(|(path, contents, kind)| Artifact {
                kind,
                path,
                contents,
            })
            .collect()
        }
        ArtifactKind::Event => {
            require_spring_project(&root, "event")?;
            let pkg = place(layout::MESSAGING);
            let domain = place(layout::DOMAIN);
            let parsed = parse_fields(fields)?;
            crate::spring::event_files(&root, &pkg, &domain, &name, &parsed)?
                .into_iter()
                .map(|(path, contents, kind)| Artifact {
                    kind,
                    path,
                    contents,
                })
                .collect()
        }
        ArtifactKind::Dto => {
            let domain = place(layout::DOMAIN);
            let web = place(layout::WEB);
            let parsed = parse_fields(fields)?;
            let components = if parsed.is_empty() {
                fields_from_record(&root, &domain, &name).ok_or_else(|| {
                    format!(
                        "no {name} record found under {domain}, and no field spec was given.\n       \
                         Either `jails g record {name} <field:type ...>` first, or pass the fields here."
                    )
                })?
            } else {
                parsed
            };
            crate::spring::dto_files(&root, &web, &domain, &name, &components)
                .into_iter()
                .map(|(path, contents, kind)| Artifact {
                    kind,
                    path,
                    contents,
                })
                .collect()
        }
        ArtifactKind::Record => {
            let parsed = parse_fields(fields)?;
            let domain = place(layout::DOMAIN);
            vec![
                Artifact {
                    kind: "record",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: record_java(&domain, &name, &parsed),
                },
                Artifact {
                    kind: "record test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: record_test(&root, &domain, &name, &parsed),
                },
            ]
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
                    contents: value_java(&domain, &name, &parsed),
                },
                Artifact {
                    kind: "value test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: value_test(&root, &domain, &name, &parsed),
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
                    contents: enum_java(&domain, &name, &constants),
                },
                Artifact {
                    kind: "enum test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: enum_test(&domain, &name, &constants),
                },
            ]
        }
        ArtifactKind::Repo => {
            let app = place(layout::APP);
            let adapters = place(layout::ADAPTERS);
            let domain = place(layout::DOMAIN);
            let mut artifacts = Vec::new();
            // Three sources for the columns, in order of how much they know:
            // the field spec on this command line, the record already on
            // disk, or nothing (which yields the TODO-shaped adapter).
            let spec = parse_fields(fields)?;
            let record_fields = if spec.is_empty() {
                fields_from_record(&root, &domain, &name).unwrap_or_default()
            } else {
                spec
            };

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
                    contents: record_java(&domain, &name, &id),
                });
                artifacts.push(Artifact {
                    kind: "record test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: record_test(&root, &domain, &name, &id),
                });
            }

            artifacts.push(Artifact {
                kind: "repository port",
                path: main_dir(&root, &app).join(format!("{name}Repository.java")),
                contents: repository_port(&app, &name, &import_of(&app, &domain, &name)),
            });
            artifacts.push(Artifact {
                kind: "JDBC adapter",
                path: main_dir(&root, &adapters).join(format!("Jdbc{name}Repository.java")),
                contents: jdbc_repository_for(
                    &root,
                    &adapters,
                    &name,
                    &format!(
                        "{}{}",
                        import_of(&adapters, &domain, &name),
                        import_of(&adapters, &app, &format!("{name}Repository"))
                    ),
                    &crate::sql::columns(&record_fields, &root, &domain, &lower_first(&name)),
                    &domain,
                ),
            });
            artifacts.push(Artifact {
                kind: "JDBC adapter integration test",
                path: test_dir(&root, &adapters).join(format!("Jdbc{name}RepositoryIT.java")),
                contents: jdbc_repository_test(&adapters, &name),
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
                contents: handler_java(&api, &name, &import_of(&api, &domain, "ApiError")),
            });
            artifacts.push(Artifact {
                kind: "handler test",
                path: test_dir(&root, &api).join(format!("{name}HandlerTest.java")),
                contents: handler_test(&api, &name),
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
                    contents: sealed_java(&domain, &name, &variants),
                },
                Artifact {
                    kind: "sealed type test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: sealed_test(&domain, &name, &variants),
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
                contents: strategy_interface_java(&domain, &name, &variants, on, strategy_yields),
            }];
            for variant in &variants {
                let class = strategy_class(variant, &name);
                artifacts.push(Artifact {
                    kind: "strategy implementation",
                    path: main_dir(&root, &domain).join(format!("{class}.java")),
                    contents: strategy_impl_java(
                        &domain,
                        &name,
                        &class,
                        on,
                        strategy_yields,
                        spring,
                    ),
                });
                artifacts.push(Artifact {
                    kind: "strategy implementation test",
                    path: test_dir(&root, &domain).join(format!("{class}Test.java")),
                    contents: strategy_impl_test(&domain, &name, &class, on, strategy_yields),
                });
            }
            artifacts
        }
        ArtifactKind::Command => {
            let cli = place(layout::CLI);
            vec![
                Artifact {
                    kind: "command",
                    path: main_dir(&root, &cli).join(format!("{name}Command.java")),
                    contents: command_java(&cli, &name),
                },
                Artifact {
                    kind: "command test",
                    path: test_dir(&root, &cli).join(format!("{name}CommandTest.java")),
                    contents: command_test(&cli, &name),
                },
            ]
        }
        ArtifactKind::Cli => {
            let cli = place(layout::CLI);
            vec![
                Artifact {
                    kind: "cli",
                    path: main_dir(&root, &cli).join(format!("{name}Cli.java")),
                    contents: cli_java(&cli, &format!("{name}Cli"), &name.to_lowercase()),
                },
                Artifact {
                    kind: "cli test",
                    path: test_dir(&root, &cli).join(format!("{name}CliTest.java")),
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
                contents: stub_test(&pkg, &name),
            }]
        }
        ArtifactKind::IntegrationTest => {
            let pkg = place("");
            vec![Artifact {
                kind: "integration test",
                path: test_dir(&root, &pkg).join(format!("{name}IT.java")),
                contents: integration_test_java(&pkg, &name),
            }]
        }
    };

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

    for artifact in &artifacts {
        if artifact.path.exists()
            && !(artifact.kind == "scheduling"
                && fs::read_to_string(&artifact.path)
                    .is_ok_and(|source| source == artifact.contents))
        {
            return Err(format!("{} already exists", artifact.path.display()));
        }
    }
    // `--pretend` still runs every check above, so a run that would have
    // collided reports the collision rather than a clean-looking plan.
    if pretend {
        for artifact in &artifacts {
            println!("would create {} {}", artifact.kind, artifact.path.display());
        }
        if matches!(kind, ArtifactKind::Command) {
            println!("would register {name} in the project's command dispatcher");
        }
        if let Some(dep) = match kind {
            ArtifactKind::Dto | ArtifactKind::Scaffold => Some(&crate::spring::VALIDATION_STARTER),
            ArtifactKind::Client => Some(&crate::spring::RESTCLIENT_STARTER),
            ArtifactKind::Fetcher => Some(&crate::spring::APACHE_HTTPCLIENT),
            ArtifactKind::Event => Some(&crate::spring::TESTCONTAINERS_KAFKA),
            _ => None,
        } {
            println!(
                "would ensure dependency {}:{}",
                dep.group_id, dep.artifact_id
            );
        }
        println!();
        println!("--pretend: nothing was written.");
        return Ok(());
    }
    for artifact in &artifacts {
        if artifact.path.exists() && artifact.kind == "scheduling" {
            println!("exists scheduling {}", artifact.path.display());
            continue;
        }
        write_new_file(&root, &artifact.path, &artifact.contents)?;
        println!("created {} {}", artifact.kind, artifact.path.display());
    }

    if matches!(kind, ArtifactKind::Command) {
        register_command(&root, &base, &name)?;
    }
    // A generator that emits code needing a dependency has to supply it.
    // The alternative is handing the reader a compile error for a line they
    // did not write, which is exactly the plumbing this tool exists to
    // remove. Splicing is idempotent -- pom.rs reports when it is already
    // there and changes nothing.
    ensure_failsafe(&root, &artifacts)?;
    match kind {
        ArtifactKind::Dto | ArtifactKind::Scaffold => {
            ensure_dependency(&root, &crate::spring::VALIDATION_STARTER)?
        }
        ArtifactKind::Client => ensure_dependency(&root, &crate::spring::RESTCLIENT_STARTER)?,
        ArtifactKind::Fetcher => {
            ensure_dependency(&root, &crate::spring::APACHE_HTTPCLIENT)?;
            ensure_dependency(&root, &crate::spring::ACTUATOR_STARTER)?;
        }
        ArtifactKind::Event => {
            ensure_dependency(&root, &crate::spring::TESTCONTAINERS_KAFKA)?;
            ensure_dependency(&root, &crate::spring::SPRING_TESTCONTAINERS)?;
        }
        _ => {}
    }
    Ok(())
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

/// The one command that spans layers, and so the only place that has to say
/// out loud which package each half of a vertical slice lives in -- and add
/// the imports that crossing those boundaries now costs.
fn scaffold_artifacts(
    root: &Path,
    base: &str,
    name: &str,
    fields: &[String],
    package: Option<&str>,
    indexes: &[String],
) -> Result<Vec<Artifact>> {
    let parsed = parse_fields(fields)?;

    let config = crate::config::Config::load(root)?;
    let place = |default: &str| subpackage(base, package.unwrap_or(config.layer(default)));
    let domain = place(layout::DOMAIN);
    let repository = place(layout::APP);
    let adapters = place(layout::ADAPTERS);
    let service = place(layout::SERVICE);
    let web = place(layout::WEB);

    crate::spring::require_scope_authorizer(root, base, "scaffold", name, &parsed)?;

    let domain_in = |user: &str| import_of(user, &domain, name);
    let columns = crate::sql::columns(&parsed, root, &domain, &lower_first(name));

    // The migration is emitted only when the project has somewhere to put
    // one -- `jails add db` creates db/migration, and a .sql file in a
    // project with no Flyway is dead weight nobody asked for. When it is
    // emitted it comes from the same column list as the adapter, which is
    // the point: a hand-written pair drifts (an `amount` column against an
    // `amount_minor` select), and one list cannot disagree with itself.
    let migration_dir = root.join("src/main/resources/db/migration");
    let mut artifacts = Vec::new();

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

    artifacts.extend(vec![
        Artifact {
            kind: "record",
            path: main_dir(root, &domain).join(format!("{name}.java")),
            contents: record_java(&domain, name, &parsed),
        },
        Artifact {
            kind: "record test",
            path: test_dir(root, &domain).join(format!("{name}Test.java")),
            contents: record_test(root, &domain, name, &parsed),
        },
        Artifact {
            kind: "repository port",
            path: main_dir(root, &repository).join(format!("{name}Repository.java")),
            contents: repository_port(&repository, name, &domain_in(&repository)),
        },
        Artifact {
            kind: "JDBC adapter",
            path: main_dir(root, &adapters).join(format!("Jdbc{name}Repository.java")),
            contents: jdbc_repository_for(
                root,
                &adapters,
                name,
                &format!(
                    "{}{}",
                    domain_in(&adapters),
                    import_of(&adapters, &repository, &format!("{name}Repository"))
                ),
                // The record was just written from these same fields, so the
                // adapter and the type it maps cannot disagree.
                &crate::sql::columns(&parsed, root, &domain, &lower_first(name)),
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
                repository_wiring(root) != RepositoryWiring::JdbcClientBean,
            ),
        },
        Artifact {
            kind: "request",
            path: main_dir(root, &web).join(format!("{name}Request.java")),
            contents: crate::spring::request_java_for(
                &web,
                name,
                &parsed,
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
                &parsed,
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
                base,
                &web,
                name,
                &format!(
                    "{}{}",
                    domain_in(&web),
                    import_of(&web, &service, &format!("{name}Service"))
                ),
                parsed.iter().any(|f| f.name == "id"),
                &parsed,
            ),
        },
        Artifact {
            kind: "controller test",
            path: test_dir(root, &web).join(format!("{name}ControllerTest.java")),
            contents: crate::spring::resource_controller_test_java(
                base,
                &web,
                name,
                &import_of(&web, &service, &format!("{name}Service")),
                &parsed,
                webmvc_test_import(root),
            ),
        },
    ]);

    Ok(artifacts)
}

pub fn destroy(
    kind: ArtifactKind,
    name: &str,
    force: bool,
    package: Option<&str>,
    pretend: bool,
) -> Result<()> {
    let root = find_project_root()?;
    let base = base_package(&root)?;
    let config = crate::config::Config::load(&root)?;
    let place = |default: &str| subpackage(&base, package.unwrap_or(config.layer(default)));
    // `cases` is addressed by the markdown path it was generated from, which
    // must not be run through capitalize like a class name.
    let raw_name = name.to_string();
    let name = strip_redundant_suffix(kind, &capitalize(name));

    let paths: Vec<PathBuf> = match kind {
        ArtifactKind::Scaffold => vec![
            main_dir(&root, &place(layout::DOMAIN)).join(format!("{name}.java")),
            test_dir(&root, &place(layout::DOMAIN)).join(format!("{name}Test.java")),
            main_dir(&root, &place(layout::APP)).join(format!("{name}Repository.java")),
            main_dir(&root, &place(layout::ADAPTERS)).join(format!("Jdbc{name}Repository.java")),
            test_dir(&root, &place(layout::ADAPTERS)).join(format!("Jdbc{name}RepositoryIT.java")),
            main_dir(&root, &place(layout::ADAPTERS))
                .join(format!("InMemory{name}Repository.java")),
            main_dir(&root, &place(layout::SERVICE)).join(format!("{name}Service.java")),
            test_dir(&root, &place(layout::SERVICE)).join(format!("{name}ServiceTest.java")),
            main_dir(&root, &place(layout::WEB)).join(format!("{name}Controller.java")),
            test_dir(&root, &place(layout::WEB)).join(format!("{name}ControllerTest.java")),
            main_dir(&root, &place(layout::WEB)).join(format!("{name}Request.java")),
            main_dir(&root, &place(layout::WEB)).join(format!("{name}Response.java")),
        ],
        ArtifactKind::Controller => vec![
            main_dir(&root, &place(layout::WEB)).join(format!("{name}Controller.java")),
            test_dir(&root, &place(layout::WEB)).join(format!("{name}ControllerTest.java")),
        ],
        ArtifactKind::Service => vec![
            main_dir(&root, &place(layout::SERVICE)).join(format!("{name}Service.java")),
            test_dir(&root, &place(layout::SERVICE)).join(format!("{name}ServiceTest.java")),
        ],
        ArtifactKind::Record | ArtifactKind::Value | ArtifactKind::Enum | ArtifactKind::Sealed => {
            vec![
                main_dir(&root, &place(layout::DOMAIN)).join(format!("{name}.java")),
                test_dir(&root, &place(layout::DOMAIN)).join(format!("{name}Test.java")),
            ]
        }
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
        ArtifactKind::Command => vec![
            main_dir(&root, &place(layout::CLI)).join(format!("{name}Command.java")),
            test_dir(&root, &place(layout::CLI)).join(format!("{name}CommandTest.java")),
        ],
        ArtifactKind::Handler => vec![
            main_dir(&root, &place(layout::API)).join(format!("{name}Handler.java")),
            test_dir(&root, &place(layout::API)).join(format!("{name}HandlerTest.java")),
        ],
        ArtifactKind::Repo => vec![
            main_dir(&root, &place(layout::APP)).join(format!("{name}Repository.java")),
            main_dir(&root, &place(layout::ADAPTERS)).join(format!("Jdbc{name}Repository.java")),
            test_dir(&root, &place(layout::ADAPTERS)).join(format!("Jdbc{name}RepositoryIT.java")),
        ],
        ArtifactKind::Cli => vec![
            main_dir(&root, &place(layout::CLI)).join(format!("{name}Cli.java")),
            test_dir(&root, &place(layout::CLI)).join(format!("{name}CliTest.java")),
        ],
        // `cases` derives its class from a markdown file's name, so destroy
        // takes that same path and resolves it the same way generate did.
        ArtifactKind::Cases => {
            vec![
                test_dir(&root, &place(""))
                    .join(format!("{}.java", cases_class_name(Path::new(&raw_name))?)),
            ]
        }
        ArtifactKind::Migration | ArtifactKind::Association => {
            return Err(
                "migrations and associations are forward-only; create a new migration instead of destroying one"
                    .to_string(),
            );
        }
        ArtifactKind::Class => vec![
            main_dir(&root, &place("")).join(format!("{name}.java")),
            test_dir(&root, &place("")).join(format!("{name}Test.java")),
        ],
        ArtifactKind::Interface => vec![main_dir(&root, &place("")).join(format!("{name}.java"))],
        ArtifactKind::Test => vec![test_dir(&root, &place("")).join(format!("{name}Test.java"))],
        // The shared registration files (HttpClientsConfig, SchedulingConfig)
        // are deliberately not listed: a second client or job still needs
        // them, and deleting one would strand the other.
        ArtifactKind::Client => {
            let pkg = place(layout::CLIENTS);
            vec![
                main_dir(&root, &pkg).join(format!("{name}Client.java")),
                test_dir(&root, &pkg).join(format!("{name}ClientTest.java")),
            ]
        }
        ArtifactKind::Fetcher => {
            let pkg = place(layout::CLIENTS);
            vec![
                main_dir(&root, &pkg).join(format!("{name}Fetcher.java")),
                main_dir(&root, &pkg).join(format!("Safe{name}Fetcher.java")),
                test_dir(&root, &pkg).join(format!("Safe{name}FetcherTest.java")),
            ]
        }
        ArtifactKind::Job => {
            let pkg = place(layout::JOBS);
            vec![
                main_dir(&root, &pkg).join(format!("{name}Job.java")),
                test_dir(&root, &pkg).join(format!("{name}JobTest.java")),
            ]
        }
        ArtifactKind::HttpSink => {
            let pkg = place(layout::JOBS);
            vec![
                main_dir(&root, &pkg).join(format!("{name}HttpOutboxSink.java")),
                test_dir(&root, &pkg).join(format!("{name}HttpOutboxSinkTest.java")),
            ]
        }
        ArtifactKind::HttpWorkflow => {
            let jobs = place(layout::JOBS);
            let web = subpackage(&base, config.layer(layout::WEB));
            vec![
                main_dir(&root, &jobs).join(format!("{name}Workflow.java")),
                main_dir(&root, &web).join(format!("{name}WorkflowController.java")),
                test_dir(&root, &jobs).join(format!("{name}WorkflowIT.java")),
            ]
        }
        ArtifactKind::DurableJob => {
            let jobs = place(layout::JOBS);
            let web = subpackage(&base, config.layer(layout::WEB));
            vec![
                main_dir(&root, &jobs).join(format!("{name}Work.java")),
                main_dir(&root, &jobs).join(format!("{name}Queue.java")),
                main_dir(&root, &jobs).join(format!("Jdbc{name}Store.java")),
                main_dir(&root, &jobs).join(format!("{name}Worker.java")),
                main_dir(&root, &web).join(format!("{name}JobController.java")),
                test_dir(&root, &jobs).join(format!("{name}JobIT.java")),
            ]
        }
        ArtifactKind::Event => {
            let pkg = place(layout::MESSAGING);
            vec![
                main_dir(&root, &pkg).join(format!("{name}Event.java")),
                main_dir(&root, &pkg).join(format!("{name}Publisher.java")),
                main_dir(&root, &pkg).join(format!("{name}Listener.java")),
                test_dir(&root, &pkg).join(format!("{name}MessagingIT.java")),
            ]
        }
        ArtifactKind::Usecase => {
            let service = place(layout::SERVICE);
            let web = place(layout::WEB);
            let jobs = place(layout::JOBS);
            vec![
                main_dir(&root, &service).join(format!("{name}Command.java")),
                main_dir(&root, &service).join(format!("{name}UseCase.java")),
                main_dir(&root, &service).join(format!("Default{name}UseCase.java")),
                test_dir(&root, &service).join(format!("{name}UseCaseTest.java")),
                main_dir(&root, &web).join(format!("{name}Controller.java")),
                test_dir(&root, &web).join(format!("{name}ControllerTest.java")),
                main_dir(&root, &service).join(format!("Outbox{name}UseCase.java")),
                main_dir(&root, &jobs).join(format!("Jdbc{name}Outbox.java")),
                main_dir(&root, &jobs).join(format!("{name}OutboxWorker.java")),
                test_dir(&root, &jobs).join(format!("{name}OutboxIT.java")),
            ]
        }
        ArtifactKind::Query => {
            let service = place(layout::SERVICE);
            let web = place(layout::WEB);
            let adapters = place(layout::ADAPTERS);
            vec![
                main_dir(&root, &service).join(format!("{name}Query.java")),
                main_dir(&root, &service).join(format!("{name}QueryPort.java")),
                main_dir(&root, &adapters).join(format!("Jdbc{name}Query.java")),
                test_dir(&root, &adapters).join(format!("Jdbc{name}QueryIT.java")),
                main_dir(&root, &web).join(format!("{name}QueryController.java")),
                test_dir(&root, &web).join(format!("{name}QueryControllerTest.java")),
            ]
        }
        ArtifactKind::Transition => {
            let service = place(layout::SERVICE);
            let web = place(layout::WEB);
            let adapters = place(layout::ADAPTERS);
            vec![
                main_dir(&root, &service).join(format!("{name}Command.java")),
                main_dir(&root, &service).join(format!("{name}UseCase.java")),
                main_dir(&root, &adapters).join(format!("Jdbc{name}Transition.java")),
                test_dir(&root, &adapters).join(format!("Jdbc{name}TransitionIT.java")),
                main_dir(&root, &web).join(format!("{name}Controller.java")),
                test_dir(&root, &web).join(format!("{name}ControllerTest.java")),
            ]
        }
        ArtifactKind::Dto => {
            let web = place(layout::WEB);
            vec![
                main_dir(&root, &web).join(format!("{name}Request.java")),
                main_dir(&root, &web).join(format!("{name}Response.java")),
                test_dir(&root, &web).join(format!("{name}DtoTest.java")),
            ]
        }
        ArtifactKind::IntegrationTest => {
            vec![test_dir(&root, &place("")).join(format!("{name}IT.java"))]
        }
    };

    let existing: Vec<&PathBuf> = paths.iter().filter(|p| p.exists()).collect();
    if existing.is_empty() {
        // A command's files can already be gone while the dispatcher still
        // calls it -- a half-finished delete by hand is exactly when the
        // registration most needs taking out.
        if matches!(kind, ArtifactKind::Command) && !pretend {
            unregister_command(&root, &name)?;
        }
        println!("nothing to destroy");
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
            Path::new("/tmp/does-not-matter"),
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
            Path::new("/tmp/does-not-matter"),
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
            (ArtifactKind::IntegrationTest, "PaymentIT", "Payment"),
            (ArtifactKind::Job, "ReconcileJob", "Reconcile"),
            (ArtifactKind::Client, "LedgerClient", "Ledger"),
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
        for forbidden in ["org.springframework"] {
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
            mockmvc_autoconfigure_import(&root),
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
            mockmvc_autoconfigure_import(&root),
            "org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc"
        );
    }

    #[test]
    fn mockmvc_import_defaults_to_legacy_when_pom_is_unreadable() {
        let root = scratch("no-pom");
        assert_eq!(
            mockmvc_autoconfigure_import(&root),
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
