//! `jails add <capability>...` -- grow an existing project with one or more
//! capabilities at a time. `jails remove` is the inverse: it unsplices the
//! same dependencies, deletes the same files, and takes compose services back
//! out of `compose.yaml`.
//!
//! Where `generate` emits a *class*, `add` emits a *slice*: the dependency,
//! the code that uses it, and a test that proves the wiring compiles and
//! runs. Compose-backed capabilities (`db`, `kafka`) also splice a service
//! into `compose.yaml` and start it; `jails run` starts whatever is left in
//! the file. Every capability is idempotent (re-running reports what is
//! already there) and takes no required arguments -- the library, the
//! version, the package and the class names all have opinionated defaults.
//!
//! `Capability` is a `clap::ValueEnum` rather than a `String` on purpose:
//! that is the only way `clap_complete` can emit a static completion list for
//! `jails add <TAB>`, and the doc comment on each variant becomes its
//! completion description.

use crate::Result;
use crate::compose::{self, Service as ComposeService};
use crate::generate::{
    base_package, find_project_root, import_of, layout, main_dir, normalize_imports, package_of,
    subpackage, test_dir, write_new_file,
};
use crate::pom::{self, Dependency, Flavor, MIN_RELEASE, TARGET_RELEASE};
use clap::ValueEnum;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

mod messaging;
pub(crate) use messaging::*;

mod data;
pub(crate) use data::*;

mod testing;
pub(crate) use testing::*;

mod tooling;
pub(crate) use tooling::*;

mod database;
pub(crate) use database::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Capability {
    /// PostgreSQL + Flyway + Testcontainers + a compose service; raw SQL only, never an ORM
    #[value(alias = "postgres")]
    Db,
    /// Apache Kafka client + a compose broker (KRaft, no ZooKeeper)
    Kafka,
    /// Read CSV files into records (Apache Commons CSV)
    Csv,
    /// SQLite persistence: JDBC connections and a migration runner (sqlite-jdbc)
    Sqlite,
    /// Read and write JSON (Jackson databind)
    Json,
    /// Deterministic test helpers: clocks, ids, fixtures, in-process CLI runs
    Testkit,
    /// A scripted test double for any interface, driven by a lambda
    Fake,
    /// An HTTP server on the JDK's own httpserver -- no framework
    Http,
    /// Automatic formatting on `mvn verify` (Spotless + palantir-java-format)
    Format,
    /// RFC 9457 problem responses and bean validation, handled in one place
    #[value(alias = "errors")]
    Api,
    /// Actuator health, info and metrics, exposed narrowly rather than with `*`
    Actuator,
    /// Caching that is switched on, bounded, and proven by a test
    Cache,
    /// An explicit Spring Security filter chain, shaped for an API
    Security,
    /// Redis: a TTL-enforcing key/value wrapper, a compose service, and a
    /// real-container integration test
    Redis,
    /// Metrics: a Prometheus scrape endpoint, application-tagged meters, and
    /// meter names declared once rather than per call site
    #[value(alias = "metrics")]
    Observability,
    /// Network failure you can switch on: a Toxiproxy container in front of a
    /// dependency, so a test can cut the connection or add latency
    #[value(alias = "faults")]
    Toxiproxy,
}

impl Capability {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Capability::Db => "db",
            Capability::Kafka => "kafka",
            Capability::Csv => "csv",
            Capability::Sqlite => "sqlite",
            Capability::Json => "json",
            Capability::Testkit => "testkit",
            Capability::Fake => "fake",
            Capability::Http => "http",
            Capability::Format => "format",
            Capability::Api => "api",
            Capability::Actuator => "actuator",
            Capability::Cache => "cache",
            Capability::Security => "security",
            Capability::Redis => "redis",
            Capability::Observability => "observability",
            Capability::Toxiproxy => "toxiproxy",
        }
    }
}

/// A file a capability wants to create.
struct NewFile {
    path: PathBuf,
    contents: String,
}

/// Everything a capability wants to do to the project, computed before
/// anything is written so `--dry-run` can describe it without side effects.
#[derive(Default)]
struct Plan {
    deps: Vec<Dependency>,
    /// Build plugins to splice, as (artifactId, rendered `<plugin>` block).
    /// Plugin configuration is far too varied to model as a struct, so the
    /// capability renders the XML and pom.rs only places it.
    plugins: Vec<(&'static str, String)>,
    files: Vec<NewFile>,
    compose: Vec<ComposeService>,
    /// `application.properties` lines this capability owns, spliced into a
    /// `# jails:<label>` block so `remove` can take exactly them back out.
    properties: Vec<String>,
    /// Dependencies an *earlier* jails added for this capability and the
    /// current one no longer does. Removed by `remove`, never added by `add`.
    ///
    /// Without this a capability that drops an artifact leaves it behind
    /// forever: `remove <cap>` unsplices `plan.deps`, and the artifact is not
    /// in there any more. `add json` moving from Jackson 2 to Jackson 3 is the
    /// case that needed it -- leaving `jackson-datatype-jsr310` in the pom
    /// keeps a second Jackson major on the classpath, which is the exact
    /// failure the move was meant to fix.
    legacy_deps: Vec<Dependency>,
    /// Spring `add db` only: a test-classpath ApplicationContextInitializer
    /// so every `@SpringBootTest` gets a DataSource without editing those
    /// files. Docker Compose is skipped in tests by default.
    spring_test_import: Option<SpringTestImport>,
}

struct SpringTestImport {
    pkg: String,
    class: &'static str,
}

impl SpringTestImport {
    fn fqcn(&self) -> String {
        format!("{}.{}", self.pkg, self.class)
    }
}

const SPRING_DOCKER_COMPOSE: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-docker-compose",
    version: None,
    scope: None,
    optional: true,
};

