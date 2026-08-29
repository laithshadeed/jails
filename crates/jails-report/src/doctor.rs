//! `jails doctor` -- everything that has to be true before the application
//! can start, checked in one pass and reported as a list.
//!
//! The command exists for a specific failure shape: the app does not come
//! up, the stack trace names a Spring internal, and the actual cause is
//! three layers away -- Docker is not running, the JDK on PATH is older than
//! the release the pom targets, a `@Repository` lost its annotation, port
//! 8080 is still held by yesterday's run. Each of those is cheap to test
//! directly and expensive to infer from a trace.
//!
//! Two rules keep it honest. Nothing here writes, starts, or stops anything
//! -- `doctor` is safe to run at any moment, including mid-debug. And every
//! failing check carries the command that fixes it, because a diagnosis the
//! reader has to translate into an action has only moved the work.

pub use crate::diagnostic::{Check, Status};
use crate::model::Project;
use std::fmt::Write as _;
use std::path::Path;
mod environment;
mod wiring;
use environment::*;
use wiring::*;

mod h2;

use crate::{compose, inspect, pom};
use jails_support::Result;

pub fn doctor(project: &Project, json: bool, mut additional: Vec<Check>) -> Result<()> {
    let mut checks = run_checks(project);
    checks.append(&mut additional);

    if json {
        return report_json(&checks);
    }

    let title_width = checks.iter().map(|c| c.title.len()).max().unwrap_or(0);
    for check in &checks {
        println!(
            "{}  {:title_width$}  {}",
            check.status.mark(),
            check.title,
            check.detail
        );
        if !check.fix.is_empty() {
            println!("{:width$}      fix: {}", "", check.fix, width = title_width);
        }
    }

    let failures = checks.iter().filter(|c| c.status == Status::Fail).count();
    let warnings = checks.iter().filter(|c| c.status == Status::Warn).count();
    println!();
    if failures == 0 && warnings == 0 {
        println!("{} checks, all clear.", checks.len());
        return Ok(());
    }
    println!(
        "{} checks: {failures} failing, {warnings} warning(s).",
        checks.len()
    );
    if failures > 0 {
        // A non-zero exit is what makes `jails doctor && jails run` a usable
        // habit, so the failure is deliberately quiet -- the list above has
        // already said everything, and main.rs would otherwise print a
        // second, redundant `jails: ...` line.
        return Err(jails_support::Failure::Reported);
    }
    Ok(())
}

/// Every check, against one resolved snapshot of the project.
///
/// `root` and the pom text used to travel together into almost every check,
/// which is `Project` spelled as two parameters -- and the pair could be
/// handed on inconsistently, since nothing tied the text to the directory.
/// The same checks, as data.
///
/// Emitted from the identical `Vec<Check>` the human report prints, so the two
/// cannot disagree about what was checked -- which is the failure mode a second
/// rendering path would introduce, and the same reason `--pretend` and apply
/// have to consume one value.
///
/// The exit code is unchanged: failures still exit non-zero, because
/// `jails doctor --json && deploy` should behave like `jails doctor && deploy`.
fn report_json(checks: &[Check]) -> Result<()> {
    let rows: Vec<String> = checks
        .iter()
        .map(|check| {
            format!(
                "    {{\"status\": {}, \"title\": {}, \"detail\": {}, \"fix\": {}}}",
                crate::json::string(check.status.name()),
                crate::json::string(&check.title),
                crate::json::string(&check.detail),
                crate::json::string(&check.fix)
            )
        })
        .collect();
    let failures = checks.iter().filter(|c| c.status == Status::Fail).count();
    let warnings = checks.iter().filter(|c| c.status == Status::Warn).count();
    println!(
        "{{\n  \"schema_version\": 1,\n  \"failures\": {failures},\n  \"warnings\": {warnings},\n  \"checks\": [\n{}\n  ]\n}}",
        rows.join(",\n")
    );
    if failures > 0 {
        return Err(jails_support::Failure::Reported);
    }
    Ok(())
}

/// What jails' own bookkeeping says about itself.
///
/// Absent is not a finding: a project jails has never touched is the ordinary
/// case and has nothing to repair. Only `Unreadable` is, and it is a failure
/// rather than a warning, because nothing that writes can run until it is
/// resolved.
fn machine_state_check(project: &Project) -> Vec<Check> {
    let jails_state::compat::MachineState::Unreadable(why) =
        jails_state::compat::read(project.root())
    else {
        return Vec::new();
    };
    // `why` is the whole refusal every mutating command already prints, fix
    // line included, so the reader meets one sentence rather than two
    // descriptions of one file -- and this check cannot drift from the
    // refusal it is reporting.
    let (detail, fix) = why
        .split_once("\n       fix: ")
        .unwrap_or((why.as_str(), ""));
    vec![Check::new(Status::Fail, "jails state", detail.trim()).fix(fix.trim())]
}

