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

mod closed;
mod domain;
use closed::*;
pub(crate) use domain::*;

mod repository;
use repository::*;

mod recipes;
mod write;
pub(crate) use recipes::*;
pub(crate) use write::*;

mod scaffold;
pub(crate) use scaffold::*;

mod remove;
pub(crate) use remove::*;

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
    // `webhook` was an alias here until the inbound kind existed. It is not
    // wrong -- this *sends* one -- but it is ambiguous now, and "webhook"
    // means the endpoint that receives Stripe's far more often than the client
    // that posts yours. `outbound` says which half without the ambiguity.
    #[value(name = "http-sink", alias = "outbound")]
    HttpSink,
    /// At-most-once execution with a *retained result*: a scoped receipt keyed
    /// by request hash, so a retry replays the first response instead of being
    /// answered 409 by a unique constraint. Needs `jails add db`.
    #[value(alias = "idempotent")]
    Idempotency,
    /// A JWT issuer for this service's own tokens: the `JwtEncoder` Boot does
    /// not auto-configure, and a decoder that refuses a token with no `exp` --
    /// which every default configuration accepts. Needs `jails add security`.
    #[value(alias = "jwt")]
    Auth,
    /// An inbound webhook endpoint whose signature is checked over the raw
    /// request bytes, in constant time, with a bounded timestamp window
    #[value(alias = "hook")]
    Webhook,
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

/// Say what a foreign build cost this generation, before printing the files.
///
/// Not a warning about the project: a statement about *this output*, naming
/// the two shapes that changed and the dependencies jails would have added and
/// cannot. Silence here is the failure `plan.md` §12 calls out -- a tool that
/// half-understands a build reports a dependency the build does not have.
fn report_degraded_shape(project: &Project, change: &Change) {
    let crate::build::Build::Foreign(tool) = project.build() else {
        return;
    };
    println!("note: this is a {tool} project, and jails does not read {tool} build files.");
    println!("      Generated code therefore assumes plain JDBC (no Spring `JdbcClient`)");
    println!("      and no JSpecify, because those are read off a pom.xml that is not here.");
    for dep in &change.deps {
        println!("      Add yourself: {}:{}", dep.group_id, dep.artifact_id);
    }
    for (artifact_id, _) in &change.plugins {
        println!("      Add yourself: the {artifact_id} plugin.");
    }
}

/// Walk up from the current directory looking for a project root.
///
/// **Any** build marker, not only `pom.xml` -- `plan.md` §12. Most of jails
/// never touches Maven (`inspect.rs` and `rename.rs` contain zero occurrences
/// of `pom`), so refusing at the door was refusing commands that would have
/// worked. The commands that do need Maven refuse themselves, through
/// `build::require_maven`, which is a refusal that can say what still works.
///
/// Nearest wins, so a Gradle sub-module inside a Maven reactor resolves to the
/// sub-module -- the same rule as before, applied to more markers.
pub(crate) fn find_project_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().map_err(|e| format!("failed to get cwd: {e}"))?;
    loop {
        if crate::build::detect(&dir) != crate::build::Build::Bare {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(
                "no pom.xml (or build.gradle, settings.gradle, build.xml, BUILD.bazel) \
                 in this or any parent directory"
                    .to_string(),
            );
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
    let mut planned = planned_package_infos(&root, project.pom(), &artifacts);
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
    // Degraded mode has to *say* which shape it chose (`plan.md` §12). Every
    // structural decision in the templates is read off the pom -- whether a
    // repository adapter is a `JdbcClient` bean, whether `package-info.java`
    // can be annotated -- and with no pom they all take their default. Leaving
    // that unsaid would hand the reader Java shaped by a fact they never saw.
    report_degraded_shape(project, &change);

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
