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

use crate::compose::{self, Service as ComposeService};
use crate::generate::{base_package, import_of, main_dir, package_of, test_dir, write_new_file};
use crate::model::{Artifact, Change, Layer, Project, Slice, SpringTestImport};
use crate::pom::{self, Dependency, Flavor, MIN_RELEASE, TARGET_RELEASE};
use jails_support::Result;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

mod messaging;
use messaging::*;

mod data;
use data::*;

mod testing;
use testing::*;

mod tooling;
use tooling::*;

mod database;

/// Drop the compiled shadow of a source file the transition deleted.
///
/// `target/` is derived output: it is outside every route's mutable scope, it
/// is not guarded, and a transaction has no business rewriting it. But
/// `mvn test` is incremental, so a deleted `TestcontainersConfig.java` whose
/// `.class` is still under `target/test-classes` keeps being loaded -- the
/// removal looks like it did not happen, and the failure appears in a test
/// run rather than at the command that caused it.
///
/// So this runs *after* the commit, from the receipt's own delete list, and
/// is deliberately best-effort: a `target/` jails cannot write is a `target/`
/// the next build regenerates.
pub fn drop_compiled_shadow(root: &std::path::Path, deleted: &std::path::Path) {
    database::delete_maven_output(root, deleted);
}

mod shrink;
mod test_wiring;
use database::*;
pub use shrink::*;
use test_wiring::*;

pub use crate::spec::kind::Capability;

/// Change every requested capability before any of them is applied.
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
    preflight_in(&Project::discover()?, capabilities, name, package)
}