fn run_checks(project: &Project) -> Vec<Check> {
    let root = project.root();
    let pom_text = project.pom();
    let mut checks = Vec::new();

    checks.push(project_check(project));
    // Before anything else about the project, because it decides whether the
    // rest of this report is about the project the reader has.
    //
    // `bugs.md` B47: with damaged machine state every mutating command
    // refused by name and `doctor` reported `25 checks, all clear`. That
    // state is what all of them plan against, so a report omitting it is
    // green on the one condition nothing can act on -- in the command a
    // reader runs when something is wrong. `compat` already classifies it
    // into exactly three answers; this asks it, and says nothing of its own.
    checks.extend(machine_state_check(project));
    // Nothing below reads a pom that is not there; the first check said why.
    if matches!(project.build(), crate::build::Build::Foreign(_)) {
        checks.extend(template_override_checks());
        return checks;
    }
    checks.push(maven_check(project));
    checks.push(jdk_check(project));
    checks.extend(compose_checks(project));
    checks.extend(compose_provider_check(pom_text));
    checks.extend(database_checks(project));
    checks.extend(in_memory_adapter_check(project));
    checks.push(testcontainers_check(pom_text));
    checks.extend(container_reuse_check(pom_text));
    checks.push(kafka_check(project));
    checks.push(jackson_check(project));
    checks.push(duplicate_key_check(project));
    checks.extend(management_checks(project));
    checks.extend(cors_checks(project));
    checks.extend(sql_init_checks(project));
    checks.extend(h2::checks(project));
    checks.extend(virtual_thread_checks(root));
    checks.extend(hot_reload_checks(project));
    checks.extend(port_checks(root));
    checks.extend(capability_drift_checks(project));
    checks.extend(crate::managed_drift::managed_output_checks(project));
    checks.extend(crate::managed_drift::migration_seal_checks(project));
    checks.extend(crate::managed_drift::disabled_generated_tests(project));
    checks.extend(crate::managed_drift::empty_migration_checks(project));
    checks.extend(crate::schema_lineage::schema_lineage_checks(project));
    checks.extend(template_override_checks());
    checks.push(beans_check(root));
    checks
}

/// Name every template this project has overridden.
///
/// `plan.md` §6.6 states the cost of tier 2 plainly: **an overridden template
/// is not golden-tested**, so a project that overrides one has opted out of
/// the guarantee for that file. The mitigation it names is this check -- the
/// same honesty rule as `remove`'s `unowned_properties`. A `Warn` rather than
/// a `Fail`: overriding is a supported thing to do, and the reader is entitled
/// to know they are doing it without being told they are broken.
fn template_override_checks() -> Vec<Check> {
    let active = crate::template::active();
    if active.is_empty() {
        return Vec::new();
    }
    let named = active
        .iter()
        .map(|(name, path)| format!("{name} <- {}", path.display()))
        .collect::<Vec<_>>()
        .join(", ");
    vec![
        Check::new(
            Status::Warn,
            "templates",
            format!(
                "{} template(s) overridden, so their output is not covered by \
                 jails' snapshot tests: {named}",
                active.len()
            ),
        )
        .fix("jails g <kind> --pretend, then read the generated file, if one of these looks wrong"),
    ]
}

/// Every capability `jails.toml` records, re-planned against today's project.
///
/// This is the check `doctor` could not write for its whole life, and the
/// reason is structural rather than an oversight: `add` knew which dependency,
/// property, file and compose service each capability installs, and `doctor`
/// could not reach that knowledge, so it re-encoded the parts it needed by
/// reading the project back off disk. abstract.md §4.2 names it Feature Envy
/// at module scale, and points out the sharp consequence -- the drift
/// `tests/agreement.rs` catches between `generate` and `destroy` has an exact
/// sibling between `add` and `doctor` that **nothing** catches, because there
/// was no shared value to compare.
///
/// `add::plan_for` is that shared value. Planning is pure, so asking it what a
/// capability *would* install costs nothing and writes nothing, and the delta
/// against what is actually there is the report.
///
/// What this deliberately does **not** do is replace the hand-written checks.
/// A derived check knows a dependency is missing; it does not know that two
/// Jackson majors on one classpath is a silent disaster, or that podman's
/// socket is somewhere Testcontainers will not look. Those are environment and
/// interaction facts no plan can carry, and abstract.md §6.2 says exactly that:
/// what survives is the checks that probe the environment.
fn capability_drift_checks(project: &Project) -> Vec<Check> {
    use clap::ValueEnum as _;

    let recorded = project.capabilities();
    if recorded.is_empty() {
        return vec![Check::new(
            Status::Skip,
            "capabilities",
            "jails.toml records none -- nothing to reconcile",
        )];
    }

    let properties = std::fs::read_to_string(
        project
            .root()
            .join("src/main/resources/application.properties"),
    )
    .unwrap_or_default();
    let compose = std::fs::read_to_string(project.root().join("compose.yaml")).unwrap_or_default();

    let mut checks = Vec::new();
    for label in recorded {
        let Some(capability) = crate::add::Capability::value_variants()
            .iter()
            .find(|candidate| candidate.label() == label)
        else {
            checks.push(
                Check::new(
                    Status::Warn,
                    "capability",
                    format!("jails.toml records `{label}`, which this jails does not know"),
                )
                .fix(format!(
                    "remove it from [project] capabilities, or upgrade jails: jails remove {label}"
                )),
            );
            continue;
        };

        // A capability that no longer *plans* is a finding in itself: it was
        // applied to a project that has since changed shape under it.
        let plan = match crate::add::plan_for(*capability, project) {
            Ok(plan) => plan,
            Err(error) => {
                checks.push(
                    Check::new(
                        Status::Fail,
                        format!("capability {label}"),
                        format!("recorded, but can no longer be planned: {error}"),
                    )
                    .fix(format!("jails remove {label}")),
                );
                continue;
            }
        };

        let mut missing = Vec::new();
        for dep in &plan.deps {
            // `Some(false)` only. A build file jails cannot read fully answers
            // `None`, and reporting that as a missing dependency would be
            // doctor inventing a defect -- the failure mode this check exists
            // to catch, pointed the wrong way. Silence is the honest report
            // for a question that cannot be answered.
            if project.declares_dependency(dep.group_id, dep.artifact_id) == Some(false) {
                missing.push(format!("dependency {}:{}", dep.group_id, dep.artifact_id));
            }
        }
        for file in &plan.files {
            if !file.path.exists() {
                missing.push(format!(
                    "file {}",
                    file.path
                        .strip_prefix(project.root())
                        .unwrap_or(&file.path)
                        .display()
                ));
            }
        }
        for property in &plan.properties {
            // A capability's property block is prose *and* settings: `add
            // cors` writes a line saying never to pair `*` with credentials,
            // and `add db` one saying jails starts compose itself. Reading a
            // comment as a required key made `doctor` demand a property whose
            // name was an English sentence, and no amount of `app apply`
            // could satisfy it.
            let line = property.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let key = line.split('=').next().unwrap_or_default().trim();
            // Only whole keys, and only outside comments: a commented example
            // naming the key is not the key being set.
            if !key.is_empty()
                && !properties.lines().any(|line| {
                    let line = line.trim();
                    !line.starts_with('#')
                        && line
                            .split('=')
                            .next()
                            .is_some_and(|current| current.trim() == key)
                })
            {
                missing.push(format!("property {key}"));
            }
        }
        for service in &plan.compose {
            if !compose.contains(&format!("{}:", service.name)) {
                missing.push(format!("compose service {}", service.name));
            }
        }

        if missing.is_empty() {
            checks.push(Check::new(
                Status::Ok,
                format!("capability {label}"),
                "everything it installs is present",
            ));
        } else {
            let shown = missing.len().min(3);
            let more = missing.len() - shown;
            let detail = format!(
                "{} missing: {}{}",
                missing.len(),
                missing[..shown].join(", "),
                if more > 0 {
                    format!(", and {more} more")
                } else {
                    String::new()
                }
            );
            checks.push(
                Check::new(Status::Fail, format!("capability {label}"), detail)
                    .fix("jails sync".to_string()),
            );
        }
    }
    checks
}

