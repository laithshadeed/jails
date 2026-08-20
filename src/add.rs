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
    fn label(self) -> &'static str {
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
        write_new_file(&file.path, &file.contents)?;
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

// ---------------------------------------------------------------------------
// db -- PostgreSQL, Flyway, and real integration tests; deliberately no ORM
// ---------------------------------------------------------------------------

const SPRING_JDBC: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-jdbc",
    version: None,
    scope: None,
    optional: false,
};
const POSTGRES_MANAGED: Dependency = Dependency {
    group_id: "org.postgresql",
    artifact_id: "postgresql",
    version: None,
    scope: Some("runtime"),
    optional: false,
};
const POSTGRES_PINNED: Dependency = Dependency {
    group_id: "org.postgresql",
    artifact_id: "postgresql",
    version: Some("42.7.11"),
    scope: Some("runtime"),
    optional: false,
};
/// Flyway's Boot integration, which is a *different artifact* from Flyway.
///
/// Boot 4 split auto-configuration into ~130 modules, and there is no Flyway
/// class in `spring-boot-autoconfigure` at all. With only `flyway-core` on the
/// classpath the migrations are never run and nothing says so: no error, no
/// warning, not one Flyway log line -- and then `relation "..." does not
/// exist` from the first query, which reads like a broken migration rather
/// than an absent one.
///
/// The general rule this is one instance of: in Boot 4 the technology jar and
/// the auto-configuration jar are separate dependencies, and a capability that
/// ships only the former ships something that does not run.
const SPRING_BOOT_FLYWAY: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-flyway",
    version: None,
    scope: None,
    optional: false,
};
const FLYWAY_CORE_MANAGED: Dependency = Dependency {
    group_id: "org.flywaydb",
    artifact_id: "flyway-core",
    version: None,
    scope: None,
    optional: false,
};
const FLYWAY_POSTGRES_MANAGED: Dependency = Dependency {
    group_id: "org.flywaydb",
    artifact_id: "flyway-database-postgresql",
    version: None,
    scope: None,
    optional: false,
};
const FLYWAY_CORE_PINNED: Dependency = Dependency {
    group_id: "org.flywaydb",
    artifact_id: "flyway-core",
    version: Some("12.8.1"),
    scope: None,
    optional: false,
};
const FLYWAY_POSTGRES_PINNED: Dependency = Dependency {
    group_id: "org.flywaydb",
    artifact_id: "flyway-database-postgresql",
    version: Some("12.8.1"),
    scope: None,
    optional: false,
};
const TESTCONTAINERS_POSTGRES: Dependency = Dependency {
    group_id: "org.testcontainers",
    artifact_id: "testcontainers-postgresql",
    version: Some("2.0.5"),
    scope: Some("test"),
    optional: false,
};
const TESTCONTAINERS_JUNIT: Dependency = Dependency {
    group_id: "org.testcontainers",
    artifact_id: "testcontainers-junit-jupiter",
    version: Some("2.0.5"),
    scope: Some("test"),
    optional: false,
};
const SPRING_TESTCONTAINERS: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-testcontainers",
    version: None,
    scope: Some("test"),
    optional: false,
};

const POSTGRES_IMAGE: &str = "postgres:17-alpine";
const TESTCONTAINERS_CONFIG: &str = "TestcontainersConfig";

fn db_plan(root: &std::path::Path, flavor: Flavor, pkg: &str) -> Result<Plan> {
    let mut deps = match flavor {
        Flavor::SpringBoot => vec![
            SPRING_JDBC,
            POSTGRES_MANAGED,
            SPRING_BOOT_FLYWAY,
            FLYWAY_CORE_MANAGED,
            FLYWAY_POSTGRES_MANAGED,
        ],
        Flavor::PlainMaven => vec![POSTGRES_PINNED, FLYWAY_CORE_PINNED, FLYWAY_POSTGRES_PINNED],
    };
    deps.extend([TESTCONTAINERS_POSTGRES, TESTCONTAINERS_JUNIT]);
    if flavor == Flavor::SpringBoot {
        // `@ServiceConnection` and the lifecycle initializer that starts a
        // container declared as a bean both live in this module.
        deps.push(SPRING_TESTCONTAINERS);
    }

    let mut files = vec![NewFile {
        path: root.join("src/main/resources/db/migration/.gitkeep"),
        contents: String::new(),
    }];
    let spring_test_import = if flavor == Flavor::SpringBoot {
        files.push(NewFile {
            path: test_dir(root, pkg).join(format!("{TESTCONTAINERS_CONFIG}.java")),
            contents: testcontainers_config_java(pkg),
        });
        Some(SpringTestImport {
            pkg: pkg.to_string(),
            class: TESTCONTAINERS_CONFIG,
        })
    } else {
        None
    };

    Ok(Plan {
        deps,
        files,
        compose: vec![compose::POSTGRES],
        spring_test_import,
        ..Plan::default()
    })
}

/// The test-side database wiring: a container declared as a Spring bean.
///
/// `@ServiceConnection` is how a container's url, username and password reach
/// auto-configuration, and a container that is a `@Bean` is started and
/// stopped with the context -- `spring-boot-testcontainers` contributes
/// `TestcontainersLifecycleApplicationContextInitializer` from its own
/// `spring.factories`, so nothing here calls `start()`. Boot's reference docs
/// prefer this over a `@Testcontainers`/`@Container` static field, because
/// Spring caches a context beyond the container's JUnit-managed lifetime and
/// later tests then fail against a stopped container.
///
/// ## Why this is imported rather than registered globally
///
/// jails used to register this from a test-classpath `spring.factories`, so
/// that every `@SpringBootTest` got a DataSource without an annotation. That
/// solved a real problem -- once `spring-boot-starter-jdbc` is present, JDBC
/// auto-config demands a DataSource for *every* context, including a test
/// that never queries -- and created a worse one: **every** test paid for a
/// PostgreSQL container, including pure slices and `@WebMvcTest`s that have
/// no business touching a database. A test suite that starts a database it
/// does not use is slow in a way that is nobody's fault and never fixed.
///
/// So the container is imported by the tests that need it, and `add db`
/// splices that `@Import` into the `@SpringBootTest` classes already in the
/// project (see [`import_into_spring_boot_tests`]) -- which is what keeps the
/// original problem from coming back as a mysterious "Failed to determine a
/// suitable driver class" on a test the user did not write.
fn testcontainers_config_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.context.annotation.Bean;
import org.testcontainers.postgresql.PostgreSQLContainer;

/**
 * A real PostgreSQL for the tests that need one.
 *
 * <p>Import it on a test class that talks to the database:
 *
 * <pre>{{@code
 * @SpringBootTest
 * @Import(TestcontainersConfig.class)
 * class RewardIngestionIT {{ ... }}
 * }}</pre>
 *
 * <p>{{@code @ServiceConnection}} publishes the container's JDBC url, username
 * and password to auto-configuration. Connection details take precedence over
 * {{@code spring.datasource.*}}, so the application's own settings do not need
 * to be overridden for tests.
 *
 * <p>Nothing calls {{@code start()}} -- a container that is a bean is started
 * and stopped with the application context.
 */
@TestConfiguration(proxyBeanMethods = false)
public class {TESTCONTAINERS_CONFIG} {{

    @Bean
    @ServiceConnection
    PostgreSQLContainer postgresContainer() {{
        return new PostgreSQLContainer("{POSTGRES_IMAGE}");
    }}
}}
"#
    )
}

/// Whether a previously generated container config should be rewritten.
///
/// Three generations exist now. The first was a `@TestConfiguration` that
/// needed an `@Import`; the second an `ApplicationContextInitializer` that
/// injected `spring.datasource.*` by hand; the third the initializer holding a
/// nested `@ServiceConnection` bean. The current shape is back to an imported
/// `@TestConfiguration`, on purpose -- see [`testcontainers_config_java`] --
/// so the marker to look for is the *absence* of the initializer plus the
/// presence of `@ServiceConnection`.
fn should_replace_postgres_test_config(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    if name != "PostgresContainerConfig.java" && name != "TestcontainersConfig.java" {
        return false;
    }
    fs::read_to_string(path).is_ok_and(|s| {
        !s.contains("ServiceConnection") || s.contains("ApplicationContextInitializer")
    })
}

fn spring_factories_path(root: &Path) -> PathBuf {
    root.join("src/test/resources/META-INF/spring.factories")
}

fn application_properties_path(root: &Path) -> PathBuf {
    root.join("src/main/resources/application.properties")
}

/// JDBC auto-config registers a CGLIB proxy around every `@Repository`.
/// jails (and the code it generates) uses `final` classes, so that proxy
/// cannot be created. Exception translation is a JPA concern anyway -- this
/// capability is raw SQL.
const EXCEPTION_TRANSLATION_PROPERTY: &str =
    "spring.persistence.exceptiontranslation.enabled=false";

/// jails already owns the compose lifecycle -- `jails run` and `jails start`
/// bring the services up, and `jails stop` takes them down -- so Spring's own
/// docker-compose module has no job left to do in a jails project. Leaving it
/// on is not merely redundant: it shells out to the compose provider with
/// Docker Compose v2 syntax (`--ansi never`, `config --format=json`) that
/// podman-compose rejects, and the application then dies during startup
/// before any of its own code runs. Flip this to `true` to hand compose back
/// to Spring.
const COMPOSE_DISABLED_PROPERTY: &str = "spring.docker.compose.enabled=false";
const COMPOSE_LIFECYCLE_COMMENT: &str =
    "# jails starts compose itself (jails run / jails start).";

/// The application's own datasource, pointing at the compose service `add
/// db` just wrote.
///
/// Spring Boot can discover this itself through `spring-boot-docker-compose`,
/// and where that works these properties are simply overridden by it --
/// connection details take precedence over properties. Writing them anyway
/// buys two things. The application starts on a machine whose compose
/// provider Spring cannot drive (`spring-boot-docker-compose` shells out
/// with Docker Compose v2 syntax that podman-compose rejects, and the app
/// dies during startup). And the connection is visible in the project rather
/// than materialising from a module, which is the same reason this
/// capability emits SQL you can read instead of an ORM.
fn application_properties_block(connect: &compose::PostgresConnect) -> String {
    let compose::PostgresConnect {
        host,
        port,
        user,
        password,
        database,
    } = connect;
    format!(
        "# jails:db\n\
         {EXCEPTION_TRANSLATION_PROPERTY}\n\
         spring.datasource.url=jdbc:postgresql://{host}:{port}/{database}\n\
         spring.datasource.username={user}\n\
         spring.datasource.password={password}\n\
         {COMPOSE_LIFECYCLE_COMMENT}\n\
         {COMPOSE_DISABLED_PROPERTY}\n\
         # /jails:db\n"
    )
}