/// The same check against a project the caller already resolved.
pub fn preflight_in(
    project: &Project,
    capabilities: &[Capability],
    name: Option<&str>,
    package: Option<&str>,
) -> Result<()> {
    if capabilities.len() < 2 {
        return Ok(());
    }
    require_java_release(project.java_release())?;
    let mut combined = Change::default();
    for &capability in capabilities {
        let planned = build_plan(capability, project, name, package).map_err(|e| {
            format!(
                "{e}\n\nnothing was written -- `{}` was refused, so none of the {} \
                 requested capabilities were applied.",
                capability.label(),
                capabilities.len()
            )
        })?;
        combined = combined.merge(planned).map_err(|e| {
            format!(
                "{e}\n\nnothing was written -- the {} requested capabilities conflict.",
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
    add_in(
        &Project::discover()?,
        capability,
        name,
        dry_run,
        package,
        debug,
        no_start,
    )
}

/// Apply a capability to a project the caller has already resolved.
///
/// `Project::discover()` reads the process CWD, which is the right default at
/// the CLI boundary and wrong everywhere else: `jails new --app <manifest>`
/// applies to the project it just created, not to whatever encloses the
/// directory the user happens to be standing in. Resolving once at the top and
/// threading the value is rung 1's whole shape.
#[allow(clippy::too_many_arguments)]
pub fn add_in(
    project: &Project,
    capability: Capability,
    name: Option<&str>,
    dry_run: bool,
    package: Option<&str>,
    debug: bool,
    no_start: bool,
) -> Result<()> {
    // Not exempted, on purpose (`plan.md` §12): a capability is a dependency
    // plus code plus a test, and jails will not edit a build file it refuses to
    // read. A capability that installs the code and silently skips the
    // dependency is worse than one that refuses -- the reader gets a compile
    // error for a file they did not write.
    project.require_maven(capability.label())?;
    let root = project.root().to_path_buf();
    let pom_text = project.pom().to_string();
    let flavor = project.flavor();
    require_java_release(project.java_release())?;
    let plan = build_plan(capability, project, name, package)?;

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
        match pom::add_dependency(&updated_pom, &crate::pom::SPRING_DOCKER_COMPOSE)? {
            Some(next) => {
                updated_pom = next;
                docker_compose_dep = true;
            }
            None => println!(
                "  exists  {}:{}",
                crate::pom::SPRING_DOCKER_COMPOSE.group_id,
                crate::pom::SPRING_DOCKER_COMPOSE.artifact_id
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
                crate::pom::SPRING_DOCKER_COMPOSE.group_id,
                crate::pom::SPRING_DOCKER_COMPOSE.artifact_id
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
        crate::apply::put_named(root.join("pom.xml"), &updated_pom, "pom.xml")?;
        for dep in &spliced {
            println!("     dep  {}:{}", dep.group_id, dep.artifact_id);
        }
        if docker_compose_dep {
            println!(
                "     dep  {}:{} (optional)",
                crate::pom::SPRING_DOCKER_COMPOSE.group_id,
                crate::pom::SPRING_DOCKER_COMPOSE.artifact_id
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
                    jails_java::tidy::normalize_imports(&file.contents)
                } else {
                    file.contents.clone()
                };
                // This is the write plan.md §11 names: a capability updating a
                // file it wrote before, which used to go straight past the
                // collision check with a bare `fs::write`. `replace` says so.
                crate::apply::replace(&file.path, &contents)?;
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

    // Same rule as Failsafe: a capability that writes a test writes it
    // against AssertJ, so the project has to have AssertJ. `add testkit` on a
    // plain Maven project jails did not create is where this showed up --
    // six `cannot find symbol: method assertThat` for a file the reader never
    // wrote.
    crate::generate::ensure_assertj(
        project,
        plan.files
            .iter()
            .any(|f| f.path.to_string_lossy().contains("src/test/java")),
    )?;

    // And the same rule for Boot 4's servlet test slice. `add security`
    // generates a `@WebMvcTest`, and `spring-boot-starter-test` does not
    // bring the module that holds it -- so without this, `mvn verify` stops
    // while compiling the generated test and no test in the project runs.
    crate::generate::ensure_webmvc_test(
        project,
        crate::generate::writes_a_webmvc_test(&plan.files),
    )?;

    // Same rule as the generators: a capability that writes an `*IT` has to
    // make sure something runs it. Failsafe is not in the Spring Boot
    // parent's default build, so without this `mvn verify` completes,
    // reports success, and executes none of them.
    if plan.files.iter().any(|f| {
        f.path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("IT.java"))
    }) && let Some(next) = pom::add_plugin(
        &updated_pom,
        crate::spring::FAILSAFE_ARTIFACT,
        crate::spring::failsafe_plugin(pom::flavor(&updated_pom)),
    )? {
        crate::apply::put_named(root.join("pom.xml"), &next, "pom.xml")?;
        println!("  plugin  {}", crate::spring::FAILSAFE_ARTIFACT);
        tests_wired = true;
    }

    if created == 0 && !pom_changed && compose_added.is_empty() && !tests_wired {
        // Still recorded: the capability *is* part of this project, and a
        // manifest that omits it because it happened to be installed before
        // the manifest existed is a manifest `sync` would act on wrongly.
        crate::config::record_capability(&root, capability.label())?;
        // `format` is the one capability whose work is not finished by its
        // pom edit: it also has to leave the sources formatted. Code written
        // *after* it was installed has never been through the formatter, and
        // that is the normal case rather than an odd one -- `jails app apply`
        // installs every capability first and then runs the generate intents,
        // so on a manifest naming `format` every generated file arrives after
        // the plugin does. Returning early here is what made App D fail
        // `jails check` on a project whose every line jails wrote.
        if matches!(capability, Capability::Format) {
            if crate::maven::format_quietly(&root) {
                println!("  format  applied to the sources generated since");
            } else {
                println!("  note    could not run the formatter on this toolchain");
            }
            return Ok(());
        }
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
        if crate::maven::format_quietly(&root) {
            println!("  format  applied to the existing sources");
        } else {
            crate::apply::put_named(root.join("pom.xml"), &pom_text, "pom.xml")?;
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

/// The project must be able to compile what a capability installs.
///
/// `pub` because both write paths need it and there must not be two copies:
/// V1's `add_in` and the V2 capability route ask the same question of the same
/// resolved `Project`.
pub fn require_java_release(release: Option<u32>) -> Result<()> {
    match release {
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

/// The `Change` a capability would make to this project, computed and not applied.
///
/// This is the half of `add` that `doctor` needs and could not reach.
/// abstract.md §4.2 calls the consequence Feature Envy at module scale:
/// `doctor.rs` re-derives, by reading the project back off disk, the facts
/// `add/database.rs`, `add/messaging.rs` and `add/data.rs` already own -- and
/// the drift between the two has no test, because there was no shared value to
/// compare. There is one now.
///
/// Planning is pure: no writes, no subprocesses. That is what makes it safe
/// for `doctor`, which is read-only by contract.
pub fn plan_for(capability: Capability, project: &Project) -> Result<Change> {
    build_plan(capability, project, None, None)
}

/// The same, for the capabilities that take a subject.
///
/// `csv`, `sqlite`, `json` and `http` are about *something* -- a reader for one
/// record, a database, a server class -- so they take the name the caller gave,
/// and plan.md §R1.1 makes those parameters part of which capability it is.
/// Which capability accepts which parameter is refused at
/// `CapabilityId::resolve`, not here: this is one entry point rather than a
/// second table saying which recipes read a name.
pub fn plan_named(
    capability: Capability,
    project: &Project,
    name: Option<&str>,
    package: Option<&str>,
) -> Result<Change> {
    build_plan(capability, project, name, package)
}

fn build_plan(
    capability: Capability,
    project: &Project,
    name: Option<&str>,
    package: Option<&str>,
) -> Result<Change> {
    // One value reaches every capability plan: the resolved project plus the
    // `--package` override. `root`, `flavor` and a per-layer `place` closure
    // used to be unpacked here and threaded on as three more arguments, which
    // is how a plan could be handed a flavour that disagreed with the pom it
    // was about to splice.
    let slice = Slice::new(project, package);
    match capability {
        Capability::Db => db_plan(&slice),
        Capability::Kafka => kafka_plan(&slice),
        Capability::Csv => csv_plan(&slice, name),
        Capability::Sqlite => sqlite_plan(&slice, name),
        Capability::Json => json_plan(&slice, name),
        Capability::Testkit => testkit_plan(&slice),
        Capability::Fake => fake_plan(&slice),
        Capability::Http => http_plan(&slice, name),
        Capability::Format => format_plan(&slice),
        Capability::Coverage => coverage_plan(),
        Capability::Loadtest => loadtest_plan(&slice),
        Capability::Ci => ci_plan(&slice),
        Capability::Docker => docker_plan(&slice),
        Capability::K8s => k8s_plan(&slice),
        Capability::Api => spring_slice_plan(&slice, "api", crate::spring::api_slice),
        Capability::Actuator => {
            spring_slice_plan(&slice, "actuator", crate::spring::actuator_slice)
        }
        Capability::Cache => spring_slice_plan(&slice, "cache", crate::spring::cache_slice),
        Capability::Security => {
            spring_slice_plan(&slice, "security", crate::spring::security_slice)
        }
        Capability::Cors => spring_slice_plan(&slice, "cors", crate::spring::cors_slice),
        Capability::Sse => spring_slice_plan(&slice, "sse", crate::spring::sse_slice),
        Capability::Mail => spring_slice_plan(&slice, "mail", crate::spring::mail_slice),
        Capability::Observability => {
            spring_slice_plan(&slice, "observability", crate::spring::observability_slice)
        }
        Capability::Toxiproxy => toxiproxy_plan(&slice),
        Capability::Redis => {
            spring_slice_plan(&slice, "redis", crate::spring::redis_slice).map(|plan| Change {
                compose: vec![compose::REDIS],
                ..plan
            })
        }
    }
}

/// Adapt a Spring-only slice to the shape `add` already executes. The Spring
/// check lives here rather than in each slice so there is one message for it.
/// Check the one precondition these capabilities share, then build.
///
/// The check used to run *after* the slice was built, so a plain-Maven project
/// rendered a pile of Spring Java and threw it away before refusing.
/// abstract.md §6.2 puts it the other way round: `require_spring` is a
/// precondition on the recipe, checked by `plan` against `Project.flavor`.
fn spring_slice_plan(
    slice: &Slice,
    capability: &str,
    build: fn(&Slice) -> Change,
) -> Result<Change> {
    crate::spring::require_spring(slice.flavor(), capability)?;
    Ok(build(slice))
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
        let db = database_java("com.example.demo", "ArchiveDatabase");
        assert!(db.contains("public record ArchiveDatabase(Path file)"));
        assert!(db.contains("public static ArchiveDatabase inMemory()"));

        let test = database_test_java("com.example.demo", "ArchiveDatabase", "ArchiveMigrations");
        assert!(test.contains("class ArchiveDatabaseTest"));
        assert!(test.contains("ArchiveMigrations.applyAll("));
    }

    #[test]
    fn json_pins_a_version_only_when_no_parent_manages_it() {
        // Spring Boot's parent already pins Jackson; declaring our own version
        // would override the curated one.
        assert_eq!(JACKSON.version, Some("3.0.1"));
        let (_sr, sp) = spring_project("json-plan-spring");
        let spring = json_plan(&Slice::new(&sp, None), None).unwrap();
        assert!(spring.deps.iter().all(|d| d.version.is_none()));
        let (_pr, pp) = plain_project("json-plan-plain");
        let plain = json_plan(&Slice::new(&pp, None), None).unwrap();
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
        let (_root, project) = plain_project("json-legacy");
        let plan = json_plan(&Slice::new(&project, None), None).unwrap();
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
        let (_sr, sp) = spring_project("json-one-spring");
        let (_pr, pp) = plain_project("json-one-plain");
        for project in [&sp, &pp] {
            let flavor = project.flavor();
            let plan = json_plan(&Slice::new(project, None), None).unwrap();
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

    /// A scratch project of each flavour.
    ///
    /// `build_plan` now hands every capability one `Slice`, so the flavour and
    /// the base package come from a resolved project rather than from three
    /// positional arguments a test could set inconsistently -- which is the
    /// point of the change these fixtures exist to support.
    fn plain_project(tag: &str) -> (std::path::PathBuf, Project) {
        crate::spring::scratch_project(tag, "<project></project>")
    }

    fn spring_project(tag: &str) -> (std::path::PathBuf, Project) {
        crate::spring::scratch_project(
            tag,
            "<project><parent><groupId>org.springframework.boot</groupId>\
             <artifactId>spring-boot-starter-parent</artifactId>\
             <version>4.0.0</version></parent></project>",
        )
    }

    #[test]
    fn db_plan_ships_compose_postgres_and_never_an_orm() {
        let (_root, project) = plain_project("db-plan-plain");
        let plan = db_plan(&Slice::new(&project, None)).unwrap();
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
        let (_root, project) = spring_project("db-plan-spring");
        let plan = db_plan(&Slice::new(&project, None)).unwrap();
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
        for expected in [
            "spring.datasource.hikari.pool-name=primary",
            "spring.datasource.hikari.maximum-pool-size=20",
            "spring.datasource.hikari.connection-timeout=1000",
            "spring.datasource.hikari.initialization-fail-timeout=1",
            "spring.datasource.hikari.transaction-isolation=TRANSACTION_READ_COMMITTED",
            "spring.datasource.hikari.connection-init-sql=SELECT 1/(1-pg_is_in_recovery()::int)",
            "server.shutdown=graceful",
            "spring.lifecycle.timeout-per-shutdown-phase=30s",
        ] {
            assert!(block.contains(expected), "missing {expected}: {block}");
        }
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
        assert!(
            block.contains("spring.datasource.username=rewards"),
            "{block}"
        );
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
    fn kafka_plan_is_a_client_plus_a_compose_broker() {
        // The Spring path reads the base package, for the deserializer's
        // trusted-packages list -- so it needs a project to read.
        let (_spring_root, spring_project) = spring_project("kafka-plan-spring");
        let spring = kafka_plan(&Slice::new(&spring_project, None)).unwrap();
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
            spring
                .properties
                .iter()
                .any(|p| p.contains("JacksonJsonSerializer")),
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
        let testcontainers = spring
            .files
            .iter()
            .find(|file| file.path.ends_with("KafkaTestcontainersConfig.java"))
            .expect("Spring Kafka should include an importable broker fixture");
        assert!(testcontainers.contents.contains("@ServiceConnection"));
        assert!(testcontainers.contents.contains("class ProcessKafka"));
        assert!(
            testcontainers.contents.contains("public void stop()"),
            "the broker must survive across importing contexts in one Failsafe JVM"
        );

        let (_plain_root, plain_project) = plain_project("kafka-plan-plain");
        let plain = kafka_plan(&Slice::new(&plain_project, None)).unwrap();
        assert_eq!(plain.deps[0].artifact_id, "kafka-clients");
        assert_eq!(plain.deps[0].version, Some("4.1.0"));
        assert_eq!(plain.compose, vec![compose::KAFKA]);
        // Plain Maven has no Spring properties file to write into.
        assert!(plain.properties.is_empty());
    }
}