fn project_check(project: &Project) -> Check {
    let root: &Path = project.root();
    let pom_text: &str = project.pom();
    // Not optional (`plan.md` §12): a confident wrong report is worse than a
    // refusal, so the first thing doctor says about a foreign project is that
    // it is one -- and what that costs, since every check below reads a pom
    // that does not exist and would otherwise report cheerful nonsense.
    if let crate::build::Build::Foreign(tool) = project.build() {
        return Check::new(
            Status::Warn,
            "project",
            format!(
                "built by {tool}. jails never reads, writes, parses or invokes a {tool} \
                 build file, so it cannot see your dependencies: no check below would be \
                 telling you anything. `routes`, `beans`, `stats`, `notes`, `rename` and \
                 most of `generate` work here; `test`, `build`, `check` and `add` refuse."
            ),
        );
    }
    let gradle = matches!(project.build(), crate::build::Build::Gradle);
    let build_file = match gradle {
        true => jails_project::gradle::FILE,
        false => "pom.xml",
    };
    if pom_text.is_empty() {
        return Check::new(
            Status::Fail,
            "project",
            format!("{build_file} is missing or unreadable"),
        )
        .fix("jails new <name>");
    }
    // Named for the build file that was actually read. "plain Maven" over a
    // `build.gradle` is a confident wrong answer about the one fact this check
    // exists to state, and every reader takes the first line of `doctor` as
    // the ground truth for the rest of it.
    let flavor = match (project.flavor(), gradle) {
        (pom::Flavor::SpringBoot, true) => "Spring Boot (Gradle)",
        (pom::Flavor::SpringBoot, false) => "Spring Boot",
        (pom::Flavor::PlainMaven, true) => "plain Gradle",
        (pom::Flavor::PlainMaven, false) => "plain Maven",
    };
    let sources = root.join("src/main/java");
    if !sources.is_dir() {
        return Check::new(
            Status::Fail,
            "project",
            format!("{flavor}, but src/main/java does not exist"),
        );
    }
    // Before anything else about the project: can Maven open this pom at
    // all? `pom::read` falls back to an empty string, so without this every
    // check below happily reported on a project no goal can run against --
    // fifteen greens over a build that cannot start (plan.md §8.9).
    // `pom::problems` is an XML reader. Handing it Groovy would report
    // fabricated defects, so a Gradle build simply is not asked -- the honest
    // shape of "this check does not apply" is skipping it, not inventing an
    // answer.
    if let Some((problem, fix)) = match gradle {
        true => None,
        false => pom::problems(pom_text).into_iter().next(),
    } {
        return Check::new(
            Status::Fail,
            "project",
            format!("{flavor}, and Maven cannot read pom.xml: {problem}"),
        )
        .fix(&fix);
    }
    Check::new(
        Status::Ok,
        "project",
        format!("{flavor}, root {}", root.display()),
    )
}