/// Splice a capability's own `application.properties` lines into a marked
/// block. Generic in the label so every capability owns exactly its own
/// lines and `remove` can take them back without touching a neighbour's --
/// the same rule `compose.yaml` already follows for services.
fn install_capability_properties(
    root: &Path,
    label: &str,
    lines: &[String],
    dry_run: bool,
) -> Result<bool> {
    if lines.is_empty() {
        return Ok(false);
    }
    let path = application_properties_path(root);
    let existing = if path.exists() {
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?
    } else {
        String::new()
    };
    let marker = format!("# jails:{label}");
    if existing.contains(&marker) {
        println!("  exists  {}", rel(root, &path));
        return Ok(false);
    }
    let block = format!("{marker}\n{}\n# /jails:{label}\n", lines.join("\n"));
    let next = if existing.trim().is_empty() {
        block
    } else {
        format!("{}\n{block}", existing.trim_end())
    };
    if dry_run {
        for line in lines {
            println!("  would set  {line} in {}", rel(root, &path));
        }
        return Ok(true);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    fs::write(&path, next).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    for line in lines {
        println!("  set     {line}");
    }
    Ok(true)
}

/// Remove one capability's marked property block, leaving every other line
/// -- including another capability's block -- exactly as it was.
/// Lines inside a `# jails:<label>` block that jails did not write.
///
/// The marked block is how `remove` knows what to take back out, and it is
/// also, inevitably, where people tune the capability -- it is the block with
/// the capability's name on it. A real project ended up with twenty
/// hand-written Kafka properties inside jails' markers (an
/// `ErrorHandlingDeserializer`, `acks=all`, a KIP-848 opt-in), every one of
/// which `remove kafka` would have deleted without a word.
///
/// jails cannot refuse to remove them -- they are inside the block it owns --
/// but it must not delete them silently. Naming them at the confirmation
/// prompt turns an invisible loss into a decision.
///
/// Comments and blank lines are ignored: a comment inside the block is
/// usually jails' own explanation of the property below it.
fn unowned_properties(existing: &str, label: &str, owned: &[String]) -> Vec<String> {
    let open = format!("# jails:{label}");
    let close = format!("# /jails:{label}");
    let mut found = Vec::new();
    let mut inside = false;
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed == open {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if trimmed == close {
            break;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if !owned.iter().any(|p| p.trim() == trimmed) {
            found.push(trimmed.to_string());
        }
    }
    found
}

/// Warn about hand-written properties inside the block about to be deleted.
fn report_unowned_properties(root: &Path, label: &str, owned: &[String]) {
    let Ok(existing) = fs::read_to_string(application_properties_path(root)) else {
        return;
    };
    let unowned = unowned_properties(&existing, label, owned);
    if unowned.is_empty() {
        return;
    }
    println!(
        "  !! {} propert{} inside the # jails:{label} block were not written by jails",
        unowned.len(),
        if unowned.len() == 1 { "y" } else { "ies" }
    );
    for line in &unowned {
        println!("     {line}");
    }
    println!("     these will be deleted with the block -- copy them out first if you need them");
}

fn remove_capability_properties(root: &Path, label: &str) -> Result<()> {
    let path = application_properties_path(root);
    let Ok(existing) = fs::read_to_string(&path) else {
        return Ok(());
    };
    let open = format!("# jails:{label}");
    let close = format!("# /jails:{label}");
    if !existing.contains(&open) {
        return Ok(());
    }
    let mut out = String::new();
    let mut skipping = false;
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed == open {
            skipping = true;
            continue;
        }
        if skipping {
            if trimmed == close {
                skipping = false;
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if out.trim().is_empty() {
        // The file existed only for this block; leaving an empty file behind
        // is litter.
        let _ = fs::remove_file(&path);
        println!("  removed {}", rel(root, &path));
        return Ok(());
    }
    fs::write(&path, out).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    println!("  updated {}", rel(root, &path));
    Ok(())
}

fn install_db_properties(root: &Path, dry_run: bool) -> Result<bool> {
    let path = application_properties_path(root);
    let existing = if path.exists() {
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?
    } else {
        String::new()
    };
    // An older jails wrote a block with only the exception-translation
    // property in it. `add` promises to write whatever is missing, so an
    // out-of-date block is replaced rather than reported as already present
    // -- otherwise a project generated last week silently never gains the
    // datasource it now needs.
    let has_block = existing.contains(EXCEPTION_TRANSLATION_PROPERTY);
    let current = existing.contains("spring.datasource.url=");
    if has_block && current {
        println!("  exists  {}", rel(root, &path));
        return Ok(false);
    }
    let existing = if has_block {
        remove_jails_db_block(&existing, EXCEPTION_TRANSLATION_PROPERTY).unwrap_or(existing)
    } else {
        existing
    };
    // Read back from compose.yaml rather than assuming the defaults: `add
    // db` writes that file, but a project may have edited the port or the
    // credentials since, and a datasource pointing at the wrong one is worse
    // than none.
    let connect = compose::read(root)
        .ok()
        .and_then(|yaml| compose::postgres_connect(&yaml))
        .unwrap_or_else(compose::PostgresConnect::defaults);
    let next = if existing.trim().is_empty() {
        application_properties_block(&connect)
    } else {
        format!(
            "{}\n{}",
            existing.trim_end(),
            application_properties_block(&connect)
        )
    };
    if dry_run {
        println!(
            "  would set  {EXCEPTION_TRANSLATION_PROPERTY} in {}",
            rel(root, &path)
        );
        return Ok(true);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    fs::write(&path, next).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    println!("  properties  {}", rel(root, &path));
    Ok(true)
}

fn uninstall_db_properties(root: &Path) -> Result<()> {
    let path = application_properties_path(root);
    if !path.exists() {
        return Ok(());
    }
    let existing =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let Some(next) = remove_jails_db_block(&existing, EXCEPTION_TRANSLATION_PROPERTY) else {
        return Ok(());
    };
    if next.trim().is_empty() {
        fs::remove_file(&path).map_err(|e| format!("failed to remove {}: {e}", path.display()))?;
        println!("  delete  {}", rel(root, &path));
    } else {
        fs::write(&path, next).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
        println!("  unsplice  {}", rel(root, &path));
    }
    Ok(())
}

/// Drop the matching class/resource under `target/` so incremental
/// `mvn test` (what `jails test` runs) does not keep using a deleted file.
fn delete_maven_output(root: &Path, src: &Path) {
    let Some(out) = maven_output_for(root, src) else {
        return;
    };
    if out.exists() {
        let _ = fs::remove_file(&out);
    }
}

fn maven_output_for(root: &Path, src: &Path) -> Option<PathBuf> {
    let rel = src.strip_prefix(root).ok()?;
    let mut parts = rel.iter();
    if parts.next()?.to_str()? != "src" {
        return None;
    }
    let scope = parts.next()?.to_str()?;
    let kind = parts.next()?.to_str()?;
    let rest: PathBuf = parts.collect();
    let target_root = match (scope, kind) {
        ("main", "java") | ("main", "resources") => root.join("target/classes"),
        ("test", "java") | ("test", "resources") => root.join("target/test-classes"),
        _ => return None,
    };
    let mut out = target_root.join(rest);
    if out.extension().is_some_and(|e| e == "java") {
        out.set_extension("class");
    }
    Some(out)
}

const SPRING_FACTORIES_KEY: &str = "org.springframework.context.ApplicationContextInitializer";

#[cfg(test)]
fn spring_factories_block(fqcn: &str) -> String {
    format!("# jails:db\n{SPRING_FACTORIES_KEY}={fqcn}\n# /jails:db\n")
}

/// Import the container config into every `@SpringBootTest` in the project.
///
/// This is an edit to a file the user owns, which jails does sparingly and
/// only surgically: one annotation line above an anchor that is already
/// there, and the import statement it needs. It is idempotent -- a class that
/// already has the annotation is skipped, not duplicated.
///
/// Why `add db` does this at all rather than leaving it to the reader: the
/// moment `spring-boot-starter-jdbc` lands in the pom, JDBC auto-config
/// demands a DataSource for *every* `@SpringBootTest`, including the
/// `contextLoads` test that came with the project and never touches a
/// database. Adding the capability and walking away would break a test the
/// user did not write, with a message ("Failed to determine a suitable driver
/// class") that names neither the cause nor the fix.
///
/// Returns whether anything changed.
fn install_test_container_import(
    root: &Path,
    cfg: &SpringTestImport,
    dry_run: bool,
) -> Result<bool> {
    let annotation = import_annotation(cfg.class);
    let mut changed = false;
    for path in find_spring_boot_tests(&root.join("src/test/java")) {
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        if source.contains(&annotation) {
            println!("  exists  {} in {}", cfg.class, rel(root, &path));
            continue;
        }
        let tests_pkg = package_of(&source).unwrap_or_else(|| cfg.pkg.clone());
        let extra = import_of(&tests_pkg, &cfg.pkg, cfg.class);
        let Some(next) = splice_spring_boot_test_import(&source, cfg.class, &extra) else {
            continue;
        };
        if dry_run {
            println!("  would import  {} into {}", cfg.class, rel(root, &path));
            changed = true;
            continue;
        }
        fs::write(&path, next).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
        println!("  import  {} -> {}", cfg.class, rel(root, &path));
        changed = true;
    }
    Ok(changed)
}

/// Remove the test-classpath `spring.factories` an earlier jails wrote.
///
/// Left in place it would register the old global initializer *as well as* the
/// new `@Import`, so every test would still start a container and the change
/// would look like it had not worked.
fn remove_legacy_spring_factories(root: &Path) -> Result<bool> {
    let path = spring_factories_path(root);
    if !path.exists() {
        return Ok(false);
    }
    let existing =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    if !existing.contains(SPRING_FACTORIES_KEY) {
        return Ok(false);
    }
    let Some(next) = remove_jails_db_block(&existing, SPRING_FACTORIES_KEY) else {
        return Ok(false);
    };
    if next.trim().is_empty() {
        fs::remove_file(&path).map_err(|e| format!("failed to remove {}: {e}", path.display()))?;
        println!("  delete  {} (superseded by @Import)", rel(root, &path));
    } else {
        fs::write(&path, next).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
        println!("  unsplice  {}", rel(root, &path));
    }
    Ok(true)
}

fn uninstall_postgres_test_initializer(root: &Path, cfg: &SpringTestImport) -> Result<()> {
    let path = spring_factories_path(root);
    if !path.exists() {
        return Ok(());
    }
    let existing =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let fqcn = cfg.fqcn();
    let Some(next) = remove_jails_db_block(&existing, &fqcn) else {
        return Ok(());
    };
    if next.trim().is_empty() {
        fs::remove_file(&path).map_err(|e| format!("failed to remove {}: {e}", path.display()))?;
        println!("  delete  {}", rel(root, &path));
    } else {
        fs::write(&path, next).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
        println!("  unsplice  {}", rel(root, &path));
    }
    Ok(())
}

fn remove_jails_db_block(source: &str, fqcn: &str) -> Option<String> {
    if !source.contains(fqcn) && !source.contains("# jails:db") {
        return None;
    }
    let mut out = String::new();
    let mut skipping = false;
    let mut changed = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed == "# jails:db" {
            skipping = true;
            changed = true;
            continue;
        }
        if skipping {
            if trimmed == "# /jails:db" {
                skipping = false;
            }
            continue;
        }
        if trimmed.contains(fqcn) {
            changed = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    changed.then_some(out)
}

/// Drop `@Import(PostgresContainerConfig)` left by earlier jails versions.
fn strip_legacy_postgres_imports(root: &Path, cfg: &SpringTestImport) -> Result<bool> {
    let mut changed = false;
    for path in find_spring_boot_tests(&root.join("src/test/java")) {
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        if !source.contains(&import_annotation(cfg.class)) {
            continue;
        }
        let tests_pkg = package_of(&source).unwrap_or_else(|| cfg.pkg.clone());
        let extra = import_of(&tests_pkg, &cfg.pkg, cfg.class);
        let Some(next) = unsplice_spring_boot_test_import(&source, cfg.class, &extra) else {
            continue;
        };
        fs::write(&path, next).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
        println!("  unsplice  {} from {}", cfg.class, rel(root, &path));
        changed = true;
    }
    Ok(changed)
}

fn import_annotation(class: &str) -> String {
    format!("@Import({class}.class)")
}

fn find_spring_boot_tests(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "java")
                && fs::read_to_string(&path).is_ok_and(|s| s.contains("@SpringBootTest"))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Insert `@Import(Class.class)` immediately above `@SpringBootTest` and add
/// the annotation import (plus `extra` when the config lives in another
/// package). `None` when the anchor is missing.
fn splice_spring_boot_test_import(source: &str, class: &str, extra: &str) -> Option<String> {
    let annotation = import_annotation(class);
    let anchor = source.find("@SpringBootTest")?;
    let line_start = source[..anchor].rfind('\n').map(|i| i + 1).unwrap_or(0);

    let mut out = String::with_capacity(source.len() + annotation.len() + extra.len() + 64);
    out.push_str(&source[..line_start]);
    out.push_str(&annotation);
    out.push('\n');
    out.push_str(&source[line_start..]);

    let mut imports = String::new();
    if !out.contains("org.springframework.context.annotation.Import") {
        imports.push_str("import org.springframework.context.annotation.Import;\n");
    }
    imports.push_str(extra);
    if !imports.is_empty() {
        let package_end = out.find(";\n").map(|i| i + 2)?;
        let mut with_import = String::with_capacity(out.len() + imports.len());
        with_import.push_str(&out[..package_end]);
        with_import.push('\n');
        with_import.push_str(&imports);
        with_import.push_str(&out[package_end..]);
        out = with_import;
    }
    Some(normalize_imports(&out))
}

fn unsplice_spring_boot_test_import(source: &str, class: &str, extra: &str) -> Option<String> {
    let annotation = import_annotation(class);
    if !source.contains(&annotation) {
        return None;
    }
    let extra = extra.trim();
    // Drop the Import import only when this was the last @Import in the file.
    let dropping_import_stmt = source.matches("@Import").count() <= 1;
    let lines: Vec<&str> = source
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed == annotation {
                return false;
            }
            if !extra.is_empty() && trimmed == extra {
                return false;
            }
            if dropping_import_stmt
                && trimmed == "import org.springframework.context.annotation.Import;"
            {
                return false;
            }
            true
        })
        .collect();
    let mut out = lines.join("\n");
    if source.ends_with('\n') {
        out.push('\n');
    }
    Some(normalize_imports(&out))
}

// ---------------------------------------------------------------------------
// kafka
// ---------------------------------------------------------------------------

const SPRING_KAFKA: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-kafka",
    version: None,
    scope: None,
    optional: false,
};
const KAFKA_CLIENTS: Dependency = Dependency {
    group_id: "org.apache.kafka",
    artifact_id: "kafka-clients",
    version: Some("4.1.0"),
    scope: None,
    optional: false,
};
/// Without this no test can touch a broker, which is why `add kafka` used to
/// produce a capability with no possible test.
const TESTCONTAINERS_KAFKA: Dependency = Dependency {
    group_id: "org.testcontainers",
    artifact_id: "testcontainers-kafka",
    version: Some("2.0.5"),
    scope: Some("test"),
    optional: false,
};
/// The `MeterRegistry` *API*, so the generated error handler can count
/// dead-lettered records.
///
/// Needed explicitly, which is not obvious: `spring-kafka` declares
/// micrometer-core as `optionalApi` and `spring-boot-kafka` declares it
/// `optional`, and neither kind is inherited by a downstream consumer. Without
/// this line `KafkaConfig` does not compile.
///
/// The API only. No registry *bean* is auto-configured without Actuator --
/// `MetricsAutoConfiguration` and `CompositeMeterRegistryAutoConfiguration` are
/// `@ConditionalOnClass` inside a module that only
/// `spring-boot-starter-actuator` puts on the classpath. That is why the
/// generated bean takes an `ObjectProvider<MeterRegistry>` rather than a
/// `MeterRegistry`: asking for a broker should not drag in Actuator and its
/// endpoints. `jails add observability` is what supplies the registry.
const MICROMETER_CORE: Dependency = Dependency {
    group_id: "io.micrometer",
    artifact_id: "micrometer-core",
    version: None,
    scope: None,
    optional: false,
};
/// Consuming is asynchronous, so every meaningful Kafka test waits for
/// something. Without a waiting primitive the generated test is a `Thread.sleep`
/// that is either flaky or slow.
const AWAITILITY: Dependency = Dependency {
    group_id: "org.awaitility",
    artifact_id: "awaitility",
    version: None,
    scope: Some("test"),
    optional: false,
};

fn kafka_plan(root: &Path, flavor: Flavor, pkg: &str) -> Result<Plan> {
    // Spring projects also get the properties that make publish-and-consume
    // work at all. Without them the broker is up, the code compiles, and
    // nothing is ever received -- see `spring::kafka_properties` for why each
    // one is there.
    let properties = match flavor {
        Flavor::SpringBoot => {
            let base = base_package(root)?;
            // The artifactId, not the directory name: a consumer group is a
            // shared, durable identity in the broker, and naming it after
            // whatever the checkout happens to be called gives two clones of
            // the same service two different groups -- so both receive every
            // message instead of splitting the work.
            let group = pom::read(root)
                .ok()
                .and_then(|pom| crate::project::artifact_id(&pom))
                .unwrap_or_else(|| "app".to_string());
            crate::spring::kafka_properties(&base, &group)
        }
        Flavor::PlainMaven => Vec::new(),
    };
    // The poison-message path is Spring-only: it is Spring Kafka's
    // `DefaultErrorHandler` that routes a bad record, and a plain
    // `kafka-clients` consumer has no equivalent to generate.
    let (deps, files) = match flavor {
        Flavor::SpringBoot => (
            vec![
                SPRING_KAFKA,
                MICROMETER_CORE,
                SPRING_TESTCONTAINERS,
                TESTCONTAINERS_KAFKA,
                TESTCONTAINERS_JUNIT,
                AWAITILITY,
            ],
            crate::spring::kafka_files(root, pkg)
                .into_iter()
                .map(|(path, contents, _)| NewFile { path, contents })
                .collect(),
        ),
        Flavor::PlainMaven => (vec![KAFKA_CLIENTS], Vec::new()),
    };

    Ok(Plan {
        deps,
        files,
        compose: vec![compose::KAFKA],
        properties,
        ..Plan::default()
    })
}

// ---------------------------------------------------------------------------
// csv
// ---------------------------------------------------------------------------

/// Commons CSV renamed `Builder.build()` to `Builder.get()` in 1.13, so the
/// pinned version and the generated call have to move together.
const COMMONS_CSV: Dependency = Dependency {
    group_id: "org.apache.commons",
    artifact_id: "commons-csv",
    version: Some("1.14.1"),
    scope: None,
    optional: false,
};

fn csv_plan(
    root: &std::path::Path,
    pkg: &str,
    _flavor: Flavor,
    name: Option<&str>,
) -> Result<Plan> {
    let base = capitalize(name.unwrap_or("Csv"));
    let class = format!("{base}Reader");

    Ok(Plan {
        // Spring Boot's dependency management does not cover commons-csv, so
        // the version is pinned in both flavors.
        deps: vec![COMMONS_CSV],
        files: vec![
            NewFile {
                path: main_dir(root, pkg).join(format!("{class}.java")),
                contents: csv_reader_java(pkg, &class),
            },
            NewFile {
                path: test_dir(root, pkg).join(format!("{class}Test.java")),
                contents: csv_reader_test_java(pkg, &class),
            },
        ],
        ..Plan::default()
    })
}

fn csv_reader_java(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;
import org.apache.commons.csv.CSVFormat;

/**
 * Reads a CSV file with a header row into {{@link Row}} values.
 *
 * <p>Parsing is delegated to Commons CSV so quoted fields, embedded commas
 * and embedded newlines are handled correctly.
 */
public final class {class} {{

    private {class}() {{}}

    /** One CSV record: column name to value. */
    public record Row(Map<String, String> values) {{

        public Row {{
            values = Map.copyOf(values);
        }}

        /** Value of {{@code column}}, or a clear failure if it is not in the header. */
        public String get(String column) {{
            var value = values.get(column);
            if (value == null) {{
                throw new IllegalArgumentException("no column named '" + column + "' in " + values.keySet());
            }}
            return value;
        }}

        public int getInt(String column) {{
            return Integer.parseInt(get(column));
        }}
    }}

    /** Reads every row of {{@code path}}, treating the first line as the header. */
    public static List<Row> read(Path path) throws IOException {{
        var format = CSVFormat.DEFAULT.builder()
                .setHeader()
                .setSkipHeaderRecord(true)
                .setTrim(true)
                .get();
        try (var reader = Files.newBufferedReader(path);
                var parser = format.parse(reader)) {{
            return parser.stream().map(record -> new Row(record.toMap())).toList();
        }} catch (UncheckedIOException e) {{
            throw e.getCause();
        }}
    }}
}}
"#
    )
}

fn csv_reader_test_java(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class {class}Test {{

    @TempDir
    Path tmp;

    private Path csv(String contents) throws Exception {{
        var path = tmp.resolve("rows.csv");
        Files.writeString(path, contents);
        return path;
    }}

    @Test
    void readsRowsKeyedByHeader() throws Exception {{
        var rows = {class}.read(csv("name,qty\nbolt,7\n"));

        assertEquals(1, rows.size());
        assertEquals("bolt", rows.getFirst().get("name"));
        assertEquals(7, rows.getFirst().getInt("qty"));
    }}

    @Test
    void keepsCommasInsideQuotedFields() throws Exception {{
        var rows = {class}.read(csv("name,qty\n\"widget, large\",3\n"));

        assertEquals("widget, large", rows.getFirst().get("name"));
    }}

    @Test
    void readsAnEmptyFileAsNoRows() throws Exception {{
        assertEquals(List.of(), {class}.read(csv("name,qty\n")));
    }}

    @Test
    void namesTheColumnWhenItIsMissing() throws Exception {{
        var rows = {class}.read(csv("name,qty\nbolt,7\n"));

        var error = assertThrows(IllegalArgumentException.class, () -> rows.getFirst().get("price"));
        assertEquals(true, error.getMessage().contains("price"));
    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// sqlite
// ---------------------------------------------------------------------------

const SQLITE_JDBC: Dependency = Dependency {
    group_id: "org.xerial",
    artifact_id: "sqlite-jdbc",
    version: Some("3.49.1.0"),
    scope: None,
    optional: false,
};

/// Deliberately the same code in both flavors. `java.sql` is part of the
/// standard library, so a plain JDBC connection plus a migration runner needs
/// nothing beyond the driver or the fiddliness of a persistence framework.
/// A Spring project can inject the record wherever it needs a connection.
fn sqlite_plan(
    root: &std::path::Path,
    pkg: &str,
    _flavor: Flavor,
    name: Option<&str>,
) -> Result<Plan> {
    let base = name.map(capitalize).unwrap_or_default();
    let database = format!("{base}Database");
    let migrations = format!("{base}Migrations");

    Ok(Plan {
        deps: vec![SQLITE_JDBC],
        files: vec![
            NewFile {
                path: main_dir(root, pkg).join(format!("{database}.java")),
                contents: database_java(pkg, &database),
            },
            NewFile {
                path: main_dir(root, pkg).join(format!("{migrations}.java")),
                contents: migrations_java(pkg, &migrations),
            },
            NewFile {
                path: root.join("src/main/resources/db/migration/001_init.sql"),
                contents: FIRST_MIGRATION.to_string(),
            },
            NewFile {
                path: test_dir(root, pkg).join(format!("{database}Test.java")),
                contents: database_test_java(pkg, &database, &migrations),
            },
        ],
        ..Plan::default()
    })
}

const FIRST_MIGRATION: &str = "-- Applied once, in filename order, by Migrations.applyAll.
create table if not exists item (
    id integer primary key autoincrement,
    name text not null,
    qty integer not null default 0
);
";

fn database_java(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import java.nio.file.Path;
import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.SQLException;

/**
 * A SQLite database file. Connections come from {{@code java.sql}} -- the only
 * thing the driver dependency adds is the {{@code jdbc:sqlite:}} URL scheme.
 *
 * <p>Callers own the {{@link Connection}} and should use try-with-resources.
 */
public record {class}(Path file) {{

    /**
     * A database that lives only for as long as the connection does. Each
     * {{@link #open()}} returns a *fresh, empty* in-memory database, which is
     * what makes it convenient for isolated tests.
     */
    public static {class} inMemory() {{
        return new {class}(Path.of(":memory:"));
    }}

    public Connection open() throws SQLException {{
        return DriverManager.getConnection("jdbc:sqlite:" + file);
    }}
}}
"#
    )
}

fn migrations_java(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.sql.Connection;
import java.sql.SQLException;
import java.util.ArrayList;
import java.util.List;

/**
 * Applies {{@code .sql}} files in filename order, exactly once each.
 *
 * <p>Applied scripts are recorded in a {{@code schema_migrations}} table, so
 * running this on every startup is safe: only new files do any work.
 */
public final class {class} {{

    private static final String CREATE_TRACKING_TABLE =
            """
            create table if not exists schema_migrations (
                name text primary key,
                applied_at text not null default (datetime('now'))
            )
            """;

    private {class}() {{}}

    /**
     * Applies every not-yet-applied script in {{@code dir}}, returning the names
     * of the ones applied. A missing directory means no migrations, not an
     * error.
     */
    public static List<String> applyAll(Connection connection, Path dir) throws IOException, SQLException {{
        try (var statement = connection.createStatement()) {{
            statement.execute(CREATE_TRACKING_TABLE);
        }}

        var applied = new ArrayList<String>();
        for (var script : scripts(dir)) {{
            var name = script.getFileName().toString();
            if (!alreadyApplied(connection, name)) {{
                apply(connection, name, Files.readString(script));
                applied.add(name);
            }}
        }}
        return List.copyOf(applied);
    }}

    private static List<Path> scripts(Path dir) throws IOException {{
        if (!Files.isDirectory(dir)) {{
            return List.of();
        }}
        try (var files = Files.list(dir)) {{
            return files.filter(path -> path.getFileName().toString().endsWith(".sql")).sorted().toList();
        }}
    }}

    private static boolean alreadyApplied(Connection connection, String name) throws SQLException {{
        try (var query = connection.prepareStatement("select 1 from schema_migrations where name = ?")) {{
            query.setString(1, name);
            try (var rows = query.executeQuery()) {{
                return rows.next();
            }}
        }}
    }}

    /** Each script runs in one transaction, together with recording its name. */
    private static void apply(Connection connection, String name, String sql) throws SQLException {{
        var autoCommit = connection.getAutoCommit();
        connection.setAutoCommit(false);
        try {{
            try (var statement = connection.createStatement()) {{
                // Simple splitter: fine for schema DDL, but it would break on a
                // semicolon inside a string literal or a trigger body.
                for (var command : sql.split(";")) {{
                    if (!command.isBlank()) {{
                        statement.execute(command);
                    }}
                }}
            }}
            try (var insert = connection.prepareStatement("insert into schema_migrations(name) values (?)")) {{
                insert.setString(1, name);
                insert.executeUpdate();
            }}
            connection.commit();
        }} catch (SQLException e) {{
            connection.rollback();
            throw e;
        }} finally {{
            connection.setAutoCommit(autoCommit);
        }}
    }}
}}
"#
    )
}

fn database_test_java(pkg: &str, database: &str, migrations: &str) -> String {
    format!(
        r#"package {pkg};

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class {database}Test {{

    @TempDir
    Path tmp;

    private Path migrationDir() throws Exception {{
        var dir = tmp.resolve("migration");
        Files.createDirectories(dir);
        Files.writeString(dir.resolve("001_init.sql"), "create table item (id integer primary key, name text not null);");
        return dir;
    }}

    @Test
    void appliesEachMigrationExactlyOnce() throws Exception {{
        var database = new {database}(tmp.resolve("test.db"));
        var dir = migrationDir();

        try (var connection = database.open()) {{
            assertEquals(List.of("001_init.sql"), {migrations}.applyAll(connection, dir));
            assertEquals(List.of(), {migrations}.applyAll(connection, dir), "second run should be a no-op");
        }}
    }}

    @Test
    void storesAndReadsRows() throws Exception {{
        var database = new {database}(tmp.resolve("test.db"));
        var dir = migrationDir();

        try (var connection = database.open()) {{
            {migrations}.applyAll(connection, dir);

            try (var insert = connection.prepareStatement("insert into item(name) values (?)")) {{
                insert.setString(1, "bolt");
                insert.executeUpdate();
            }}
            try (var query = connection.prepareStatement("select name from item");
                    var rows = query.executeQuery()) {{
                assertTrue(rows.next());
                assertEquals("bolt", rows.getString("name"));
            }}
        }}
    }}

    @Test
    void treatsAMissingMigrationDirectoryAsNoMigrations() throws Exception {{
        try (var connection = {database}.inMemory().open()) {{
            assertEquals(List.of(), {migrations}.applyAll(connection, tmp.resolve("nope")));
        }}
    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// json
// ---------------------------------------------------------------------------

/// Jackson **3**, whose coordinates changed with the major version:
/// `tools.jackson.core`, not `com.fasterxml.jackson.core`.
///
/// This matters more than a version bump usually does. Spring Boot 4's web
/// starter already brings Jackson 3 in, so adding the 2.x artifact put *two
/// Jackson majors on one classpath* and generated a utility written against
/// the deprecated one. They do not conflict at the class level -- the
/// packages differ -- which is exactly why nothing complains and the wrong
/// mapper is used forever.
const JACKSON_VERSION: &str = "3.0.1";

const JACKSON: Dependency = Dependency {
    group_id: "tools.jackson.core",
    artifact_id: "jackson-databind",
    version: Some(JACKSON_VERSION),
    scope: None,
    optional: false,
};

/// Jackson 3 needs **no** `jackson-datatype-jsr310`: java.time support moved
/// into the core databind module, so the 2.x migration *deletes* a dependency
/// rather than adding one.
///
/// Kept as a constant so `remove json` can still unsplice it from a project
/// that jails wrote before the move.
const JACKSON_JSR310: Dependency = Dependency {
    group_id: "com.fasterxml.jackson.datatype",
    artifact_id: "jackson-datatype-jsr310",
    version: Some("2.19.0"),
    scope: None,
    optional: false,
};

fn json_plan(
    root: &std::path::Path,
    pkg: &str,
    flavor: Flavor,
    name: Option<&str>,
) -> Result<Plan> {
    let base = name.map(capitalize).unwrap_or_default();
    let class = format!("{base}Json");

    // Spring Boot's dependency management already pins Jackson (and the web
    // starter pulls it in transitively), so declaring a version here would
    // fight the parent pom.
    // One artifact, not two: Jackson 3 has java.time built in. On Spring the
    // version is left to the parent, which already manages Jackson 3.
    let deps = match flavor {
        Flavor::SpringBoot => vec![Dependency {
            version: None,
            ..JACKSON
        }],
        Flavor::PlainMaven => vec![JACKSON],
    };

    Ok(Plan {
        deps,
        legacy_deps: vec![JACKSON_JSR310, Dependency { group_id: "com.fasterxml.jackson.core", ..JACKSON }],
        files: vec![
            NewFile {
                path: main_dir(root, pkg).join(format!("{class}.java")),
                contents: json_java(pkg, &class),
            },
            NewFile {
                path: test_dir(root, pkg).join(format!("{class}Test.java")),
                contents: json_test_java(pkg, &class),
            },
        ],
        ..Plan::default()
    })
}

fn json_java(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import tools.jackson.databind.JsonNode;
import tools.jackson.databind.json.JsonMapper;

/**
 * JSON reading and writing over one shared, thread-safe {{@link JsonMapper}}.
 *
 * <p>Jackson 3 (`tools.jackson`), not the 2.x `com.fasterxml.jackson` line.
 * java.time support is built in, so {{@code LocalDate}} round-trips as an ISO
 * string with no module to register, and dates are written as strings by
 * default rather than as numeric timestamps.
 *
 * <p>Records map to JSON objects without any annotations.
 *
 * <p>Two ways in, for two situations. {{@link #read}} binds the whole document
 * to a type -- right for input you control, wrong for input you do not, since
 * one bad element fails the entire parse. For untrusted input use
 * {{@link #readTree}} and {{@link #convert}} to validate element by element,
 * keeping the good records and reporting the bad ones.
 */
public final class {class} {{

    private static final JsonMapper MAPPER = JsonMapper.builder().build();

    private {class}() {{}}

    public static <T> T read(Path path, Class<T> type) throws IOException {{
        try (var in = Files.newInputStream(path)) {{
            return MAPPER.readValue(in, type);
        }}
    }}

    /**
     * Reads the whole document as a tree, without binding it to any type.
     *
     * <p>Use this when the shape cannot be trusted: walk the tree, check each
     * node with {{@code isObject()}} and friends, and {{@link #convert}} the ones
     * that look right. Nothing is lost to a single malformed element.
     */
    public static JsonNode readTree(Path path) throws IOException {{
        try (var in = Files.newInputStream(path)) {{
            return MAPPER.readTree(in);
        }}
    }}

    /** Binds one already-parsed tree node to {{@code type}}. */
    public static <T> T convert(JsonNode node, Class<T> type) {{
        return MAPPER.convertValue(node, type);
    }}

    /**
     * Reads a JSON Lines file: one JSON value per line, blank lines skipped.
     *
     * <p>The format event logs and streaming exports use, because appending a
     * line is cheap where appending to an array is not. Returned as trees
     * rather than bound values for the same reason {{@link #readTree}} exists --
     * one malformed line should not cost you the whole file.
     */
    public static List<JsonNode> readJsonl(Path path) throws IOException {{
        try (var lines = Files.lines(path)) {{
            var nodes = new ArrayList<JsonNode>();
            for (var line : lines.filter(text -> !text.isBlank()).toList()) {{
                nodes.add(MAPPER.readTree(line));
            }}
            return List.copyOf(nodes);
        }}
    }}

    /** Reads a top-level JSON array into a list of {{@code element}}. */
    public static <T> List<T> readList(Path path, Class<T> element) throws IOException {{
        var listType = MAPPER.getTypeFactory().constructCollectionType(List.class, element);
        try (var in = Files.newInputStream(path)) {{
            return MAPPER.readValue(in, listType);
        }}
    }}

    /** Writes {{@code value}} as indented JSON, replacing any existing file. */
    public static void write(Path path, Object value) throws IOException {{
        try (var out = Files.newOutputStream(path)) {{
            MAPPER.writerWithDefaultPrettyPrinter().writeValue(out, value);
        }}
    }}

    /**
     * No {{@code throws}}: {{@code JacksonException}} extends
     * {{@link RuntimeException}} in Jackson 3, where its 2.x counterpart was
     * checked.
     */
    public static String toJson(Object value) {{
        return MAPPER.writeValueAsString(value);
    }}

    public static <T> T parse(String json, Class<T> type) {{
        return MAPPER.readValue(json, type);
    }}
}}
"#
    )
}

fn json_test_java(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.file.Files;
import java.nio.file.Path;
import java.time.LocalDate;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class {class}Test {{

    /** Records need no annotations to round-trip. */
    record Item(String name, int qty) {{}}

    record Dated(String name, LocalDate on) {{}}

    @TempDir
    Path tmp;

    @Test
    void roundTripsARecordThroughAFile() throws Exception {{
        var path = tmp.resolve("item.json");
        {class}.write(path, new Item("bolt", 7));

        assertEquals(new Item("bolt", 7), {class}.read(path, Item.class));
    }}

    @Test
    void readsAJsonArrayAsAList() throws Exception {{
        var path = tmp.resolve("items.json");
        Files.writeString(path, "[{{\"name\":\"bolt\",\"qty\":7}},{{\"name\":\"nut\",\"qty\":3}}]");

        assertEquals(List.of(new Item("bolt", 7), new Item("nut", 3)), {class}.readList(path, Item.class));
    }}

    @Test
    void roundTripsThroughAString() throws Exception {{
        assertEquals(new Item("bolt", 7), {class}.parse({class}.toJson(new Item("bolt", 7)), Item.class));
    }}

    /**
     * Without the java.time module on the classpath this writes
     * {{@code {{"year":2026,...}}}} instead of an ISO string, and reading it back
     * fails outright.
     */
    @Test
    void writesDatesAsIsoStringsNotObjects() throws Exception {{
        var json = {class}.toJson(new Dated("invoice", LocalDate.of(2026, 8, 1)));

        assertTrue(json.contains("\"2026-08-01\""), "expected an ISO date in " + json);
        assertEquals(new Dated("invoice", LocalDate.of(2026, 8, 1)), {class}.parse(json, Dated.class));
    }}

    @Test
    void readsOneJsonValuePerLine() throws Exception {{
        var path = tmp.resolve("events.jsonl");
        Files.writeString(path, "{{\"id\":1}}\n\n{{\"id\":2}}\n");

        var events = {class}.readJsonl(path);

        assertEquals(2, events.size(), "blank lines should be skipped");
        assertEquals(1, events.getFirst().get("id").asInt());
        assertEquals(2, events.getLast().get("id").asInt());
    }}

    @Test
    void readsAnEmptyJsonlFileAsNoEvents() throws Exception {{
        var path = tmp.resolve("empty.jsonl");
        Files.writeString(path, "");

        assertEquals(List.of(), {class}.readJsonl(path));
    }}

    @Test
    void readsATreeWithoutBindingItToAType() throws Exception {{
        var path = tmp.resolve("tree.json");
        Files.writeString(path, "{{\"items\":[{{\"name\":\"bolt\",\"qty\":7}}]}}");

        var root = {class}.readTree(path);

        assertTrue(root.isObject());
        assertEquals("bolt", root.get("items").get(0).get("name").asText());
    }}

    /**
     * The reason the tree API exists: a document with junk mixed into an array
     * still yields every well-formed element, rather than failing as a whole.
     */
    @Test
    void keepsGoodElementsWhenSiblingsAreMalformed() throws Exception {{
        var path = tmp.resolve("mixed.json");
        Files.writeString(path, "[{{\"name\":\"bolt\",\"qty\":7}},\"not-an-object\",{{\"name\":\"nut\",\"qty\":3}}]");

        var good = new ArrayList<Item>();
        var skipped = 0;
        for (var node : {class}.readTree(path)) {{
            if (node.isObject()) {{
                good.add({class}.convert(node, Item.class));
            }} else {{
                skipped++;
            }}
        }}

        assertEquals(List.of(new Item("bolt", 7), new Item("nut", 3)), good);
        assertEquals(1, skipped);
    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// testkit
// ---------------------------------------------------------------------------

/// The four things every testable CLI needs and nobody enjoys writing twice.
/// No dependency: JUnit and AssertJ are already there, and everything here is
/// plain JDK.
///
/// These helpers also apply pressure in the right direction. `Clocks` and
/// `Ids` are only usable by code that *takes* a `Clock` and a
/// `Supplier<String>` instead of calling `Instant.now()` and
/// `UUID.randomUUID()` -- so generating them nudges the design toward the one
/// that can be tested deterministically at all.
fn testkit_plan(root: &std::path::Path, testkit: &str) -> Result<Plan> {
    let dir = test_dir(root, testkit);

    Ok(Plan {
        files: vec![
            NewFile {
                path: dir.join("Clocks.java"),
                contents: clocks_java(testkit),
            },
            NewFile {
                path: dir.join("Ids.java"),
                contents: ids_java(testkit),
            },
            NewFile {
                path: dir.join("Fixtures.java"),
                contents: fixtures_java(testkit),
            },
            NewFile {
                path: dir.join("Cli.java"),
                contents: testkit_cli_java(testkit),
            },
            NewFile {
                path: dir.join("TestkitTest.java"),
                contents: testkit_test_java(testkit),
            },
            NewFile {
                path: root.join("src/test/resources/fixtures/example.json"),
                contents: EXAMPLE_FIXTURE.to_string(),
            },
        ],
        ..Plan::default()
    })
}

const EXAMPLE_FIXTURE: &str = "{\n  \"name\": \"bolt\",\n  \"qty\": 7\n}\n";

fn clocks_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.time.ZoneId;
import java.time.ZoneOffset;

/**
 * Deterministic clocks.
 *
 * <p>These only work on code that accepts a {{@link Clock}} rather than calling
 * {{@code Instant.now()}}. That is the point: taking the clock as a parameter is
 * what makes a timestamp assertable at all.
 *
 * <p>{{@code Clock.fixed}} is already in the JDK, so only the stepping clock --
 * for asserting that events are ordered and distinct -- needs writing.
 */
public final class Clocks {{

    /** An arbitrary, memorable instant. Deterministic is the only requirement. */
    public static final Instant DEFAULT_START = Instant.parse("2026-01-01T00:00:00Z");

    private Clocks() {{}}

    public static Clock fixed(Instant instant) {{
        return Clock.fixed(instant, ZoneOffset.UTC);
    }}

    public static Clock fixed() {{
        return fixed(DEFAULT_START);
    }}

    /** A clock that advances by {{@code step}} on every read. */
    public static Clock stepping(Instant start, Duration step) {{
        return new SteppingClock(start, step, ZoneOffset.UTC);
    }}

    public static Clock stepping() {{
        return stepping(DEFAULT_START, Duration.ofSeconds(1));
    }}

    private static final class SteppingClock extends Clock {{

        private final Duration step;
        private final ZoneId zone;
        private Instant current;

        private SteppingClock(Instant start, Duration step, ZoneId zone) {{
            this.current = start;
            this.step = step;
            this.zone = zone;
        }}

        @Override
        public ZoneId getZone() {{
            return zone;
        }}

        @Override
        public Clock withZone(ZoneId other) {{
            return new SteppingClock(current, step, other);
        }}

        @Override
        public synchronized Instant instant() {{
            var value = current;
            current = current.plus(step);
            return value;
        }}
    }}
}}
"#
    )
}

fn ids_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Supplier;

/**
 * Deterministic identifiers.
 *
 * <p>The counterpart to {{@link Clocks}}: code that takes a
 * {{@code Supplier<String>}} instead of calling {{@code UUID.randomUUID()}} can
 * have its output asserted in full, identifiers included.
 */
public final class Ids {{

    private Ids() {{}}

    /** Yields {{@code prefix-1}}, {{@code prefix-2}}, ... */
    public static Supplier<String> sequential(String prefix, int start) {{
        var next = new AtomicInteger(start);
        return () -> prefix + "-" + next.getAndIncrement();
    }}

    public static Supplier<String> sequential(String prefix) {{
        return sequential(prefix, 1);
    }}
}}
"#
    )
}

fn fixtures_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import java.io.IOException;
import java.io.UncheckedIOException;
import java.net.URISyntaxException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

/**
 * Loads sample files from {{@code src/test/resources/fixtures}}.
 *
 * <p>Off the classpath, not by walking relative paths from the working
 * directory: {{@code Path.of("../fixtures")}} works until something runs the
 * suite from elsewhere, and then fails in a way that looks like a test bug.
 *
 * <p>A missing fixture fails immediately, naming what it looked for. Silently
 * returning empty input turns a typo into a passing test.
 */
public final class Fixtures {{

    private static final String ROOT = "/fixtures/";

    private Fixtures() {{}}

    /** Raw bytes of a fixture, e.g. {{@code bytes("example.json")}}. */
    public static byte[] bytes(String name) {{
        try (var in = Fixtures.class.getResourceAsStream(ROOT + name)) {{
            if (in == null) {{
                throw new IllegalArgumentException("no fixture named '" + name + "' under src/test/resources" + ROOT);
            }}
            return in.readAllBytes();
        }} catch (IOException error) {{
            throw new UncheckedIOException("unreadable fixture: " + name, error);
        }}
    }}

    public static String text(String name) {{
        return new String(bytes(name), StandardCharsets.UTF_8);
    }}

    /** Non-blank lines, for line-oriented formats like CSV and JSONL. */
    public static List<String> lines(String name) {{
        return text(name).lines().filter(line -> !line.isBlank()).toList();
    }}

    /** Real filesystem path, for code under test that insists on a {{@link Path}}. */
    public static Path path(String name) {{
        var url = Fixtures.class.getResource(ROOT + name);
        if (url == null) {{
            throw new IllegalArgumentException("no fixture named '" + name + "' under src/test/resources" + ROOT);
        }}
        try {{
            return Path.of(url.toURI());
        }} catch (URISyntaxException error) {{
            throw new IllegalStateException("fixture path is not a file: " + name, error);
        }}
    }}

    /** Copies a fixture into {{@code directory}}, for tests that mutate their input. */
    public static Path copyTo(String name, Path directory) {{
        try {{
            Files.createDirectories(directory);
            var target = directory.resolve(Path.of(name).getFileName().toString());
            Files.write(target, bytes(name));
            return target;
        }} catch (IOException error) {{
            throw new UncheckedIOException("could not copy fixture " + name, error);
        }}
    }}
}}
"#
    )
}

fn testkit_cli_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.util.List;

/**
 * Runs a command in-process and captures what a user would have seen.
 *
 * <p>No {{@code System.setOut}} anywhere: the command under test takes its
 * streams as arguments, so capturing them is just passing different ones. That
 * keeps these tests safe to run in parallel, which the swap-the-global approach
 * never is.
 *
 * <p>{{@link Command}} matches the shape {{@code jails generate command}} and
 * {{@code jails generate cli}} emit, so a real command is a method reference:
 *
 * {{@snippet :
 * var run = Cli.run(GreetCommand::run, "world");
 * assertThat(run.exitCode()).isZero();
 * assertThat(run.out()).contains("hello world");
 * }}
 */
public final class Cli {{

    /** Anything that takes streams plus argv and returns an exit code. */
    @FunctionalInterface
    public interface Command {{
        int run(PrintStream out, PrintStream err, String... args);
    }}

    /** What one invocation produced. */
    public record Run(String out, String err, int exitCode) {{

        /** Stdout split into non-blank lines, for asserting line by line. */
        public List<String> outLines() {{
            return out.lines().filter(line -> !line.isBlank()).toList();
        }}

        public boolean succeeded() {{
            return exitCode == 0;
        }}
    }}

    private Cli() {{}}

    public static Run run(Command command, String... args) {{
        var out = new ByteArrayOutputStream();
        var err = new ByteArrayOutputStream();
        int exitCode;
        try (var capturedOut = new PrintStream(out, true, StandardCharsets.UTF_8);
                var capturedErr = new PrintStream(err, true, StandardCharsets.UTF_8)) {{
            exitCode = command.run(capturedOut, capturedErr, args);
        }}
        return new Run(out.toString(StandardCharsets.UTF_8), err.toString(StandardCharsets.UTF_8), exitCode);
    }}
}}
"#
    )
}

fn testkit_test_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.time.Instant;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

/** Proves the test kit itself works, so a failure elsewhere is never its fault. */
class TestkitTest {{

    @Test
    void fixedClockDoesNotMove() {{
        var clock = Clocks.fixed();

        assertThat(clock.instant()).isEqualTo(Clocks.DEFAULT_START).isEqualTo(clock.instant());
    }}

    @Test
    void steppingClockAdvancesOnEveryRead() {{
        var clock = Clocks.stepping(Instant.parse("2026-01-01T00:00:00Z"), Duration.ofMinutes(1));

        assertThat(clock.instant()).isEqualTo(Instant.parse("2026-01-01T00:00:00Z"));
        assertThat(clock.instant()).isEqualTo(Instant.parse("2026-01-01T00:01:00Z"));
    }}

    @Test
    void idsAreSequentialAndPrefixed() {{
        var ids = Ids.sequential("txn");

        assertThat(ids.get()).isEqualTo("txn-1");
        assertThat(ids.get()).isEqualTo("txn-2");
    }}

    @Test
    void fixturesLoadOffTheClasspath() {{
        assertThat(Fixtures.text("example.json")).contains("bolt");
        assertThat(Fixtures.path("example.json")).exists();
    }}

    /** A typo in a fixture name must fail, not quietly read nothing. */
    @Test
    void aMissingFixtureNamesWhatItLookedFor() {{
        assertThatThrownBy(() -> Fixtures.text("nope.json"))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("nope.json");
    }}

    @Test
    void cliCapturesBothStreamsAndTheExitCode() {{
        var run = Cli.run(
                (out, err, args) -> {{
                    out.println("out: " + String.join(",", args));
                    err.println("err");
                    return 3;
                }},
                "a",
                "b");

        assertThat(run.out()).contains("out: a,b");
        assertThat(run.err()).contains("err");
        assertThat(run.exitCode()).isEqualTo(3);
        assertThat(run.succeeded()).isFalse();
    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// fake
// ---------------------------------------------------------------------------

/// A scripted test double. Generic by construction: jails has no Java parser
/// and no business acquiring one, so rather than generating a fake *of* some
/// interface, this generates the replay engine and you attach it to any
/// interface with a lambda. One class covers every collaborator in the project.
fn fake_plan(root: &std::path::Path, testkit: &str) -> Result<Plan> {
    let dir = test_dir(root, testkit);

    Ok(Plan {
        files: vec![
            NewFile {
                path: dir.join("Fake.java"),
                contents: scripted_java(testkit),
            },
            NewFile {
                path: dir.join("FakeTest.java"),
                contents: scripted_test_java(testkit),
            },
        ],
        ..Plan::default()
    })
}

// ---------------------------------------------------------------------------
// toxiproxy -- network failure as something a test can switch on
// ---------------------------------------------------------------------------

const TESTCONTAINERS_TOXIPROXY: Dependency = Dependency {
    group_id: "org.testcontainers",
    artifact_id: "testcontainers-toxiproxy",
    version: Some("2.0.5"),
    scope: Some("test"),
    optional: false,
};
/// The client the container speaks to. Testcontainers 2.x ships the container
/// and nothing else -- `getProxy` lived on the 1.x class and is gone -- so the
/// control API has to be driven directly.
const TOXIPROXY_JAVA: Dependency = Dependency {
    group_id: "eu.rekawek.toxiproxy",
    artifact_id: "toxiproxy-java",
    version: Some("2.1.11"),
    scope: Some("test"),
    optional: false,
};

fn toxiproxy_plan(root: &std::path::Path, testkit: &str) -> Result<Plan> {
    let dir = test_dir(root, testkit);

    Ok(Plan {
        // Deliberately not TESTCONTAINERS_JUNIT: the generated test drives the
        // container itself, and claiming a dependency another capability also
        // owns means `remove toxiproxy` takes it away from `db` too.
        deps: vec![TESTCONTAINERS_TOXIPROXY, TOXIPROXY_JAVA],
        files: vec![
            NewFile {
                path: dir.join("Faults.java"),
                contents: faults_java(testkit),
            },
            NewFile {
                path: dir.join("FaultsTest.java"),
                contents: faults_test_java(testkit),
            },
        ],
        ..Plan::default()
    })
}

fn faults_java(pkg: &str) -> String {
    format!(
        r##"package {pkg};

import eu.rekawek.toxiproxy.Proxy;
import eu.rekawek.toxiproxy.ToxiproxyClient;
import eu.rekawek.toxiproxy.model.ToxicDirection;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.time.Duration;
import java.util.concurrent.atomic.AtomicInteger;
import org.testcontainers.Testcontainers;
import org.testcontainers.containers.Network;
import org.testcontainers.toxiproxy.ToxiproxyContainer;

/**
 * Network failure you can switch on and off inside a test.
 *
 * <p>A dependency reached through {{@link Faults}} is reached through a proxy
 * you control, so "the database went away mid-transaction" and "the broker
 * answers, slowly" stop being things you reason about and become things you
 * assert. Stopping the dependency's container proves much less: it takes
 * seconds, it cannot be undone, and it never reproduces the case that actually
 * pages you -- a connection that is up, accepted, and then silent.
 *
 * <p>Point the application at {{@link Fault#host()}} and {{@link Fault#port()}}
 * rather than at the dependency's own address. Traffic sent to the real address
 * bypasses the proxy, and the test then passes for no reason:
 *
 * {{@snippet :
 * try (var faults = Faults.start()) {{
 *     var postgres = new PostgreSQLContainer("postgres:17-alpine")
 *             .withNetwork(faults.network())
 *             .withNetworkAliases("postgres");
 *     postgres.start();
 *     var db = faults.inFrontOf("postgres", 5432);
 *
 *     // ... point the datasource at db.host():db.port() ...
 *     db.cut();
 *     assertThatThrownBy(() -> repository.findAll()).isInstanceOf(DataAccessException.class);
 *     db.restore();
 * }}
 * }}
 */
public final class Faults implements AutoCloseable {{

    private static final String IMAGE = "ghcr.io/shopify/toxiproxy:2.12.0";

    /**
     * Toxiproxy listens on a port per proxy, and a container's ports have to be
     * declared before it starts -- so a fixed handful are opened up front and
     * handed out as proxies are created.
     */
    private static final int FIRST_LISTEN_PORT = 8666;

    /** The proxy's own alias on {{@link #network()}}, and its control port. */
    public static final String ALIAS = "toxiproxy";

    public static final int CONTROL_PORT = 8474;

    private static final int LISTEN_PORTS = 8;

    private final Network network;
    private final ToxiproxyContainer container;
    private final ToxiproxyClient client;
    private final AtomicInteger nextPort = new AtomicInteger(FIRST_LISTEN_PORT);

    private Faults(Network network, ToxiproxyContainer container) {{
        this.network = network;
        this.container = container;
        // getControlPort() is already the mapped port, not 8474 -- mapping it
        // again asks for a port that was never exposed.
        this.client = new ToxiproxyClient(container.getHost(), container.getControlPort());
    }}

    /** Starts the proxy. Put every container you want to disturb on {{@link #network()}}. */
    public static Faults start() {{
        var network = Network.newNetwork();
        var ports = new Integer[LISTEN_PORTS + 1];
        ports[0] = CONTROL_PORT;
        for (int i = 0; i < LISTEN_PORTS; i++) {{
            ports[i + 1] = FIRST_LISTEN_PORT + i;
        }}
        var container = new ToxiproxyContainer(IMAGE)
                .withNetwork(network)
                .withNetworkAliases(ALIAS)
                .withExposedPorts(ports);
        container.start();
        return new Faults(network, container);
    }}

    /** The network the proxy is on. A container is only reachable if it shares this. */
    public Network network() {{
        return network;
    }}

    /**
     * A proxy in front of {{@code alias:port}}, where {{@code alias}} is the
     * network alias of another container on {{@link #network()}}.
     */
    public Fault inFrontOf(String alias, int port) {{
        return proxy(alias + "-" + port, alias + ":" + port);
    }}

    /**
     * A proxy in front of a server running in this JVM -- a stub HTTP server, an
     * embedded broker -- rather than in a container.
     */
    public Fault inFrontOfHost(int port) {{
        Testcontainers.exposeHostPorts(port);
        return proxy("host-" + port, "host.testcontainers.internal:" + port);
    }}

    private Fault proxy(String name, String upstream) {{
        var listen = nextPort.getAndIncrement();
        if (listen >= FIRST_LISTEN_PORT + LISTEN_PORTS) {{
            throw new IllegalStateException("no listen port left: Faults opens " + LISTEN_PORTS);
        }}
        try {{
            var proxy = client.createProxy(name, "0.0.0.0:" + listen, upstream);
            return new Fault(proxy, container.getHost(), container.getMappedPort(listen));
        }} catch (IOException error) {{
            throw new UncheckedIOException("could not proxy " + upstream, error);
        }}
    }}

    @Override
    public void close() {{
        container.stop();
        network.close();
    }}

    /** One proxied dependency, and the ways it is allowed to misbehave. */
    public record Fault(Proxy proxy, String host, int port) {{

        /**
         * Refuses every connection, and drops the ones already open. What a
         * process being killed looks like from the other side.
         */
        public void cut() {{
            run(proxy::disable);
        }}

        public void restore() {{
            run(proxy::enable);
        }}

        /** Delays every packet, in both directions. Use to prove a timeout exists. */
        public void latency(Duration delay) {{
            run(() -> proxy.toxics().latency("latency", ToxicDirection.DOWNSTREAM, delay.toMillis()));
        }}

        /**
         * Accepts the connection and then never answers, until {{@code after}}
         * bytes have gone by. The failure a missing read timeout hangs on
         * forever -- and the one that a "is the port open" health check misses.
         */
        public void blackhole() {{
            run(() -> proxy.toxics().timeout("timeout", ToxicDirection.DOWNSTREAM, 0));
        }}

        /**
         * Undoes everything: removes every toxic *and* re-enables a cut proxy.
         *
         * <p>Both, deliberately. A {{@code heal}} that only dropped the toxics
         * would leave a {{@link #cut}} in place, and the next test would fail
         * against a dependency it never touched -- with an error that points at
         * the wrong test.
         */
        public void heal() {{
            run(() -> {{
                for (var toxic : proxy.toxics().getAll()) {{
                    toxic.remove();
                }}
                proxy.enable();
            }});
        }}

        private static void run(Failing action) {{
            try {{
                action.run();
            }} catch (IOException error) {{
                throw new UncheckedIOException("toxiproxy refused the change", error);
            }}
        }}

        @FunctionalInterface
        private interface Failing {{
            void run() throws IOException;
        }}
    }}
}}
"##
    )
}

fn faults_test_java(pkg: &str) -> String {
    format!(
        r##"package {pkg};

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.io.IOException;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.net.http.HttpTimeoutException;
import java.time.Duration;
import org.junit.jupiter.api.Test;

/**
 * Proves the fault injector itself works, so a failure elsewhere is never its
 * fault.
 *
 * <p>The upstream is Toxiproxy's own control API, reached through a proxy that
 * Toxiproxy is running. That sounds cute but it is the most honest option
 * available: it needs no second image and no bridge back to a port on the test
 * JVM, so a failure here is the proxy misbehaving and cannot be anything else.
 */
class FaultsTest {{

    private static final Duration PATIENCE = Duration.ofSeconds(5);

    /** Long enough to rule out slowness, short enough that a hang fails fast. */
    private static final Duration IMPATIENCE = Duration.ofSeconds(2);

    @Test
    void aProxiedDependencyAnswersUntilItIsCutAndThenAgainAfterItIsRestored() throws Exception {{
        try (var faults = Faults.start()) {{
            var fault = faults.inFrontOf(Faults.ALIAS, Faults.CONTROL_PORT);

            assertThat(status(fault, PATIENCE)).as("the proxy passes traffic through").isEqualTo(200);

            fault.cut();
            assertThatThrownBy(() -> status(fault, PATIENCE))
                    .as("a cut dependency refuses the connection")
                    .isInstanceOf(IOException.class);

            fault.restore();
            assertThat(status(fault, PATIENCE)).as("the dependency came back").isEqualTo(200);
        }}
    }}

    @Test
    void aBlackholedDependencyAcceptsTheConnectionAndThenSaysNothing() throws Exception {{
        try (var faults = Faults.start()) {{
            var fault = faults.inFrontOf(Faults.ALIAS, Faults.CONTROL_PORT);
            fault.blackhole();

            // The failure a missing read timeout hangs on forever: the socket
            // is open, so anything that checks only "did it connect" believes
            // the dependency is healthy.
            assertThatThrownBy(() -> status(fault, IMPATIENCE))
                    .isInstanceOf(HttpTimeoutException.class);

            fault.heal();
            assertThat(status(fault, PATIENCE)).isEqualTo(200);
        }}
    }}

    @Test
    void latencyIsAddedToAnOtherwiseHealthyDependency() throws Exception {{
        try (var faults = Faults.start()) {{
            var fault = faults.inFrontOf(Faults.ALIAS, Faults.CONTROL_PORT);
            fault.latency(Duration.ofSeconds(3));

            assertThatThrownBy(() -> status(fault, IMPATIENCE))
                    .as("a caller more impatient than the delay gives up")
                    .isInstanceOf(HttpTimeoutException.class);

            fault.heal();
            assertThat(status(fault, PATIENCE)).isEqualTo(200);
        }}
    }}

    private static int status(Faults.Fault fault, Duration timeout) throws Exception {{
        try (var http = HttpClient.newHttpClient()) {{
            var request = HttpRequest.newBuilder(URI.create("http://%s:%d/version".formatted(fault.host(), fault.port())))
                    .timeout(timeout)
                    .build();
            return http.send(request, HttpResponse.BodyHandlers.discarding()).statusCode();
        }}
    }}
}}
"##
    )
}

fn scripted_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * A collaborator that replays a fixed script and records how it was called.
 *
 * <p>Attach it to any interface with a lambda -- which is why this is one class
 * rather than one fake per interface, and why it needs no mocking framework:
 *
 * {{@snippet :
 * var model = Fake.of(Fake.value("ok"), Fake.failure(new IllegalStateException("timeout")));
 * ModelProvider provider = prompt -> model.next(prompt);
 *
 * assertThat(provider.generate("hello")).isEqualTo("ok");
 * assertThat(model.calls()).containsExactly(List.of("hello"));
 * }}
 *
 * <p>Once the script runs out the last step repeats, so a test that only cares
 * about the first response does not have to pad the script to match.
 */
public final class Fake<T> {{

    /** One scripted turn. Sealed, so a switch over it is checked for exhaustiveness. */
    public sealed interface Step<T> {{}}

    public record Value<T>(T value) implements Step<T> {{}}

    public record Failure<T>(RuntimeException error) implements Step<T> {{}}

    private final List<Step<T>> script;
    private final List<List<Object>> calls = new ArrayList<>();
    private int index = 0;

    private Fake(List<Step<T>> script) {{
        if (script.isEmpty()) {{
            throw new IllegalArgumentException("a fake needs at least one step");
        }}
        this.script = List.copyOf(script);
    }}

    @SafeVarargs
    public static <T> Fake<T> of(Step<T>... steps) {{
        return new Fake<>(List.of(steps));
    }}

    public static <T> Step<T> value(T value) {{
        return new Value<>(value);
    }}

    public static <T> Step<T> failure(RuntimeException error) {{
        return new Failure<>(error);
    }}

    /**
     * Records the arguments it was called with, then plays the next step.
     *
     * <p>{{@code Stream.toList()}} rather than {{@code List.of}}: a null argument
     * is a perfectly ordinary thing to want to assert a collaborator was
     * called with, and {{@code List.of}} rejects it.
     */
    public T next(Object... arguments) {{
        calls.add(Arrays.stream(arguments).toList());
        var step = script.get(Math.min(index++, script.size() - 1));
        return switch (step) {{
            case Value<T>(var value) -> value;
            case Failure<T>(var error) -> throw error;
        }};
    }}

    /** Every call so far, in order, each as its argument list. */
    public List<List<Object>> calls() {{
        return List.copyOf(calls);
    }}

    public int callCount() {{
        return calls.size();
    }}
}}
"#
    )
}

fn scripted_test_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;

import java.util.List;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

class FakeTest {{

    @Test
    void playsEachStepInOrder() {{
        var fake = Fake.of(Fake.value("first"), Fake.value("second"));

        assertThat(fake.next()).isEqualTo("first");
        assertThat(fake.next()).isEqualTo("second");
    }}

    @Test
    void repeatsTheLastStepOnceTheScriptRunsOut() {{
        var fake = Fake.of(Fake.value("only"));

        assertThat(fake.next()).isEqualTo("only");
        assertThat(fake.next()).isEqualTo("only");
    }}

    @Test
    void throwsWhateverTheScriptSaysToThrow() {{
        var fake = Fake.<String>of(Fake.failure(new IllegalStateException("simulated timeout")));

        assertThatThrownBy(fake::next).isInstanceOf(IllegalStateException.class).hasMessage("simulated timeout");
    }}

    @Test
    void recordsHowItWasCalled() {{
        var fake = Fake.of(Fake.value(1));

        fake.next("a", 2);
        fake.next("b");

        assertThat(fake.calls()).containsExactly(List.of("a", 2), List.of("b"));
        assertThat(fake.callCount()).isEqualTo(2);
    }}

    @Test
    void rejectsAnEmptyScript() {{
        assertThatThrownBy(Fake::of).isInstanceOf(IllegalArgumentException.class);
    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// http
// ---------------------------------------------------------------------------

/// An HTTP server with no dependency at all: `com.sun.net.httpserver` has
/// shipped in the JDK since 6 and is a supported API, and `java.net.http`
/// gives the test its client. A framework here would be the biggest dependency
/// in the project and buy nothing a route map does not.
fn http_plan(root: &std::path::Path, pkg: &str, name: Option<&str>) -> Result<Plan> {
    let base = name.map(capitalize).unwrap_or_default();
    let class = format!("{base}Server");

    Ok(Plan {
        files: vec![
            NewFile {
                path: main_dir(root, pkg).join(format!("{class}.java")),
                contents: http_server_java(pkg, &class),
            },
            NewFile {
                path: test_dir(root, pkg).join(format!("{class}Test.java")),
                contents: http_server_test_java(pkg, &class),
            },
        ],
        ..Plan::default()
    })
}

fn http_server_java(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpServer;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * A small HTTP server on the JDK's own {{@code com.sun.net.httpserver}} -- no
 * framework, no container, no dependency.
 *
 * <p>Handlers are pure functions from {{@link Request}} to {{@link Response}}, so
 * the interesting half can be unit-tested without any socket at all; this class
 * only maps them onto HTTP.
 *
 * <p>Requests are served on virtual threads, so a handler that blocks on I/O
 * costs a stack, not a platform thread.
 *
 * {{@snippet :
 * try (var server = {class}.start(0, Map.of("/health", request -> Response.ok("{{\"status\":\"up\"}}")))) {{
 *     var uri = URI.create("http://localhost:" + server.port() + "/health");
 * }}
 * }}
 */
public final class {class} implements AutoCloseable {{

    /** Everything a handler is allowed to see. */
    public record Request(String method, String path, String query, String body) {{}}

    /** Everything a handler can say. JSON by default -- override for anything else. */
    public record Response(int status, String contentType, String body) {{

        public static Response ok(String body) {{
            return new Response(200, "application/json", body);
        }}

        public static Response text(String body) {{
            return new Response(200, "text/plain; charset=utf-8", body);
        }}

        public static Response notFound() {{
            return new Response(404, "application/json", "{{\"error\":\"not found\"}}");
        }}

        public static Response badRequest(String message) {{
            return new Response(400, "application/json", "{{\"error\":\"" + escape(message) + "\"}}");
        }}

        /**
         * Escapes exactly what a JSON string body needs. Deliberately not a JSON
         * library: this class has no dependencies, and one interpolated message
         * does not justify adding one. Build real payloads with a real
         * serialiser -- {{@code jails add json}} gives you Jackson.
         */
        private static String escape(String text) {{
            var out = new StringBuilder(text.length() + 16);
            for (var c : text.toCharArray()) {{
                switch (c) {{
                    case '"' -> out.append("\\\"");
                    case '\\' -> out.append("\\\\");
                    case '\n' -> out.append("\\n");
                    case '\r' -> out.append("\\r");
                    case '\t' -> out.append("\\t");
                    // Appended from a char rather than written as one literal:
                    // Java translates a backslash-u escape before it even lexes
                    // the file, and %04x is not four hex digits, so the obvious
                    // spelling is an "illegal unicode escape" at compile time.
                    // (Which applies to comments too -- hence this wording.)
                    default -> {{
                        if (c < 0x20) {{
                            out.append('\\').append("u%04x".formatted((int) c));
                        }} else {{
                            out.append(c);
                        }}
                    }}
                }}
            }}
            return out.toString();
        }}
    }}

    @FunctionalInterface
    public interface Handler {{
        Response handle(Request request);
    }}

    private final HttpServer http;
    private final ExecutorService requests;

    private {class}(HttpServer http, ExecutorService requests) {{
        this.http = http;
        this.requests = requests;
    }}

    /**
     * Binds and starts. Pass port 0 to let the OS pick a free one and read it
     * back from {{@link #port()}} -- which is what makes tests safe to run in
     * parallel, and CI safe from whatever else is listening on 8080.
     */
    public static {class} start(int port, Map<String, Handler> routes) {{
        try {{
            var http = HttpServer.create(new InetSocketAddress(port), 0);
            routes.forEach((path, handler) -> http.createContext(path, exchange -> dispatch(exchange, handler)));
            var requests = Executors.newVirtualThreadPerTaskExecutor();
            http.setExecutor(requests);
            http.start();
            return new {class}(http, requests);
        }} catch (IOException error) {{
            throw new UncheckedIOException("could not start the server on port " + port, error);
        }}
    }}

    public int port() {{
        return http.getAddress().getPort();
    }}

    private static void dispatch(HttpExchange exchange, Handler handler) throws IOException {{
        try (exchange) {{
            var uri = exchange.getRequestURI();
            Response response;
            try (var in = exchange.getRequestBody()) {{
                var body = new String(in.readAllBytes(), StandardCharsets.UTF_8);
                var request = new Request(exchange.getRequestMethod(), uri.getPath(), uri.getQuery(), body);
                // A handler that throws must not leave the connection hanging:
                // the client would block until it timed out, with nothing said.
                response = handle(handler, request);
            }}

            var bytes = response.body().getBytes(StandardCharsets.UTF_8);
            exchange.getResponseHeaders().set("Content-Type", response.contentType());
            exchange.sendResponseHeaders(response.status(), bytes.length);
            try (var out = exchange.getResponseBody()) {{
                out.write(bytes);
            }}
        }}
    }}

    private static Response handle(Handler handler, Request request) {{
        try {{
            return handler.handle(request);
        }} catch (RuntimeException error) {{
            // The client gets nothing useful (deliberately -- an exception
            // message can carry internals), but swallowing it outright leaves
            // nobody anything to debug from. Swap in a logger when you add one.
            System.err.println("handler failed for " + request.method() + " " + request.path());
            error.printStackTrace();
            return new Response(500, "application/json", "{{\"error\":\"internal error\"}}");
        }}
    }}

    /**
     * Stops accepting connections and shuts the request executor down.
     *
     * <p>Both halves matter: {{@link HttpServer#stop}} does <em>not</em> shut down
     * an executor the caller supplied, so stopping without this leaks one per
     * server -- which a test that starts a server per case does many times over.
     */
    @Override
    public void close() {{
        http.stop(0);
        requests.close();
    }}
}}
"#
    )
}

fn http_server_test_java(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;

import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.util.Map;

import static org.assertj.core.api.Assertions.assertThat;

/** End-to-end over a real socket, on an ephemeral port so nothing collides. */
class {class}Test {{

    private static final Map<String, {class}.Handler> ROUTES = Map.of(
            "/health", request -> {class}.Response.ok("{{\"status\":\"up\"}}"),
            "/echo", request -> {class}.Response.text(request.method() + " " + request.body()),
            "/boom", request -> {{
                throw new IllegalStateException("handler blew up");
            }});

    private HttpResponse<String> call(int port, String path, String body) throws Exception {{
        var request = HttpRequest.newBuilder(URI.create("http://localhost:" + port + path))
                .method(body == null ? "GET" : "POST", body == null
                        ? HttpRequest.BodyPublishers.noBody()
                        : HttpRequest.BodyPublishers.ofString(body))
                .build();
        try (var client = HttpClient.newHttpClient()) {{
            return client.send(request, HttpResponse.BodyHandlers.ofString());
        }}
    }}

    @Test
    void servesARegisteredRoute() throws Exception {{
        try (var server = {class}.start(0, ROUTES)) {{
            var response = call(server.port(), "/health", null);

            assertThat(response.statusCode()).isEqualTo(200);
            assertThat(response.body()).contains("up");
            assertThat(response.headers().firstValue("Content-Type")).hasValue("application/json");
        }}
    }}

    @Test
    void handsTheHandlerTheMethodAndBody() throws Exception {{
        try (var server = {class}.start(0, ROUTES)) {{
            assertThat(call(server.port(), "/echo", "hello").body()).isEqualTo("POST hello");
        }}
    }}

    @Test
    void answersUnknownPathsWithFourOhFour() throws Exception {{
        try (var server = {class}.start(0, ROUTES)) {{
            assertThat(call(server.port(), "/nope", null).statusCode()).isEqualTo(404);
        }}
    }}

    /** A throwing handler must still answer, or the client just hangs. */
    @Test
    void turnsAHandlerExceptionIntoAFiveHundred() throws Exception {{
        try (var server = {class}.start(0, ROUTES)) {{
            assertThat(call(server.port(), "/boom", null).statusCode()).isEqualTo(500);
        }}
    }}

    @Test
    void picksAFreePortWhenAskedForZero() {{
        try (var server = {class}.start(0, ROUTES)) {{
            assertThat(server.port()).isPositive();
        }}
    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// format
// ---------------------------------------------------------------------------

/// Spotless, bound to `verify` as a check and available as `jails fmt` to
/// apply. Formatting nobody has to think about is the only kind that survives.
const SPOTLESS_ARTIFACT: &str = "spotless-maven-plugin";

fn format_plan() -> Result<Plan> {
    Ok(Plan {
        plugins: vec![(SPOTLESS_ARTIFACT, SPOTLESS_PLUGIN.to_string())],
        ..Plan::default()
    })
}

/// palantir-java-format over google-java-format: it keeps a 120-column line,
/// which the generated code (records with several components, fluent AssertJ
/// chains) reads far better at than 100. Both are pinned -- a formatter that
/// drifts version rewrites files nobody touched.
const SPOTLESS_PLUGIN: &str = r#"<plugin>
    <groupId>com.diffplug.spotless</groupId>
    <artifactId>spotless-maven-plugin</artifactId>
    <version>3.9.0</version>
    <configuration>
        <java>
            <palantirJavaFormat>
                <version>2.97.0</version>
            </palantirJavaFormat>
            <removeUnusedImports/>
        </java>
    </configuration>
    <executions>
        <execution>
            <id>spotless-check</id>
            <phase>verify</phase>
            <goals>
                <goal>check</goal>
            </goals>
        </execution>
    </executions>
</plugin>"#;

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