/// Plan every requested capability before any of them is applied.
///
/// `jails add db kafka` applied them in turn, so a project that cannot have
/// the second was left with the first: `add` reported a failure over a
/// half-changed pom, and the obvious retry then had to skip `db` by hand.
/// Planning is pure, and it is where the refusals live (`require_spring`, a
/// release too old, an unreadable pom), so building all the plans first
/// turns that class of failure back into "nothing happened".
///
/// This is not a transaction and does not claim to be: an I/O error part-way
/// through the apply still leaves a partial change. It removes the failure
/// jails can actually see coming.
pub fn preflight(
    capabilities: &[Capability],
    name: Option<&str>,
    package: Option<&str>,
) -> Result<()> {
    if capabilities.len() < 2 {
        return Ok(());
    }
    let root = find_project_root()?;
    let pom_text = pom::read(&root)?;
    let flavor = pom::flavor(&pom_text);
    require_java_release(&pom_text)?;
    for &capability in capabilities {
        build_plan(capability, &root, flavor, name, package).map_err(|e| {
            format!(
                "{e}\n\nnothing was written -- `{}` was refused, so none of the {} \
                 requested capabilities were applied.",
                capability.label(),
                capabilities.len()
            )
        })?;
    }
    Ok(())
}

pub fn add(
    capability: Capability,
    name: Option<&str>,
    dry_run: bool,
    package: Option<&str>,
    debug: bool,
    no_start: bool,
) -> Result<()> {
    let root = find_project_root()?;
    let pom_text = pom::read(&root)?;
    let flavor = pom::flavor(&pom_text);
    require_java_release(&pom_text)?;
    let plan = build_plan(capability, &root, flavor, name, package)?;

    let mut updated_pom = pom_text.clone();
    let mut spliced: Vec<&Dependency> = Vec::new();
    for dep in &plan.deps {
        match pom::add_dependency(&updated_pom, dep)? {
            Some(next) => {
                updated_pom = next;
                spliced.push(dep);
            }
            None => println!("  exists  {}:{}", dep.group_id, dep.artifact_id),
        }
    }

    let mut spliced_plugins: Vec<&str> = Vec::new();
    for (artifact_id, body) in &plan.plugins {
        match pom::add_plugin(&updated_pom, artifact_id, body)? {
            Some(next) => {
                updated_pom = next;
                spliced_plugins.push(artifact_id);
            }
            None => println!("  exists  plugin {artifact_id}"),
        }
    }

    let mut compose_text = compose::read(&root)?;
    let mut compose_added: Vec<&ComposeService> = Vec::new();
    for svc in &plan.compose {
        match compose::add_service(&compose_text, svc) {
            Some(next) => {
                compose_text = next;
                compose_added.push(svc);
            }
            None => println!("  exists  compose service {}", svc.name),
        }
    }

    let mut docker_compose_dep = false;
    if flavor == Flavor::SpringBoot
        && !plan.compose.is_empty()
        && compose::has_services(&compose_text)
    {
        match pom::add_dependency(&updated_pom, &SPRING_DOCKER_COMPOSE)? {
            Some(next) => {
                updated_pom = next;
                docker_compose_dep = true;
            }
            None => println!(
                "  exists  {}:{}",
                SPRING_DOCKER_COMPOSE.group_id, SPRING_DOCKER_COMPOSE.artifact_id
            ),
        }
    }

    if dry_run {
        for dep in &spliced {
            println!(
                "  would add dependency  {}:{}",
                dep.group_id, dep.artifact_id
            );
        }
        if docker_compose_dep {
            println!(
                "  would add dependency  {}:{} (optional)",
                SPRING_DOCKER_COMPOSE.group_id, SPRING_DOCKER_COMPOSE.artifact_id
            );
        }
        for artifact_id in &spliced_plugins {
            println!("  would add plugin  {artifact_id}");
        }
        for file in &plan.files {
            let verb = if file.path.exists() {
                "would skip (exists)"
            } else {
                "would create"
            };
            println!("  {verb}  {}", rel(&root, &file.path));
        }
        for svc in &compose_added {
            println!("  would add compose service  {}", svc.name);
        }
        if let Some(cfg) = &plan.spring_test_import {
            install_db_properties(&root, true)?;
            install_test_container_import(&root, cfg, true)?;
        }
        install_capability_properties(&root, capability.label(), &plan.properties, true)?;
        return Ok(());
    }

    let pom_changed = !spliced.is_empty() || !spliced_plugins.is_empty() || docker_compose_dep;
    if pom_changed {
        std::fs::write(root.join("pom.xml"), &updated_pom)
            .map_err(|e| format!("failed to write pom.xml: {e}"))?;
        for dep in &spliced {
            println!("     dep  {}:{}", dep.group_id, dep.artifact_id);
        }
        if docker_compose_dep {
            println!(
                "     dep  {}:{} (optional)",
                SPRING_DOCKER_COMPOSE.group_id, SPRING_DOCKER_COMPOSE.artifact_id
            );
        }
        for artifact_id in &spliced_plugins {
            println!("  plugin  {artifact_id}");
        }
    }

    let mut created = 0;
    for file in &plan.files {
        if file.path.exists() {
            if should_replace_postgres_test_config(&file.path) {
                let contents = if file.path.extension().is_some_and(|e| e == "java") {
                    normalize_imports(&file.contents)
                } else {
                    file.contents.clone()
                };
                fs::write(&file.path, &contents)
                    .map_err(|e| format!("failed to write {}: {e}", file.path.display()))?;
                println!("  update  {}", rel(&root, &file.path));
                created += 1;
            } else {
                println!("  exists  {}", rel(&root, &file.path));
            }
            continue;
        }
        write_new_file(&root, &file.path, &file.contents)?;
        println!("  create  {}", rel(&root, &file.path));
        created += 1;
    }

    if !compose_added.is_empty() {
        compose::write(&root, &compose_text)?;
        println!("  compose {}", rel(&root, &compose::path(&root)));
        for svc in &compose_added {
            println!("  service {}", svc.name);
        }
    }

    let mut tests_wired = false;
    if let Some(cfg) = &plan.spring_test_import {
        tests_wired = install_db_properties(&root, false)?;
        tests_wired |= remove_legacy_spring_factories(&root)?;
        tests_wired |= install_test_container_import(&root, cfg, false)?;
    }
    tests_wired |=
        install_capability_properties(&root, capability.label(), &plan.properties, false)?;

    // Same rule as the generators: a capability that writes an `*IT` has to
    // make sure something runs it. Failsafe is not in the Spring Boot
    // parent's default build, so without this `mvn verify` completes,
    // reports success, and executes none of them.
    if plan
        .files
        .iter()
        .any(|f| f.path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with("IT.java")))
        && let Some(next) = pom::add_plugin(
            &updated_pom,
            crate::spring::FAILSAFE_ARTIFACT,
            crate::spring::FAILSAFE_PLUGIN,
        )?
    {
        std::fs::write(root.join("pom.xml"), &next)
            .map_err(|e| format!("failed to write pom.xml: {e}"))?;
        println!("  plugin  {}", crate::spring::FAILSAFE_ARTIFACT);
        tests_wired = true;
    }

    if created == 0 && !pom_changed && compose_added.is_empty() && !tests_wired {
        // Still recorded: the capability *is* part of this project, and a
        // manifest that omits it because it happened to be installed before
        // the manifest existed is a manifest `sync` would act on wrongly.
        crate::config::record_capability(&root, capability.label())?;
        println!("{} is already set up -- nothing to do", capability.label());
        return Ok(());
    }

    // Installing a formatter that immediately fails `mvn verify` is a bad
    // trade: the wrapping it wants is not something a template can predict, so
    // run it once and leave the project green.
    //
    // And if it cannot run at all, undo the pom edit. A formatter bound to
    // `verify` that crashes on this toolchain turns a working project into one
    // that cannot build -- palantir-java-format does exactly that when its
    // pinned version predates the JDK on PATH, which is a bad thing for a
    // scaffolding tool to leave behind.
    if matches!(capability, Capability::Format) {
        if crate::run::fmt_quietly(&root) {
            println!("  format  applied to the existing sources");
        } else {
            std::fs::write(root.join("pom.xml"), &pom_text)
                .map_err(|e| format!("failed to restore pom.xml: {e}"))?;
            return Err(
                "the formatter could not run on this toolchain, so pom.xml was left unchanged.\n       \
                 palantir-java-format needs a JDK it was built against -- try a current LTS (Java 25),\n       \
                 or configure Spotless yourself if you need a different formatter."
                    .to_string(),
            );
        }
    }

    if !compose_added.is_empty() && !no_start {
        let names: Vec<&str> = compose_added.iter().map(|s| s.name).collect();
        if compose::up(&root, &names, debug) {
            println!("  start   {}", names.join(", "));
        } else {
            println!(
                "  note    start with `{}`",
                compose::missing_docker_hint(&names)
            );
        }
    } else if !compose_added.is_empty() {
        let names: Vec<&str> = compose_added.iter().map(|s| s.name).collect();
        println!(
            "  note    start with `{}`",
            compose::missing_docker_hint(&names)
        );
    }

    // The manifest is written last, so it records what actually landed.
    crate::config::record_capability(&root, capability.label())?;
    println!(
        "added {} ({})",
        capability.label(),
        match flavor {
            Flavor::SpringBoot => "spring boot",
            Flavor::PlainMaven => "plain maven",
        }
    );
    Ok(())
}