fn compose_checks(project: &Project) -> Vec<Check> {
    let root: &Path = project.root();
    let _pom_text: &str = project.pom();
    let mut checks = Vec::new();
    if !compose::exists(root) {
        checks.push(Check::new(
            Status::Skip,
            "compose",
            "no compose.yaml -- this project declares no local services",
        ));
        return checks;
    }
    let yaml = compose::read(root).unwrap_or_default();
    let services = declared_services(&yaml);
    checks.push(Check::new(
        Status::Ok,
        "compose",
        format!("compose.yaml declares: {}", services.join(", ")),
    ));

    if !crate::process::on_path("docker") {
        checks.push(
            Check::new(
                Status::Fail,
                "docker",
                "compose.yaml declares services but docker is not on PATH",
            )
            .fix("install Docker, or remove the services with `jails remove db kafka`"),
        );
        return checks;
    }
    if !docker_daemon_running() {
        checks.push(
            Check::new(
                Status::Fail,
                "docker",
                "docker is installed but the daemon is not responding",
            )
            .fix("start Docker (`systemctl --user start docker` / open Docker Desktop)"),
        );
        return checks;
    }

    let running = running_containers();
    for service in &services {
        let up = service_is_running(service, &running);
        checks.push(if up {
            Check::new(Status::Ok, format!("service {service}"), "running")
        } else {
            Check::new(
                Status::Fail,
                format!("service {service}"),
                "declared in compose.yaml but not running",
            )
            .fix(format!("jails start {}", runtime_flag(service)))
        });
    }
    checks
}

/// The static half of a "required a bean of type ... that could not be
/// found" failure, available without starting the context.
fn beans_check(root: &Path) -> Check {
    let (beans, project_types) = inspect::collect_beans(root);
    if beans.is_empty() {
        return Check::new(
            Status::Skip,
            "beans",
            "no Spring stereotypes in src/main/java",
        );
    }
    let supplied = inspect::providers(&beans);
    let mut missing = Vec::new();
    let mut ambiguous = Vec::new();
    for bean in &beans {
        for need in &bean.needs {
            match supplied.get(need.as_str()).map(Vec::len).unwrap_or(0) {
                1 => {}
                // Spring will not choose between candidates, so two is as
                // broken as zero -- and it is the failure a project hits the
                // day it keeps an in-memory fake alongside a real adapter.
                n if n > 1 => ambiguous.push(format!(
                    "{need} has {n} candidates ({})",
                    supplied[need.as_str()].join(", ")
                )),
                _ if project_types.contains(need.as_str()) => {
                    missing.push(format!("{} needs {need}", bean.type_name))
                }
                _ => {}
            }
        }
    }
    if missing.is_empty() && ambiguous.is_empty() {
        return Check::new(
            Status::Ok,
            "beans",
            format!(
                "{} bean(s), every project-typed dependency resolvable",
                beans.len()
            ),
        );
    }
    let mut detail = String::new();
    if !missing.is_empty() {
        let _ = write!(detail, "unresolvable: {}", missing.join("; "));
    }
    if !ambiguous.is_empty() {
        if !detail.is_empty() {
            detail.push_str("; ");
        }
        ambiguous.dedup();
        let _ = write!(detail, "ambiguous: {}", ambiguous.join("; "));
    }
    Check::new(Status::Fail, "beans", detail).fix(if missing.is_empty() {
        "mark one candidate @Primary, or drop the stereotype from the fake"
    } else {
        "annotate the implementation (@Component) or add an @Bean method"
    })
}

// ---------------------------------------------------------------------------
// `jails setup` -- the machine-level settings a project cannot carry.
// ---------------------------------------------------------------------------

/// Turn on Testcontainers container reuse for this machine.
///
/// Everything else jails configures lives in the project, where it is visible,
/// reviewable and shared. This one cannot: Testcontainers reads
/// `testcontainers.reuse.enable` from `~/.testcontainers.properties` or the
/// environment and **never** from the classpath, so a project that asks for
/// reuse gets it only on a machine that has opted in. That asymmetry is the
/// whole reason this command exists.
///
/// **The flag alone changes nothing**, and that is deliberate. Generated
/// container configs do not call `withReuse(true)`, because the reuse key is
/// a hash of the container's configuration and nothing in it identifies the
/// project -- two applications on the same image would share one database,
/// and Flyway would refuse to start against the other one's migration
/// history. This command sets up the half a machine owns; the project half is
/// a one-line change the reader makes deliberately, and `TestcontainersConfig`
/// says so in its Javadoc.
///
/// The edit is a splice, not a rewrite: `~/.testcontainers.properties` is a
/// file the reader owns and may already hold `docker.client.strategy`,
/// `ryuk.disabled` or a registry mirror. Same rule as `pom.xml`.
pub fn setup(dry_run: bool) -> Result<()> {
    let Some(home) = std::env::var_os("HOME") else {
        return Err(jails_support::Failure::Told(
            "no HOME, so there is no ~/.testcontainers.properties to write".to_string(),
        ));
    };
    let path = Path::new(&home).join(".testcontainers.properties");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();

    if existing
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .any(|line| {
            line.replace(' ', "")
                .starts_with("testcontainers.reuse.enable=")
        })
    {
        // Present already -- including as `=false`, which is a decision, not
        // an omission. Flipping someone's explicit `false` would be jails
        // overruling them on their own machine.
        println!(
            "  exists  testcontainers.reuse.enable is already set in {}",
            path.display()
        );
        println!("          jails doctor reports whether it is on");
        return Ok(());
    }

    let mut next = existing.clone();
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(REUSE_BLOCK);

    if dry_run {
        println!("would add to {}:", path.display());
        for line in REUSE_BLOCK.lines() {
            println!("  {line}");
        }
        println!();
        println!("--dry-run: nothing was written.");
        return Ok(());
    }

    crate::apply::put_outside_project(&path, next)?;
    println!(
        "  write   testcontainers.reuse.enable=true -> {}",
        path.display()
    );
    println!("          This machine now permits reuse. Nothing reuses anything yet:");
    println!("          add `.withReuse(true)` to the container bean in TestcontainersConfig,");
    println!("          and read its Javadoc first -- two projects on one image share a");
    println!("          database, and Flyway will not start against another project's history.");
    println!("          Reused containers are not reaped; `jails doctor` counts them.");
    Ok(())
}

/// The note `jails setup` leaves above the key it writes.
///
/// `# jails --`, not `# jails:`: the second spells the opening of a marked
/// block, and this is prose in a file that has none. Nothing scans this file
/// for markers today, which is exactly when a confusable spelling is cheap to
/// remove -- `codemod`'s own rule is that a marker is matched on a whole line,
/// and a comment that starts like one is a line waiting to be misread.
const REUSE_BLOCK: &str = "\
# jails -- permit containers to be reused between test runs -- the largest
# saving available to a suite that starts PostgreSQL.
#
# This only permits it. A container is reused when its bean asks, with
# `withReuse(true)`, and that is a per-project decision: the reuse key is a
# hash of the container configuration, so two projects on the same image would
# share one database and Flyway would reject the other one's migrations.
#
# Reused containers are deliberately not registered with Ryuk, so nothing
# reaps them -- `jails doctor` counts them.
testcontainers.reuse.enable=true
";

#[cfg(test)]
mod tests {
    use super::*;

    /// A capability's property block is prose *and* settings, and only the
    /// settings are keys.
    ///
    /// `add cors` writes "Exact browser origins; never use `*` together with
    /// credentials." above the line it explains. Reading it as a required
    /// property made `doctor` demand a key whose name was an English
    /// sentence, on a project where the capability was installed correctly --
    /// and no amount of `jails sync` could satisfy it, because nothing would
    /// ever write that key.
    #[test]
    fn a_property_comment_is_not_a_required_key() {
        let root = jails_support::scratch::ScratchDir::in_temp("jails-doctor-cors")
            .unwrap()
            .keep();
        std::fs::create_dir_all(root.join("src/main/java/com/example/demo")).unwrap();
        std::fs::write(
            root.join("src/main/java/com/example/demo/App.java"),
            "package com.example.demo;\npublic final class App {}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("pom.xml"),
            "<project><parent><groupId>org.springframework.boot</groupId>\
             <artifactId>spring-boot-starter-parent</artifactId><version>4.1.0</version>\
             </parent></project>",
        )
        .unwrap();
        std::fs::write(
            root.join("jails.toml"),
            "[project]\ncapabilities = [\"cors\"]\n",
        )
        .unwrap();

        let project = Project::inspect(&root).unwrap();
        let checks = capability_drift_checks(&project);

        for check in &checks {
            assert!(
                !check.detail.contains("property #"),
                "a comment was read as a required key: {}",
                check.detail
            );
            assert!(
                !check.detail.contains("never use"),
                "a comment was read as a required key: {}",
                check.detail
            );
        }
    }

    /// The drift class that had no test because it had no shared value.
    ///
    /// `add` knew what `json` installs; `doctor` could not ask. So a project
    /// whose `jails.toml` still lists a capability while its dependency or its
    /// generated file has gone reported nothing at all. Now `doctor` re-plans
    /// the capability through `add::plan_for` and diffs.
    #[test]
    fn a_recorded_capability_missing_its_own_output_is_reported() {
        let root = jails_support::scratch::ScratchDir::in_temp("jails-doctor-drift")
            .unwrap()
            .keep();
        std::fs::create_dir_all(root.join("src/main/java/com/example/demo")).unwrap();
        std::fs::write(
            root.join("src/main/java/com/example/demo/App.java"),
            "package com.example.demo;\npublic final class App {}\n",
        )
        .unwrap();
        std::fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        // Recorded as installed, with none of it actually present.
        std::fs::write(
            root.join("jails.toml"),
            "[project]\ncapabilities = [\"json\"]\n",
        )
        .unwrap();

        let project = Project::inspect(&root).unwrap();
        let checks = capability_drift_checks(&project);
        assert_eq!(checks.len(), 1, "{}", checks.len());
        assert_eq!(checks[0].status, Status::Fail);
        assert!(checks[0].detail.contains("missing"), "{}", checks[0].detail);
        assert_eq!(checks[0].fix, "jails sync");

        std::fs::remove_dir_all(&root).ok();
    }