/// Make the project match what `jails.toml` says it is made of.
///
/// The manifest is the point: `add` records every capability it applies, so
/// the file is a true description of the project rather than one somebody has
/// to remember to update. `sync` reads it back and applies whatever is
/// missing.
///
/// What that buys, in the order it matters:
///
/// - A fresh clone becomes the project it claims to be in one command,
///   instead of whoever set it up recalling which `jails add` calls they ran.
/// - A project regenerates against a newer jails. The rewards audit ends with
///   exactly this problem -- a project still carrying hand-written files that
///   jails now produces, with no way to take the improvements but to redo the
///   commands.
/// - `--pretend` answers "what is this project missing?" without writing.
///
/// Every capability is idempotent and reports what is already there, so a
/// `sync` over a project that is already correct changes nothing and says so.
pub fn sync(dry_run: bool, debug: bool, no_start: bool) -> Result<()> {
    use clap::ValueEnum;

    let root = find_project_root()?;
    let config = crate::config::Config::load(&root)?;
    let labels = config.capabilities();

    if labels.is_empty() {
        println!(
            "{} declares no capabilities, so there is nothing to sync.\n\n\
             `jails add <capability>` records what it applies, so the file\n\
             describes the project from then on. To adopt a project that was\n\
             built before the manifest existed, re-run the `add` calls it had:\n\
             each one reports what is already there and changes nothing else.",
            crate::config::FILE
        );
        return Ok(());
    }

    // Parsing is validated at load, so an unknown label cannot reach here --
    // but resolving every one before applying any keeps `sync` consistent
    // with `add A B`, which preflights for the same reason.
    let mut capabilities = Vec::with_capacity(labels.len());
    for label in labels {
        let capability = Capability::value_variants()
            .iter()
            .find(|c| c.label() == label)
            .copied()
            .ok_or_else(|| format!("{}: unknown capability `{label}`", crate::config::FILE))?;
        capabilities.push(capability);
    }
    preflight(&capabilities, None, None)?;

    println!(
        "{} declares {}: {}\n",
        crate::config::FILE,
        match capabilities.len() {
            1 => "1 capability".to_string(),
            n => format!("{n} capabilities"),
        },
        labels.join(", ")
    );
    for capability in capabilities {
        add(capability, None, dry_run, None, debug, no_start)?;
    }
    Ok(())
}