    /// A project that records nothing has nothing to reconcile, and saying so
    /// is not a failure -- `doctor` must stay usable on a project jails did
    /// not create.
    #[test]
    fn recording_no_capabilities_is_reported_as_nothing_to_do() {
        let root = jails_support::scratch::ScratchDir::in_temp("jails-doctor-nodrift")
            .unwrap()
            .keep();
        let project = project_with_pom(&root, "<project></project>");
        let checks = capability_drift_checks(&project);
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].status, Status::Skip);
        std::fs::remove_dir_all(&root).ok();
    }

    /// A `Project` over a scratch root whose pom says exactly this.
    ///
    /// The checks take a resolved project now, so a test states the pom by
    /// *writing* it rather than by passing a second argument beside the
    /// directory -- which is the pairing that could go inconsistent, and the
    /// reason these two parameters became one value.
    fn project_with_pom(root: &Path, pom: &str) -> Project {
        std::fs::create_dir_all(root).unwrap();
        std::fs::write(root.join("pom.xml"), pom).unwrap();
        Project::inspect(root).unwrap()
    }

    #[test]
    fn cors_checks_flag_mvc_takeover_and_a_global_mapping_without_origins() {
        let root = jails_support::scratch::ScratchDir::in_temp("jails-doctor-cors")
            .unwrap()
            .keep();
        let source = root.join("src/main/java/com/example/WebConfig.java");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(
            &source,
            "@EnableWebMvc\nclass WebConfig { void x() { registry.addMapping(\"/**\"); } }",
        )
        .unwrap();
        let checks = cors_checks(&project_with_pom(
            &root,
            "<artifactId>spring-boot-starter-webmvc</artifactId>",
        ));
        assert_eq!(checks.len(), 2);
        assert!(checks.iter().all(|check| check.status == Status::Warn));
        assert!(checks.iter().all(|check| !check.fix.is_empty()));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn virtual_thread_checks_find_scheduler_exit_and_jfr_pinning_traps() {
        let root = jails_support::scratch::ScratchDir::in_temp("jails-doctor-virtual")
            .unwrap()
            .keep();
        let resources = root.join("src/main/resources");
        let source = root.join("src/main/java/com/example/Jobs.java");
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(
            resources.join("application.properties"),
            "spring.threads.virtual.enabled=true\n",
        )
        .unwrap();
        std::fs::write(
            &source,
            "class Jobs { @Scheduled void run() { synchronized (this) {} } }",
        )
        .unwrap();

        let checks = virtual_thread_checks(&root);
        assert_eq!(checks.len(), 2);
        assert!(checks.iter().all(|check| check.status == Status::Warn));
        assert!(checks[0].fix.contains("spring.main.keep-alive=true"));
        assert!(checks[1].detail.contains("jdk.VirtualThreadPinned"));
        assert!(!checks[1].detail.contains("tracePinnedThreads"));
        let _ = std::fs::remove_dir_all(root);
    }

    /// Every way the editor-save loop breaks is silent, so each is pinned.
    ///
    /// `plan.md` §19.5 is answered by measurement: jdt.ls writes into the
    /// project's own `target/classes` with no Maven run, so the loop exists
    /// and only its switches can be wrong.
    #[test]
    fn hot_reload_checks_catch_every_silent_way_the_save_loop_dies() {
        let base = jails_support::scratch::ScratchDir::in_temp("jails-doctor-hot")
            .unwrap()
            .keep();
        let boot = "<project><parent><groupId>org.springframework.boot</groupId>\
             <artifactId>spring-boot-starter-parent</artifactId></parent>{deps}</project>";
        let devtools = "<dependencies><dependency>\
             <groupId>org.springframework.boot</groupId>\
             <artifactId>spring-boot-devtools</artifactId></dependency></dependencies>";

        // A plain Maven project has no devtools loop to be wrong about.
        let plain = base.join("plain");
        assert!(hot_reload_checks(&project_with_pom(&plain, "<project></project>")).is_empty());

        // Boot without devtools: the editor compiles, nothing notices.
        let bare = base.join("bare");
        let checks = hot_reload_checks(&project_with_pom(&bare, &boot.replace("{deps}", "")));
        assert_eq!(checks[0].status, Status::Warn);
        // The fix has to be runnable *here*. It used to name the coordinate
        // and leave the reader to work out the command, and `run --watch`'s
        // sibling message told them to `jails new` -- which is a command that
        // exists, so the oracle over `fix:` lines passed it, and which creates
        // a different project rather than repairing this one.
        assert!(
            checks[0].fix.starts_with("jails add dependency"),
            "{}",
            checks[0].fix
        );
        assert!(checks[0].fix.contains("spring-boot-devtools"));
        assert!(
            !checks[0].fix.contains("jails new"),
            "a fix for an existing project must not tell the reader to create one"
        );

        // Restarts switched off is the one that deserves a Fail: the
        // dependency is present, so everything *looks* wired up.
        let off = base.join("off");
        let project = project_with_pom(&off, &boot.replace("{deps}", devtools));
        write_properties(&off, "spring.devtools.restart.enabled=false\n");
        let checks = hot_reload_checks(&project);
        assert_eq!(checks[0].status, Status::Fail);
        assert!(!checks[0].fix.is_empty());

        // A trigger file means a saved class is seen and deliberately ignored.
        let trigger = base.join("trigger");
        let project = project_with_pom(&trigger, &boot.replace("{deps}", devtools));
        write_properties(&trigger, "spring.devtools.restart.trigger-file=.reload\n");
        let checks = hot_reload_checks(&project);
        assert_eq!(checks[0].status, Status::Warn);
        assert!(checks[0].detail.contains(".reload"));

        // Devtools untuned still works -- it just waits up to 1.4s per save.
        let slow = base.join("slow");
        let project = project_with_pom(&slow, &boot.replace("{deps}", devtools));
        let checks = hot_reload_checks(&project);
        assert_eq!(checks[0].status, Status::Warn);
        assert!(checks[0].detail.contains("1.4s"));

        // What `jails new` produces, and the only arrangement that is Ok.
        let tuned = base.join("tuned");
        let project = project_with_pom(&tuned, &boot.replace("{deps}", devtools));
        let meta = tuned.join("src/main/resources/META-INF");
        std::fs::create_dir_all(&meta).unwrap();
        std::fs::write(
            meta.join("spring-devtools.properties"),
            "defaults.spring.devtools.restart.poll-interval=200ms\n",
        )
        .unwrap();
        let checks = hot_reload_checks(&project);
        assert_eq!(checks[0].status, Status::Ok);
        assert!(checks[0].detail.contains("target/classes"));

        std::fs::remove_dir_all(&base).ok();
    }

    fn write_properties(root: &Path, body: &str) {
        let resources = root.join("src/main/resources");
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::write(resources.join("application.properties"), body).unwrap();
    }

    #[test]
    fn java_version_output_yields_a_major_number() {
        let modern = "openjdk version \"26.0.1\" 2026-01-20\nOpenJDK Runtime Environment";
        assert_eq!(parse_java_major(modern), Some(26));
        let legacy = "java version \"1.8.0_401\"";
        assert_eq!(parse_java_major(legacy), Some(8));
        let ea = "openjdk version \"27-ea\" 2026-09-15";
        assert_eq!(parse_java_major(ea), Some(27));
        assert_eq!(parse_java_major("no version here"), None);
    }

    #[test]
    fn declared_services_reads_top_level_names_only() {
        let yaml = "\
services:
  postgres:
    image: postgres:17-alpine
    ports:
      - \"5432:5432\"
  kafka:
    image: apache/kafka:4.1.0
volumes:
  postgres-data:
";
        assert_eq!(declared_services(yaml), vec!["postgres", "kafka"]);
    }

    #[test]
    fn declared_services_skips_marker_comments() {
        let yaml = "services:\n  # jails:db\n  postgres:\n    image: x\n  # /jails:db\n";
        assert_eq!(declared_services(yaml), vec!["postgres"]);
    }

    #[test]
    fn a_jdk_older_than_the_target_release_fails() {
        // release_level reads the pom; the JDK half is checked separately
        // because it depends on the machine.
        let old = "<maven.compiler.release>27</maven.compiler.release>";
        assert_eq!(pom::release_level(old), Some(27));
    }

    /// A Maven project holding exactly this pom, for the checks that read
    /// dependencies through the resolved project rather than off raw text.
    fn maven_project(pom_body: &str) -> (jails_support::scratch::ScratchDir, Project) {
        let scratch = jails_support::scratch::ScratchDir::in_temp("jails-doctor-pom").unwrap();
        jails_support::apply::put(
            scratch.path().join("pom.xml"),
            format!("<project><dependencies>{pom_body}</dependencies></project>"),
        )
        .unwrap();
        let project = Project::inspect(scratch.path()).unwrap();
        (scratch, project)
    }

    #[test]
    fn jackson_databind_without_jsr310_is_a_failure() {
        let pom = "<dependency><groupId>com.fasterxml.jackson.core</groupId>\
                   <artifactId>jackson-databind</artifactId></dependency>";
        let (_scratch, project) = maven_project(pom);
        assert_eq!(jackson_check(&project).status, Status::Fail);
    }

    /// A working Jackson 2 pair still works -- it is just a version behind,
    /// so it warns rather than failing.
    #[test]
    fn both_jackson_2_artifacts_warn_rather_than_fail() {
        let pom = "<dependency><groupId>com.fasterxml.jackson.core</groupId>\
                   <artifactId>jackson-databind</artifactId></dependency>\
                   <dependency><groupId>com.fasterxml.jackson.datatype</groupId>\
                   <artifactId>jackson-datatype-jsr310</artifactId></dependency>";
        let (_scratch, project) = maven_project(pom);
        assert_eq!(jackson_check(&project).status, Status::Warn);
    }

    #[test]
    fn management_checks_flag_public_dangerous_and_dependency_liveness_endpoints() {
        let root = jails_support::scratch::ScratchDir::in_temp("jails-doctor-management")
            .unwrap()
            .keep();
        let resources = root.join("src/main/resources");
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::write(
            resources.join("application.properties"),
            "management.endpoints.web.exposure.include=health,env,heapdump\n\
             management.endpoint.health.group.liveness.include=ping,db\n",
        )
        .unwrap();
        let pom = "<dependency><groupId>org.springframework.boot</groupId>\
                   <artifactId>spring-boot-starter-actuator</artifactId></dependency>";
        let checks = management_checks(&project_with_pom(&root, pom));
        assert_eq!(checks.len(), 3);
        assert!(checks.iter().all(|check| check.status == Status::Warn));
        assert!(checks[1].detail.contains("env"), "{}", checks[1].detail);
        assert!(
            checks[2].detail.contains("Kubernetes"),
            "{}",
            checks[2].detail
        );
    }

    #[test]
    fn generated_management_defaults_are_all_clear() {
        let root = jails_support::scratch::ScratchDir::in_temp("jails-doctor-management-clear")
            .unwrap()
            .keep();
        let resources = root.join("src/main/resources");
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::write(
            resources.join("application.properties"),
            "management.server.port=8081\n\
             management.endpoints.web.exposure.include=health,info,prometheus,threaddump\n\
             management.endpoint.health.group.liveness.include=ping\n",
        )
        .unwrap();
        let pom = "<dependency><groupId>org.springframework.boot</groupId>\
                   <artifactId>spring-boot-starter-actuator</artifactId></dependency>";
        let checks = management_checks(&project_with_pom(&root, pom));
        assert!(checks.iter().all(|check| check.status == Status::Ok));
    }

    /// The failure that loses data without an error: a DataSource exists, but
    /// the bean serving every request is a HashMap that empties on restart.
    #[test]
    fn an_in_memory_repository_bean_beside_a_datasource_is_a_failure() {
        let root = jails_support::scratch::ScratchDir::in_temp("jails-inmem-check")
            .unwrap()
            .keep();
        let pkg = root.join("src/main/java/com/example/demo/adapters");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(
            pkg.join("InMemoryNoteRepository.java"),
            "package com.example.demo.adapters;\n\n@Repository\npublic class InMemoryNoteRepository {}\n",
        )
        .unwrap();

        let pom = "<dependency><groupId>org.springframework.boot</groupId>\
                   <artifactId>spring-boot-starter-jdbc</artifactId></dependency>";
        let check = in_memory_adapter_check(&project_with_pom(&root, pom)).expect("should report");
        assert_eq!(check.status, Status::Fail);
        assert!(
            check.detail.contains("InMemoryNoteRepository"),
            "{}",
            check.detail
        );

        // Without a DataSource it is the correct design, not a problem.
        assert!(in_memory_adapter_check(&project_with_pom(&root, "<project/>")).is_none());

        // And once the annotation moves, there is nothing to report.
        std::fs::write(
            pkg.join("InMemoryNoteRepository.java"),
            "package com.example.demo.adapters;\n\npublic class InMemoryNoteRepository {}\n",
        )
        .unwrap();
        assert!(in_memory_adapter_check(&project_with_pom(&root, pom)).is_none());

        std::fs::remove_dir_all(&root).ok();
    }

    /// The banner real Compose v2 prints, even when it is a CLI plugin
    /// driving podman -- which is the setup that works.
    #[test]
    fn a_real_compose_v2_banner_passes_even_under_podman() {
        let banner = ">>>> Executing external compose provider \
                      \"/home/me/.docker/cli-plugins/docker-compose\" <<<<\n\
                      Docker Compose version v5.5.0\n";
        let check = classify_compose_provider(banner);
        assert_eq!(check.status, Status::Ok);
        assert!(check.detail.contains("v5.5.0"), "{}", check.detail);
    }

    /// The failure this check exists for: the app dies during startup, before
    /// any of its own code runs, and the message names neither cause nor fix.
    #[test]
    fn a_podman_compose_provider_fails_with_the_plugin_fix() {
        let check = classify_compose_provider("podman-compose version 1.6.0\n");
        assert_eq!(check.status, Status::Fail);
        assert!(check.detail.contains("podman-compose"), "{}", check.detail);
        assert!(
            check.fix.contains("cli-plugins"),
            "the fix must be the one that leaves nothing broken: {:?}",
            check.fix
        );
    }

    #[test]
    fn jackson_3_alone_is_the_happy_path() {
        let pom = "<dependency><groupId>tools.jackson.core</groupId>\
                   <artifactId>jackson-databind</artifactId></dependency>";
        let (_scratch, project) = maven_project(pom);
        assert_eq!(jackson_check(&project).status, Status::Ok);
    }

    /// The failure nothing else reports: two majors coexist quietly because
    /// their packages differ, and half the code ends up on a mapper nobody
    /// configured.
    #[test]
    fn two_jackson_majors_at_once_is_a_failure() {
        let pom = "<dependency><groupId>tools.jackson.core</groupId>\
                   <artifactId>jackson-databind</artifactId></dependency>\
                   <dependency><groupId>com.fasterxml.jackson.core</groupId>\
                   <artifactId>jackson-databind</artifactId></dependency>";
        let (_scratch, project) = maven_project(pom);
        let check = jackson_check(&project);
        assert_eq!(check.status, Status::Fail);
        assert!(
            check.detail.contains("both Jackson majors"),
            "{}",
            check.detail
        );
    }

    #[test]
    fn testcontainers_is_detected_under_both_module_naming_schemes() {
        let v1 = "<groupId>org.testcontainers</groupId><artifactId>postgresql</artifactId>";
        let v2 = "<groupId>org.testcontainers</groupId><artifactId>testcontainers-postgresql</artifactId>";
        assert_ne!(testcontainers_check(v1).status, Status::Skip);
        assert_ne!(testcontainers_check(v2).status, Status::Skip);
        assert_eq!(testcontainers_check("<project/>").status, Status::Skip);
    }

    #[test]
    fn a_service_is_matched_inside_the_compose_container_name() {
        let containers = vec![
            "rewards_postgres_1".to_string(),
            "other-kafka-1".to_string(),
        ];
        assert!(service_is_running("postgres", &containers));
        assert!(service_is_running("kafka", &containers));
        assert!(!service_is_running("redis", &containers));
        // A service name that is only a substring of a segment must not match.
        assert!(!service_is_running("post", &containers));
    }

    #[test]
    fn runtime_flag_translates_the_compose_service_name() {
        assert_eq!(runtime_flag("postgres"), "db");
        assert_eq!(runtime_flag("kafka"), "kafka");
    }
}