/// Inverse of [`add`]: unsplice the same pom entries, delete the same files,
/// take compose services out, and stop their containers.
pub fn remove(
    capability: Capability,
    name: Option<&str>,
    dry_run: bool,
    force: bool,
    package: Option<&str>,
    debug: bool,
) -> Result<()> {
    let root = find_project_root()?;
    let pom_text = pom::read(&root)?;
    let flavor = pom::flavor(&pom_text);
    let plan = build_plan(capability, &root, flavor, name, package)?;

    let mut updated_pom = pom_text.clone();
    let mut removed_deps: Vec<&Dependency> = Vec::new();
    for dep in plan.deps.iter().chain(plan.legacy_deps.iter()) {
        match pom::remove_dependency(&updated_pom, dep.group_id, dep.artifact_id)? {
            Some(next) => {
                updated_pom = next;
                removed_deps.push(dep);
            }
            None => {}
        }
    }

    let mut removed_plugins: Vec<&str> = Vec::new();
    for (artifact_id, _) in &plan.plugins {
        match pom::remove_plugin(&updated_pom, artifact_id)? {
            Some(next) => {
                updated_pom = next;
                removed_plugins.push(artifact_id);
            }
            None => {}
        }
    }

    let existing_files: Vec<&PathBuf> = plan
        .files
        .iter()
        .map(|f| &f.path)
        .filter(|p| p.exists())
        .collect();

    let mut compose_text = compose::read(&root)?;
    let mut compose_removed: Vec<&ComposeService> = Vec::new();
    for svc in &plan.compose {
        match compose::remove_service(&compose_text, svc) {
            Some(next) => {
                compose_text = next;
                compose_removed.push(svc);
            }
            None => {}
        }
    }

    let mut docker_compose_dep = false;
    if flavor == Flavor::SpringBoot && !compose::has_services(&compose_text) {
        match pom::remove_dependency(
            &updated_pom,
            SPRING_DOCKER_COMPOSE.group_id,
            SPRING_DOCKER_COMPOSE.artifact_id,
        )? {
            Some(next) => {
                updated_pom = next;
                docker_compose_dep = true;
            }
            None => {}
        }
    }
    if flavor == Flavor::SpringBoot && plan.spring_test_import.is_some() {
        match pom::remove_dependency(
            &updated_pom,
            SPRING_TESTCONTAINERS.group_id,
            SPRING_TESTCONTAINERS.artifact_id,
        )? {
            Some(next) => {
                updated_pom = next;
                removed_deps.push(&SPRING_TESTCONTAINERS);
            }
            None => {}
        }
    }

    let pom_changed = !removed_deps.is_empty() || !removed_plugins.is_empty() || docker_compose_dep;
    let factories_present = plan.spring_test_import.as_ref().is_some_and(|cfg| {
        fs::read_to_string(spring_factories_path(&root)).is_ok_and(|s| s.contains(&cfg.fqcn()))
    });
    let properties_present = plan.spring_test_import.is_some()
        && fs::read_to_string(application_properties_path(&root))
            .is_ok_and(|s| s.contains(EXCEPTION_TRANSLATION_PROPERTY));
    let tests_to_unwire: Vec<PathBuf> = plan
        .spring_test_import
        .as_ref()
        .map(|cfg| {
            find_spring_boot_tests(&root.join("src/test/java"))
                .into_iter()
                .filter(|p| {
                    fs::read_to_string(p).is_ok_and(|s| s.contains(&import_annotation(cfg.class)))
                })
                .collect()
        })
        .unwrap_or_default();
    if !pom_changed
        && existing_files.is_empty()
        && compose_removed.is_empty()
        && tests_to_unwire.is_empty()
        && !factories_present
        && !properties_present
    {
        println!("{} is not set up -- nothing to do", capability.label());
        return Ok(());
    }

    if dry_run {
        for dep in &removed_deps {
            println!(
                "  would remove dependency  {}:{}",
                dep.group_id, dep.artifact_id
            );
        }
        if docker_compose_dep {
            println!(
                "  would remove dependency  {}:{}",
                SPRING_DOCKER_COMPOSE.group_id, SPRING_DOCKER_COMPOSE.artifact_id
            );
        }
        for artifact_id in &removed_plugins {
            println!("  would remove plugin  {artifact_id}");
        }
        for path in &existing_files {
            println!("  would delete  {}", rel(&root, path));
        }
        report_edited_files(&root, &plan);
        for svc in &compose_removed {
            println!("  would remove compose service  {}", svc.name);
        }
        for path in &tests_to_unwire {
            println!("  would unsplice @Import from {}", rel(&root, path));
        }
        report_unowned_properties(&root, capability.label(), &plan.properties);
        if factories_present {
            println!(
                "  would unsplice {}",
                rel(&root, &spring_factories_path(&root))
            );
        }
        if properties_present {
            println!(
                "  would unsplice {}",
                rel(&root, &application_properties_path(&root))
            );
        }
        return Ok(());
    }

    if !force {
        println!("about to remove {}:", capability.label());
        for dep in &removed_deps {
            println!("  dep {}:{}", dep.group_id, dep.artifact_id);
        }
        if docker_compose_dep {
            println!(
                "  dep {}:{}",
                SPRING_DOCKER_COMPOSE.group_id, SPRING_DOCKER_COMPOSE.artifact_id
            );
        }
        for artifact_id in &removed_plugins {
            println!("  plugin {artifact_id}");
        }
        for path in &existing_files {
            println!("  {}", rel(&root, path));
        }
        report_edited_files(&root, &plan);
        for svc in &compose_removed {
            println!("  compose {}", svc.name);
        }
        for path in &tests_to_unwire {
            println!("  import in {}", rel(&root, path));
        }
        report_unowned_properties(&root, capability.label(), &plan.properties);
        if factories_present {
            println!("  {}", rel(&root, &spring_factories_path(&root)));
        }
        if properties_present {
            println!("  {}", rel(&root, &application_properties_path(&root)));
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
    } else {
        // `--force` skips the prompt, which is the whole silent path: without
        // this, a scripted `remove --force` deletes a hand-finished class and
        // says only "removed csv".
        report_edited_files(&root, &plan);
    }

    if pom_changed {
        std::fs::write(root.join("pom.xml"), &updated_pom)
            .map_err(|e| format!("failed to write pom.xml: {e}"))?;
        for dep in &removed_deps {
            println!("  remove  {}:{}", dep.group_id, dep.artifact_id);
        }
        if docker_compose_dep {
            println!(
                "  remove  {}:{}",
                SPRING_DOCKER_COMPOSE.group_id, SPRING_DOCKER_COMPOSE.artifact_id
            );
        }
        for artifact_id in &removed_plugins {
            println!("  remove  plugin {artifact_id}");
        }
    }

    for path in existing_files {
        std::fs::remove_file(path)
            .map_err(|e| format!("failed to remove {}: {e}", path.display()))?;
        println!("  delete  {}", rel(&root, path));
        delete_maven_output(&root, path);
    }

    if !compose_removed.is_empty() {
        let names: Vec<&str> = compose_removed.iter().map(|s| s.name).collect();
        compose::stop(&root, &names, debug);
        compose::write(&root, &compose_text)?;
        if compose_text.is_empty() {
            println!("  delete  {}", rel(&root, &compose::path(&root)));
        } else {
            println!("  compose {}", rel(&root, &compose::path(&root)));
        }
        for svc in &compose_removed {
            println!("  stop    {}", svc.name);
        }
    }

    if let Some(cfg) = &plan.spring_test_import {
        uninstall_postgres_test_initializer(&root, cfg)?;
        delete_maven_output(&root, &spring_factories_path(&root));
        uninstall_db_properties(&root)?;
        delete_maven_output(&root, &application_properties_path(&root));
        let _ = strip_legacy_postgres_imports(&root, cfg)?;
    }
    if !plan.properties.is_empty() {
        remove_capability_properties(&root, capability.label())?;
        delete_maven_output(&root, &application_properties_path(&root));
    }

    // The exact inverse of the record in `add`: left listed, the next `sync`
    // would put back what was just removed.
    crate::config::forget_capability(&root, capability.label())?;
    println!("removed {}", capability.label());
    Ok(())
}

fn require_java_release(pom_text: &str) -> Result<()> {
    match pom::release_level(pom_text) {
        Some(level) if level < MIN_RELEASE => Err(format!(
            "this project targets Java {level}, but jails generates Java {MIN_RELEASE}+ code.\n       \
             Raise <maven.compiler.release> (or <java.version>) to at least {MIN_RELEASE} in pom.xml and try again."
        )),
        None => Err(format!(
            "pom.xml does not set a Java release level, and jails generates Java {MIN_RELEASE}+ code.\n       \
             Add <maven.compiler.release>{TARGET_RELEASE}</maven.compiler.release> to <properties> and try again."
        )),
        Some(_) => Ok(()),
    }
}

fn build_plan(
    capability: Capability,
    root: &Path,
    flavor: Flavor,
    name: Option<&str>,
    package: Option<&str>,
) -> Result<Plan> {
    let base = base_package(root)?;
    let config = crate::config::Config::load(root)?;
    let place = |default: &str| subpackage(&base, package.unwrap_or(config.layer(default)));
    match capability {
        Capability::Db => db_plan(root, flavor, &place("")),
        Capability::Kafka => kafka_plan(root, flavor, &place(crate::generate::layout::MESSAGING)),
        Capability::Csv => csv_plan(root, &place(layout::ADAPTERS), flavor, name),
        Capability::Sqlite => sqlite_plan(root, &place(layout::ADAPTERS), flavor, name),
        Capability::Json => json_plan(root, &place(layout::ADAPTERS), flavor, name),
        Capability::Testkit => testkit_plan(root, &place(layout::TESTKIT)),
        Capability::Fake => fake_plan(root, &place(layout::TESTKIT)),
        Capability::Http => http_plan(root, &place(layout::API), name),
        Capability::Format => format_plan(),
        Capability::Api => spring_slice_plan(
            crate::spring::api_slice(root, &place(layout::API)),
            flavor,
            "api",
        ),
        Capability::Actuator => spring_slice_plan(
            crate::spring::actuator_slice(root, &place("")),
            flavor,
            "actuator",
        ),
        Capability::Cache => spring_slice_plan(
            crate::spring::cache_slice(root, &place("")),
            flavor,
            "cache",
        ),
        Capability::Security => spring_slice_plan(
            crate::spring::security_slice(root, &place("")),
            flavor,
            "security",
        ),
        Capability::Observability => spring_slice_plan(
            crate::spring::observability_slice(root, &place("")),
            flavor,
            "observability",
        ),
        Capability::Toxiproxy => toxiproxy_plan(root, &place(layout::TESTKIT)),
        Capability::Redis => spring_slice_plan(
            crate::spring::redis_slice(root, &place(layout::ADAPTERS)),
            flavor,
            "redis",
        )
        .map(|plan| Plan {
            compose: vec![compose::REDIS],
            ..plan
        }),
    }
}

/// Adapt a Spring-only slice to the shape `add` already executes. The Spring
/// check lives here rather than in each slice so there is one message for it.
fn spring_slice_plan(
    slice: crate::spring::SpringSlice,
    flavor: Flavor,
    capability: &str,
) -> Result<Plan> {
    crate::spring::require_spring(flavor, capability)?;
    Ok(Plan {
        deps: slice.deps,
        files: slice
            .files
            .into_iter()
            .map(|(path, contents)| NewFile { path, contents })
            .collect(),
        properties: slice.properties,
        ..Plan::default()
    })
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_reader_uses_the_builder_api_matching_the_pinned_version() {
        // 1.13 renamed build() to get(); emitting the wrong one is a compile
        // error that only shows up in the real-toolchain tests.
        let src = csv_reader_java("com.example.demo", "CsvReader");
        assert_eq!(COMMONS_CSV.version, Some("1.14.1"));
        assert!(src.contains(".get();"));
        assert!(!src.contains(".build();"));
    }

    #[test]
    fn csv_reader_is_generated_into_the_projects_package() {
        let src = csv_reader_java("com.example.demo", "CsvReader");
        assert!(src.starts_with("package com.example.demo;\n"));
        assert!(src.contains("public final class CsvReader"));
    }

    #[test]
    fn csv_reader_uses_modern_java_idioms() {
        let src = csv_reader_java("com.example.demo", "CsvReader");
        assert!(
            src.contains("public record Row("),
            "rows should be a record"
        );
        assert!(src.contains(".toList()"), "should use Stream.toList()");
        assert!(
            src.contains("try (var reader"),
            "should use try-with-resources"
        );
        assert!(
            !src.contains("java.io.File"),
            "should use NIO paths, not File"
        );
    }

    #[test]
    fn csv_name_override_renames_the_class_and_its_test() {
        let src = csv_reader_java("com.example.demo", "TransactionReader");
        assert!(src.contains("public final class TransactionReader"));
        let test = csv_reader_test_java("com.example.demo", "TransactionReader");
        assert!(test.contains("class TransactionReaderTest"));
        assert!(test.contains("TransactionReader.read("));
    }

    #[test]
    fn sqlite_uses_stdlib_jdbc_and_no_orm() {
        let db = database_java("com.example.demo", "Database");
        assert!(db.contains("public record Database(Path file)"));
        assert!(db.contains("java.sql.DriverManager"));
        assert!(db.contains("jdbc:sqlite:"));

        let migrations = migrations_java("com.example.demo", "Migrations");
        assert!(
            migrations.contains("schema_migrations"),
            "applied scripts must be tracked"
        );
        assert!(
            migrations.contains("connection.rollback()"),
            "a failed script must not half-apply"
        );
        assert!(migrations.contains("\"\"\""), "SQL should use a text block");
        // The generated helper uses only JDBC and its own migration table.
        assert!(!migrations.contains("org.springframework"));
        assert!(!db.contains("org.springframework"));
    }

    #[test]
    fn sqlite_name_override_renames_both_classes_consistently() {
        let db = database_java("com.example.demo", "LedgerDatabase");
        assert!(db.contains("public record LedgerDatabase(Path file)"));
        assert!(db.contains("public static LedgerDatabase inMemory()"));

        let test = database_test_java("com.example.demo", "LedgerDatabase", "LedgerMigrations");
        assert!(test.contains("class LedgerDatabaseTest"));
        assert!(test.contains("LedgerMigrations.applyAll("));
    }

    #[test]
    fn json_pins_a_version_only_when_no_parent_manages_it() {
        // Spring Boot's parent already pins Jackson; declaring our own version
        // would override the curated one.
        assert_eq!(JACKSON.version, Some("3.0.1"));
        let root = std::path::Path::new("/tmp/does-not-matter");
        let spring = json_plan(root, "com.example.demo", Flavor::SpringBoot, None).unwrap();
        assert!(spring.deps.iter().all(|d| d.version.is_none()));
        let plain = json_plan(root, "com.example.demo", Flavor::PlainMaven, None).unwrap();
        assert!(
            plain
                .deps
                .iter()
                .all(|d| d.version == Some(JACKSON_VERSION))
        );
    }

    /// Moving a capability to a new artifact is only half a migration: the
    /// old one has to come *out*, or `remove json && add json` leaves the 2.x
    /// line beside the 3.x one -- the exact two-majors failure the move fixes.
    #[test]
    fn remove_json_also_unsplices_the_jackson_2_artifacts_it_no_longer_adds() {
        let root = std::path::Path::new("/tmp/does-not-matter");
        let plan = json_plan(root, "com.example.demo", Flavor::PlainMaven, None).unwrap();
        let legacy: Vec<(&str, &str)> = plan
            .legacy_deps
            .iter()
            .map(|d| (d.group_id, d.artifact_id))
            .collect();
        assert!(
            legacy.contains(&("com.fasterxml.jackson.datatype", "jackson-datatype-jsr310")),
            "{legacy:?}"
        );
        assert!(
            legacy.contains(&("com.fasterxml.jackson.core", "jackson-databind")),
            "the 2.x databind is a different artifact from the 3.x one: {legacy:?}"
        );
        // ...and none of them are added.
        let added: Vec<&str> = plan.deps.iter().map(|d| d.group_id).collect();
        assert_eq!(added, vec!["tools.jackson.core"]);
    }

    /// Jackson 3 has java.time in databind, so the 2.x second artifact is not
    /// merely unnecessary -- adding it would put a second Jackson major on the
    /// classpath, which is the bug this capability had.
    #[test]
    fn json_ships_one_artifact_because_jackson_3_has_java_time_built_in() {
        let root = std::path::Path::new("/tmp/does-not-matter");
        for flavor in [Flavor::SpringBoot, Flavor::PlainMaven] {
            let plan = json_plan(root, "com.example.demo", flavor, None).unwrap();
            let artifacts: Vec<&str> = plan.deps.iter().map(|d| d.artifact_id).collect();
            assert_eq!(
                artifacts,
                vec!["jackson-databind"],
                "{flavor:?} should get databind and nothing else"
            );
            assert!(
                plan.deps.iter().all(|d| d.group_id == "tools.jackson.core"),
                "{flavor:?}: Jackson 3 changed groupId; the 2.x one is a different library"
            );
        }
    }

    /// `read(path, type)` loses the whole document to one bad element, so the
    /// generated class has to offer a tree route for untrusted input.
    #[test]
    fn json_offers_a_tree_api_for_input_whose_shape_is_not_trusted() {
        let src = json_java("com.example.demo", "Json");
        assert!(src.contains("public static JsonNode readTree(Path path)"));
        assert!(src.contains("public static <T> T convert(JsonNode node, Class<T> type)"));
        assert!(src.contains("import tools.jackson.databind.JsonNode;"));

        let test = json_test_java("com.example.demo", "Json");
        assert!(test.contains("keepsGoodElementsWhenSiblingsAreMalformed"));
        assert!(test.contains("writesDatesAsIsoStringsNotObjects"));
    }

    /// JSON Lines is the format event logs use, and one malformed line must
    /// not cost the whole file -- so it returns trees, like readTree.
    #[test]
    fn json_reads_jsonl_as_a_list_of_trees() {
        let src = json_java("com.example.demo", "Json");
        assert!(
            src.contains("public static List<JsonNode> readJsonl(Path path)"),
            "{src}"
        );
        assert!(
            src.contains("isBlank"),
            "blank lines should be skipped: {src}"
        );

        let test = json_test_java("com.example.demo", "Json");
        assert!(test.contains("readJsonl"));
        assert!(test.contains("readsAnEmptyJsonlFileAsNoEvents"));
    }

    #[test]
    fn json_uses_nio_streams_rather_than_file() {
        let src = json_java("com.example.demo", "Json");
        assert!(src.contains("Files.newInputStream"));
        assert!(src.contains("Files.newOutputStream"));
        assert!(
            !src.contains("java.io.File"),
            "should not fall back to java.io.File"
        );
        assert!(
            src.contains("private static final JsonMapper MAPPER"),
            "mapper should be shared"
        );
    }

    /// validation/09 addresses the scripted double as `Fake`; the class and
    /// its file have to agree with that.
    #[test]
    fn the_scripted_double_is_called_fake() {
        let src = scripted_java("com.example.demo.testkit");
        assert!(src.contains("public final class Fake<T>"), "{src}");
        assert!(!src.contains("Scripted"), "no trace of the old name: {src}");

        let test = scripted_test_java("com.example.demo.testkit");
        assert!(test.contains("class FakeTest"));
        assert!(!test.contains("Scripted"));
    }

    #[test]
    fn capitalize_uppercases_the_first_letter_only() {
        assert_eq!("Csv", capitalize("csv"));
        assert_eq!("Transaction", capitalize("transaction"));
        assert_eq!("", capitalize(""));
    }

    #[test]
    fn db_plan_ships_compose_postgres_and_never_an_orm() {
        let root = std::path::Path::new("/tmp/does-not-matter");
        let plan = db_plan(root, Flavor::PlainMaven, "com.example.demo").unwrap();
        assert_eq!(plan.compose, vec![compose::POSTGRES]);
        let artifacts: Vec<&str> = plan.deps.iter().map(|d| d.artifact_id).collect();
        assert!(artifacts.contains(&"postgresql"));
        assert!(artifacts.contains(&"flyway-core"));
        assert!(artifacts.contains(&"testcontainers-postgresql"));
        assert!(
            !artifacts
                .iter()
                .any(|a| a.contains("hibernate") || a.contains("jpa"))
        );
        assert!(
            !plan
                .deps
                .iter()
                .any(|d| d.artifact_id == "spring-boot-docker-compose"),
            "the docker-compose module is added at apply time, and only for Spring"
        );
        assert!(plan.spring_test_import.is_none());
        assert!(
            !plan
                .files
                .iter()
                .any(|f| f.path.ends_with("TestcontainersConfig.java"))
        );
    }

    #[test]
    fn db_plan_on_spring_writes_an_importable_container_config() {
        let root = std::path::Path::new("/tmp/does-not-matter");
        let plan = db_plan(root, Flavor::SpringBoot, "com.example.demo").unwrap();
        let artifacts: Vec<&str> = plan.deps.iter().map(|d| d.artifact_id).collect();
        assert!(artifacts.contains(&"spring-boot-starter-jdbc"));
        assert!(
            artifacts.contains(&"spring-boot-testcontainers"),
            "@ServiceConnection and the container-bean lifecycle live in that module"
        );
        assert!(plan.files.iter().any(|f| {
            f.path
                .ends_with("src/test/java/com/example/demo/TestcontainersConfig.java")
        }));
        let src = testcontainers_config_java("com.example.demo");
        // Imported by the tests that need a database, not registered for all
        // of them: a global registration made every pure slice and
        // @WebMvcTest start PostgreSQL. `add db` splices the @Import into the
        // @SpringBootTest classes that already exist, which is what stops the
        // original "no suitable driver class" problem from coming back.
        assert!(
            !src.contains("ApplicationContextInitializer"),
            "the global initializer is what this replaced: {src}"
        );
        assert!(!src.contains("AnnotatedBeanDefinitionReader"), "{src}");
        // The container is a bean with @ServiceConnection, which is how
        // connection details reach auto-configuration.
        assert!(src.contains("@ServiceConnection"));
        assert!(src.contains("@TestConfiguration"));
        // Nothing starts the container by hand -- the lifecycle initializer
        // in spring-boot-testcontainers does that for a container bean.
        assert!(!src.contains(".start()"), "{src}");
        assert!(!src.contains("MapPropertySource"), "{src}");
        assert!(src.contains("org.testcontainers.postgresql.PostgreSQLContainer"));
        assert!(
            src.contains(POSTGRES_IMAGE),
            "container image must match compose: {src}"
        );
        assert!(
            compose::POSTGRES.body.contains(POSTGRES_IMAGE),
            "compose postgres image drifted from POSTGRES_IMAGE"
        );
        assert!(!src.contains("GenericContainer"));
        let cfg = plan.spring_test_import.unwrap();
        assert_eq!(cfg.pkg, "com.example.demo");
        assert_eq!(cfg.class, "TestcontainersConfig");
        assert_eq!(cfg.fqcn(), "com.example.demo.TestcontainersConfig");
    }

    /// A real project tuned Kafka inside jails' own marked block -- an
    /// ErrorHandlingDeserializer, acks=all, a KIP-848 opt-in. `remove kafka`
    /// would have deleted all of it without a word.
    #[test]
    fn hand_written_properties_inside_a_marked_block_are_reported() {
        let owned = vec![
            "spring.kafka.bootstrap-servers=localhost:9092".to_string(),
            "spring.kafka.consumer.group-id=rewards".to_string(),
        ];
        let existing = "spring.application.name=rewards\n\
                        # jails:kafka\n\
                        spring.kafka.bootstrap-servers=localhost:9092\n\
                        spring.kafka.consumer.group-id=rewards\n\
                        # a comment jails wrote\n\
                        spring.kafka.producer.acks=all\n\
                        spring.kafka.consumer.properties.group.protocol=consumer\n\
                        # /jails:kafka\n";
        let unowned = unowned_properties(existing, "kafka", &owned);
        assert_eq!(
            unowned,
            vec![
                "spring.kafka.producer.acks=all",
                "spring.kafka.consumer.properties.group.protocol=consumer"
            ]
        );
    }

    #[test]
    fn properties_outside_the_block_are_not_reported_as_unowned() {
        let owned = vec!["a=1".to_string()];
        let existing = "untouched=yes\n# jails:db\na=1\n# /jails:db\nalso.untouched=yes\n";
        assert!(unowned_properties(existing, "db", &owned).is_empty());
    }

    #[test]
    fn spring_factories_block_is_idempotent_to_remove() {
        let fqcn = "com.example.demo.PostgresContainerConfig";
        let block = spring_factories_block(fqcn);
        assert!(block.contains("# jails:db"));
        assert!(block.contains(SPRING_FACTORIES_KEY));
        assert!(block.contains(fqcn));

        let gone = remove_jails_db_block(&block, fqcn).unwrap();
        assert!(gone.trim().is_empty());

        let other =
            format!("org.springframework.context.ApplicationListener=com.example.Other\n{block}");
        let next = remove_jails_db_block(&other, fqcn).unwrap();
        assert!(next.contains("com.example.Other"));
        assert!(!next.contains(fqcn));
        assert!(!next.contains("# jails:db"));
        assert!(remove_jails_db_block("unrelated\n", fqcn).is_none());
    }

    #[test]
    fn application_properties_block_disables_exception_translation() {
        let block = application_properties_block(&compose::PostgresConnect::defaults());
        assert!(block.contains(EXCEPTION_TRANSLATION_PROPERTY));
        let gone = remove_jails_db_block(&block, EXCEPTION_TRANSLATION_PROPERTY).unwrap();
        assert!(gone.trim().is_empty());

        let existing = format!("spring.application.name=demo\n{block}");
        let next = remove_jails_db_block(&existing, EXCEPTION_TRANSLATION_PROPERTY).unwrap();
        assert!(next.contains("spring.application.name=demo"));
        assert!(!next.contains("exceptiontranslation"));
    }

    #[test]
    fn application_properties_carry_the_compose_datasource() {
        // The app needs its own connection: Spring's docker-compose module
        // supplies one where it works, but it cannot drive every provider,
        // and a dead datasource kills startup before any code runs.
        let block = application_properties_block(&compose::PostgresConnect::defaults());
        assert!(
            block.contains("spring.datasource.url=jdbc:postgresql://localhost:5432/app"),
            "{block}"
        );
        assert!(block.contains("spring.datasource.username=app"), "{block}");
        assert!(block.contains("spring.datasource.password=app"), "{block}");
        // Spring's compose module duplicates what `jails run`/`jails start`
        // already do, and cannot drive every compose provider.
        assert!(block.contains(COMPOSE_DISABLED_PROPERTY), "{block}");
    }

    #[test]
    fn the_datasource_follows_an_edited_compose_file() {
        let connect = compose::PostgresConnect {
            host: "localhost".into(),
            port: 5544,
            user: "rewards".into(),
            password: "secret".into(),
            database: "rewards".into(),
        };
        let block = application_properties_block(&connect);
        assert!(
            block.contains("spring.datasource.url=jdbc:postgresql://localhost:5544/rewards"),
            "{block}"
        );
        assert!(block.contains("spring.datasource.username=rewards"), "{block}");
    }

    #[test]
    fn maven_output_maps_java_and_resources_into_target() {
        let root = std::path::Path::new("/tmp/demo");
        assert_eq!(
            maven_output_for(
                root,
                &root.join("src/test/java/com/example/demo/PostgresContainerConfig.java")
            ),
            Some(root.join("target/test-classes/com/example/demo/PostgresContainerConfig.class"))
        );
        assert_eq!(
            maven_output_for(
                root,
                &root.join("src/test/resources/META-INF/spring.factories")
            ),
            Some(root.join("target/test-classes/META-INF/spring.factories"))
        );
        assert_eq!(
            maven_output_for(
                root,
                &root.join("src/main/resources/application.properties")
            ),
            Some(root.join("target/classes/application.properties"))
        );
    }

    #[test]
    fn splice_import_lands_above_spring_boot_test_and_is_idempotent_to_unsplice() {
        let source = r#"package com.example.demo;

import org.junit.jupiter.api.Test;
import org.springframework.boot.test.context.SpringBootTest;

@SpringBootTest
class DemoApplicationTests {

    @Test
    void contextLoads() {}
}
"#;
        let spliced =
            splice_spring_boot_test_import(source, "PostgresContainerConfig", "").unwrap();
        assert!(spliced.contains("@Import(PostgresContainerConfig.class)"));
        assert!(spliced.contains("import org.springframework.context.annotation.Import;"));
        let import_at = spliced
            .find("@Import(PostgresContainerConfig.class)")
            .unwrap();
        let boot_at = spliced.find("@SpringBootTest").unwrap();
        assert!(import_at < boot_at, "{spliced}");

        let restored =
            unsplice_spring_boot_test_import(&spliced, "PostgresContainerConfig", "").unwrap();
        assert!(!restored.contains("PostgresContainerConfig"));
        assert!(!restored.contains("org.springframework.context.annotation.Import"));
        assert!(restored.contains("@SpringBootTest"));

        let extra = "import com.example.demo.testkit.PostgresContainerConfig;\n";
        let other_pkg =
            splice_spring_boot_test_import(source, "PostgresContainerConfig", extra).unwrap();
        assert!(other_pkg.contains("import com.example.demo.testkit.PostgresContainerConfig;"));
        let round_trip =
            unsplice_spring_boot_test_import(&other_pkg, "PostgresContainerConfig", extra).unwrap();
        assert!(!round_trip.contains("testkit.PostgresContainerConfig"));
    }

    #[test]
    fn kafka_plan_is_a_client_plus_a_compose_broker() {
        // The Spring path reads the base package, for the deserializer's
        // trusted-packages list -- so it needs a project to read.
        let root = std::env::temp_dir().join(format!(
            "jails-kafka-plan-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let pkg = root.join("src/main/java/com/example/demo");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(
            pkg.join("DemoApplication.java"),
            "package com.example.demo;\npublic class DemoApplication {}\n",
        )
        .unwrap();

        let spring = kafka_plan(&root, Flavor::SpringBoot, "com.example.app.messaging").unwrap();
        assert_eq!(spring.deps[0].artifact_id, "spring-boot-starter-kafka");
        assert!(spring.deps[0].version.is_none());
        assert_eq!(spring.compose, vec![compose::KAFKA]);
        // A consumer that starts at the end of the topic sees nothing that
        // was published before it joined -- the commonest Kafka surprise.
        assert!(
            spring
                .properties
                .iter()
                .any(|p| p == "spring.kafka.consumer.auto-offset-reset=earliest"),
            "{:?}",
            spring.properties
        );
        // The Jackson-prefixed serializers: the older pair is deprecated for
        // removal since Spring Kafka 4.0.
        assert!(
            spring.properties.iter().any(|p| p.contains("JacksonJsonSerializer")),
            "{:?}",
            spring.properties
        );
        assert!(
            spring
                .properties
                .iter()
                .any(|p| p.ends_with("trusted.packages=com.example.demo,com.example.demo.*")),
            "{:?}",
            spring.properties
        );

        let plain = kafka_plan(&root, Flavor::PlainMaven, "com.example.app.messaging").unwrap();
        assert_eq!(plain.deps[0].artifact_id, "kafka-clients");
        assert_eq!(plain.deps[0].version, Some("4.1.0"));
        assert_eq!(plain.compose, vec![compose::KAFKA]);
        // Plain Maven has no Spring properties file to write into.
        assert!(plain.properties.is_empty());

        let _ = fs::remove_dir_all(&root);
    }
}
