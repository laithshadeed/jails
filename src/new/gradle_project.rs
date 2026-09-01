//! Creating a Groovy Gradle project, which `new` could not do until now.
//!
//! ## Why this is written rather than fetched
//!
//! The Maven half of `new` wraps start.spring.io, and `--offline` exists as
//! the fallback. This half inverts that: jails writes every file itself and
//! never contacts Initializr, for a reason that is not convenience.
//!
//! Initializr serves only the Spring Boot lines that are currently supported.
//! It has no answer at all for a project pinned to 2.7.18, and refusing to
//! create the shape somebody actually has to work in is the failure this
//! module exists to remove -- the same argument `build.rs`'s header records
//! for reading `build.gradle` in the first place. Writing the file also makes
//! `--pretend` honest here, which it cannot be on the Maven path.
//!
//! ## Two shapes, and the version decides which
//!
//! Boot's Gradle plugin is applied two ways, and both are in the plugin's own
//! documentation (`deps/spring-boot/build-plugin/spring-boot-gradle-plugin/
//! src/docs/.../managing-dependencies.adoc`):
//!
//! - **`buildscript {}` + `apply plugin:`** -- the classpath form. It is the
//!   only one that works for a Boot 2.x pin, because `plugins { id ... version
//!   ... }` resolves through the Gradle plugin portal against a marker
//!   artifact, and the legacy builds people still run predate that being the
//!   normal spelling.
//! - **`plugins {}` + Gradle's native bom support** -- for Boot 3 and later.
//!
//! Both shapes apply `io.spring.dependency-management` **by id with no
//! version**, which is the form Boot's own `configure-bom.gradle` example uses:
//! the plugin arrives on the buildscript classpath as a dependency of the Boot
//! plugin, so there is no third version number for jails to pin and keep in
//! step with two others -- and no number it would be guessing at, since no
//! checkout under `deps/` states one.
//!
//! Gradle's native `platform(...SpringBootPlugin.BOM_COORDINATES)` is the other
//! option Boot documents, and it was tried first. It is rejected for a reason
//! that only shows up once jails has to *operate on* what it wrote: the
//! coordinate is an expression rather than a string literal, and `gradle.rs`
//! answers `None` -- "this file says something I do not understand" -- for
//! exactly that construct. That is the reader working correctly, and it made
//! every `jails add` and `jails generate` refuse on a project `jails new` had
//! produced thirty seconds earlier. A generator that writes a build its own
//! tool cannot read is worse than one that writes an older spelling.
//!
//! ## The wrapper, and the one file jails cannot write
//!
//! `gradlew`, `gradlew.bat` and `gradle-wrapper.properties` are text and ship
//! as templates. `gradle-wrapper.jar` is a binary, and there is no standalone
//! published coordinate for it: Maven Central has no `org.gradle:gradle-
//! wrapper`, and `services.gradle.org` serves only whole distributions. So it
//! is fetched from Gradle's own repository at a tag, and its absence is
//! **reported rather than papered over** -- a `gradlew` next to no jar fails
//! with `Could not find or load main class org.gradle.wrapper.GradleWrapperMain`,
//! which says nothing about what is missing.
//!
//! When the jar cannot be had, the scripts are not written either. That is the
//! deliberate half: `run::gradlew::binary` falls back to `gradle` on PATH when
//! there is no `gradlew`, so no wrapper is a working project and a broken
//! wrapper is not.

use jails_support::Result;
use std::path::Path;
use std::process::Command;

/// The Gradle-only flags, refused rather than ignored when `--gradle` is not
/// passed.
///
/// A flag that silently does nothing is the failure `run.rs` records for
/// `jails test --fast` on a Gradle build: it looks like it worked. `--boot`
/// especially, since the Maven path takes its Boot version from
/// start.spring.io and cannot honour a pin at all.
pub(super) fn require_gradle(request: &super::Request<'_>) -> Result<()> {
    let stray = [
        ("--boot", request.boot.is_some()),
        ("--gradle-version", request.gradle_version.is_some()),
        ("--jar-name", request.jar_name.is_some()),
        ("--jar-version", request.jar_version.is_some()),
    ]
    .into_iter()
    .filter(|(_, given)| *given)
    .map(|(flag, _)| flag)
    .collect::<Vec<_>>();
    if stray.is_empty() || request.gradle {
        return Ok(());
    }
    Err(format!(
        "{} only applies to a Gradle project, and this one is Maven.\n       \
         The Maven path takes its Spring Boot version from start.spring.io, so a pin here \
         would be a number jails prints and nothing reads.\n       \
         fix: add `--gradle`, or drop {}.",
        stray.join(" and "),
        stray.join(" and ")
    )
    .into())
}

/// Which `build.gradle` shape a Boot version needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// `buildscript {}` + `apply plugin:`. Boot 2.x.
    Legacy,
    /// `plugins {}` + Gradle's native bom support. Boot 3 and later.
    Modern,
}

/// Everything the templates need, resolved once.
///
/// `root` is in here rather than beside it at four call sites: rung 1's gate
/// counts `root: &Path` parameters precisely because a fact travelling as a
/// primitive next to the value that should own it is how two answers to one
/// question appear in one run.
pub(super) struct Plan<'a> {
    pub root: &'a Path,
    pub name: &'a str,
    pub package: &'a str,
    pub group: &'a str,
    pub java: &'a str,
    pub boot: &'a str,
    pub gradle: &'a str,
    /// `bootJar { archiveBaseName }`. `None` leaves the block out, and Gradle
    /// names the jar after the project -- which is the answer whenever nobody
    /// has a reason for a different one.
    pub jar_name: Option<&'a str>,
    /// `bootJar { archiveVersion }`. Read only when `jar_name` is set, since
    /// there is no block to put it in otherwise.
    pub jar_version: Option<&'a str>,
    pub deps: &'a str,
}

/// The Boot major, or a refusal naming what was passed.
///
/// Parsed rather than pattern-matched because the major is a *decision input*
/// -- it picks the build file shape, the Gradle version and the starter names
/// -- and a version string jails cannot read is one where every one of those
/// three would be a guess.
pub(super) fn boot_major(boot: &str) -> Result<u32> {
    Ok(boot
        .split('.')
        .next()
        .and_then(|major| major.parse::<u32>().ok())
        .ok_or_else(|| {
            format!(
                "--boot {boot} is not a version jails can read.\n       \
                 fix: pass a version whose first segment is a number, e.g. `--boot 2.7.18` \
                 or `--boot {}`.",
                crate::pom::TARGET_BOOT
            )
        })?)
}

fn shape(major: u32) -> Shape {
    match major < 3 {
        true => Shape::Legacy,
        false => Shape::Modern,
    }
}

/// The Gradle distribution a Boot major can actually be built by.
///
/// Not a preference. `SpringBootPlugin.java:128` in `deps/spring-boot` throws
/// outright below Gradle 8.14, so a Boot 4 project pinned to 8.5 fails at
/// configuration time with a message about Gradle rather than about the pin.
/// 8.5 is what the Boot 2.7 project this was built against runs on today.
pub(super) fn default_gradle_version(major: u32) -> &'static str {
    match major < 3 {
        true => "8.5",
        false => "9.7.0",
    }
}

/// One Initializr dependency id, as the coordinate and configuration Gradle
/// wants.
struct Starter {
    coordinate: String,
    configuration: &'static str,
}

/// Initializr ids jails can resolve without asking Initializr.
///
/// Boot 4 renamed the servlet web starter from `spring-boot-starter-web` to
/// `spring-boot-starter-webmvc`, so the id alone does not determine the
/// artifact -- which is exactly why this takes the major rather than baking in
/// the one jails' own fixture uses. Emitting the Boot 4 name into a Boot 2.7
/// build resolves nothing and fails at dependency resolution, several steps
/// from the flag that caused it.
pub(super) fn starter(id: &str, major: u32) -> Result<(String, &'static str)> {
    let web = match major >= 4 {
        true => "spring-boot-starter-webmvc",
        false => "spring-boot-starter-web",
    };
    let boot = |artifact: &str| format!("org.springframework.boot:{artifact}");
    let found = match id {
        "web" => Starter {
            coordinate: boot(web),
            configuration: "implementation",
        },
        "validation" => Starter {
            coordinate: boot("spring-boot-starter-validation"),
            configuration: "implementation",
        },
        "jdbc" => Starter {
            coordinate: boot("spring-boot-starter-jdbc"),
            configuration: "implementation",
        },
        "data-jdbc" => Starter {
            coordinate: boot("spring-boot-starter-data-jdbc"),
            configuration: "implementation",
        },
        "actuator" => Starter {
            coordinate: boot("spring-boot-starter-actuator"),
            configuration: "implementation",
        },
        "security" => Starter {
            coordinate: boot("spring-boot-starter-security"),
            configuration: "implementation",
        },
        "data-jpa" => Starter {
            coordinate: boot("spring-boot-starter-data-jpa"),
            configuration: "implementation",
        },
        // Gradle's own configuration for it, not `implementation`:
        // `developmentOnly` keeps devtools out of the packaged jar, which is
        // what `<optional>true</optional>` buys on the Maven side.
        "devtools" => Starter {
            coordinate: boot("spring-boot-devtools"),
            configuration: "developmentOnly",
        },
        // The one id here that is not a Boot artifact. It is a driver: needed
        // at run time, never compiled against.
        "h2" => Starter {
            coordinate: "com.h2database:h2".to_string(),
            configuration: "runtimeOnly",
        },
        other => {
            return Err(format!(
                "jails does not know Initializr dependency `{other}` well enough to write it \
                 into a build file.\n       \
                 fix: use one of web, validation, jdbc, data-jdbc, actuator, security, \
                 data-jpa, devtools, h2 -- or create the project and add the rest with \
                 `jails add`."
            )
            .into());
        }
    };
    Ok((found.coordinate, found.configuration))
}

/// The `dependencies {}` body.
fn dependencies(deps: &str, major: u32) -> Result<String> {
    let mut out = String::new();
    for id in deps.split(',').map(str::trim).filter(|id| !id.is_empty()) {
        let (coordinate, configuration) = starter(id, major)?;
        out.push_str(&format!("    {configuration} '{coordinate}'\n"));
    }
    out.push_str("    testImplementation 'org.springframework.boot:spring-boot-starter-test'\n");
    Ok(out)
}

/// The `bootJar {}` block, or nothing.
///
/// Rendered here rather than branched on in the template, because the
/// templates substitute and do not decide -- the rule `template.rs` states as
/// "not a template engine".
fn jar_block(jar_name: Option<&str>, jar_version: Option<&str>) -> String {
    let Some(name) = jar_name else {
        return String::new();
    };
    let version = match jar_version {
        Some(version) => format!("    archiveVersion = '{version}'\n"),
        None => String::new(),
    };
    format!("\nbootJar {{\n    archiveBaseName = '{name}'\n{version}}}\n")
}

/// Render the build file for this plan.
pub(super) fn build_file(plan: &Plan<'_>) -> Result<String> {
    let major = boot_major(plan.boot)?;
    let body = dependencies(plan.deps, major)?;
    let jar = jar_block(plan.jar_name, plan.jar_version);
    Ok(match shape(major) {
        Shape::Legacy => crate::template::render(
            crate::template_here!("new/gradle/build_legacy.gradle"),
            &[
                ("boot", plan.boot),
                ("java", plan.java),
                ("jar", &jar),
                ("dependencies", &body),
            ],
        ),
        Shape::Modern => crate::template::render(
            crate::template_here!("new/gradle/build.gradle"),
            &[
                ("boot", plan.boot),
                ("group", plan.group),
                ("java", plan.java),
                ("jar", &jar),
                ("dependencies", &body),
            ],
        ),
    })
}

/// Every path this plan writes, in order, for `--pretend`.
pub(super) fn planned_paths(plan: &Plan<'_>, class: &str) -> Vec<std::path::PathBuf> {
    let root = plan.root;
    let source = root
        .join("src/main/java")
        .join(plan.package.replace('.', "/"));
    let tests = root
        .join("src/test/java")
        .join(plan.package.replace('.', "/"));
    vec![
        root.join("build.gradle"),
        root.join("settings.gradle"),
        root.join("gradle/wrapper/gradle-wrapper.properties"),
        root.join("gradlew"),
        root.join("gradlew.bat"),
        root.join("gradle/wrapper/gradle-wrapper.jar"),
        source.join(format!("{class}Application.java")),
        tests.join(format!("{class}ApplicationTests.java")),
        root.join("src/main/resources/application.properties"),
        root.join("mise.toml"),
        root.join("AGENTS.md"),
    ]
}

/// Write the build files. The Java sources are the caller's, since both `new`
/// paths write the same two classes from the same two templates.
pub(super) fn write_build(plan: &Plan<'_>, tree: &super::publish::Tree<'_>) -> Result<()> {
    let root = plan.root;
    tree.put_named_at(
        &root.join("build.gradle"),
        build_file(plan)?,
        "build.gradle",
    )?;
    tree.put_named_at(
        &root.join("settings.gradle"),
        crate::template::render(
            crate::template_here!("new/gradle/settings.gradle"),
            &[("name", plan.name)],
        ),
        "settings.gradle",
    )?;
    Ok(())
}

/// Write the wrapper, all of it or none of it.
///
/// Best-effort by design, and loud either way. The jar decides: with it, the
/// project is run by the Gradle version it pins; without it, the project is run
/// by whatever `gradle` is on PATH, which is worse but works. A `gradlew` with
/// no jar beside it is neither.
pub(super) fn write_wrapper(
    plan: &Plan<'_>,
    tree: &super::publish::Tree<'_>,
    offline: bool,
    debug: bool,
) -> Result<()> {
    let (root, gradle) = (plan.root, plan.gradle);
    tree.put_at(
        &root.join("gradle/wrapper/gradle-wrapper.properties"),
        crate::template::render(
            crate::template_here!("new/gradle/wrapper.properties"),
            &[("gradle", gradle)],
        ),
    )?;
    if offline {
        return skipped_wrapper(gradle, "`--offline` was passed");
    }
    match fetch_wrapper_jar(plan, tree, debug) {
        Ok(()) => {
            tree.put_executable_at(
                &root.join("gradlew"),
                crate::template_here!("new/gradle/gradlew.sh"),
            )?;
            tree.put_at(
                &root.join("gradlew.bat"),
                crate::template_here!("new/gradle/gradlew.bat"),
            )
        }
        Err(why) => skipped_wrapper(gradle, &why),
    }
}

/// Say what is missing and the one command that supplies it.
fn skipped_wrapper(gradle: &str, why: &str) -> Result<()> {
    eprintln!(
        "jails: warning: no Gradle wrapper written ({why}).\n       \
         The project is complete and builds with `gradle` on PATH; jails will use it.\n       \
         fix: run `gradle wrapper --gradle-version {gradle}` in the project to add one."
    );
    Ok(())
}

/// Gradle's tag for a distribution version.
///
/// Tags are always three segments (`v8.5.0`, `v8.14.3`) while a distribution is
/// routinely named with two (`gradle-8.5-bin.zip`). Appending `.0`
/// unconditionally is what makes `9.7.0` fetch from `v9.7.0.0`, which does not
/// exist -- and a 404 here is not a hard failure, so it would have degraded
/// silently into "no wrapper written" for every three-segment pin.
fn wrapper_tag(gradle: &str) -> String {
    match gradle.split('.').count() {
        2 => format!("{gradle}.0"),
        _ => gradle.to_string(),
    }
}

/// Fetch `gradle-wrapper.jar` from Gradle's own repository at its tag.
///
/// Gradle publishes no standalone artifact for this file -- probed against
/// Maven Central and `services.gradle.org`, both 404 -- so its own checkout is
/// the source, and it is the same file Gradle builds itself with. The bytes
/// therefore differ from what `gradle wrapper` generates locally; what has to
/// match is the class the scripts launch, `org.gradle.wrapper.GradleWrapperMain`,
/// and the wrapper's job is to read the properties written beside it and fetch
/// the distribution they name. It is not tied to the distribution's version.
fn fetch_wrapper_jar(plan: &Plan<'_>, tree: &super::publish::Tree<'_>, debug: bool) -> Result<()> {
    let (root, gradle) = (plan.root, plan.gradle);
    let path = root.join("gradle/wrapper/gradle-wrapper.jar");
    tree.ensure_directory_at(path.parent().unwrap_or(root))
        .map_err(|error| format!("failed to create the wrapper directory: {error}"))?;
    let url = format!(
        "https://raw.githubusercontent.com/gradle/gradle/v{}/gradle/wrapper/gradle-wrapper.jar",
        wrapper_tag(gradle)
    );
    let mut curl = Command::new("curl");
    curl.args(["-sfL", "-o"]).arg(&path).arg(&url);
    if debug {
        jails_support::debug_cmd(&curl);
    }
    let status = curl
        .status()
        .map_err(|error| format!("curl could not be run: {error}"))?;
    if !status.success() {
        // Left behind, `curl -o` has already created an empty or partial file,
        // and a zero-byte jar is the one shape that looks like a wrapper and
        // is not.
        let _ = tree.remove_at(&path);
        return Err(format!("{url} could not be fetched").into());
    }
    Ok(())
}

/// `jails new --gradle`, end to end.
///
/// Shares the Maven offline path's two Java templates and every finishing step
/// that is not about a POM. What it does *not* share is the enforcer plugin --
/// there is no Gradle counterpart, and the release is already stated in the
/// build file the same call wrote -- and `verify_requested_deps`, which exists
/// to catch Initializr silently dropping an id. Nothing is dropped silently
/// here: `starter` refuses an id it does not know, by name.
pub(super) fn create(request: &super::Request<'_>, deps: &str, boot: &str) -> Result<()> {
    let name = request.name;
    let java = request.java;
    let release = java
        .parse::<u32>()
        .map_err(|_| format!("--java must be a release number, got `{java}`"))?;
    if release < crate::pom::MIN_RELEASE {
        return Err(format!(
            "--java {java} is below Java {}, which jails' generated code needs",
            crate::pom::MIN_RELEASE
        )
        .into());
    }
    let major = boot_major(boot)?;
    let gradle = request
        .gradle_version
        .unwrap_or_else(|| default_gradle_version(major));
    // jails picks both of these numbers, so it must not pick a pair it has
    // seen fail. `--boot 2.7.18` pins Gradle 8.5 (Boot 2 does not run on 9.x)
    // while `--java` still defaults to the current release, and 8.5 dies on
    // JDK 26 before it reads the build script -- so `jails new --gradle
    // --boot 2.7.18` wrote a project that could not be built at all, and
    // `doctor` reported `ok jdk java 26 on PATH, project targets 26` over it.
    //
    // Only a *measured* failure refuses. A pairing jails has not run is
    // `Unknown` and passes, because refusing on a guess would block a reader
    // who pinned `--gradle-version` themselves and knows better.
    if jails_project::gradle::launches_on(gradle, release) == jails_project::gradle::Launches::No {
        let known = jails_project::gradle::highest_measured_release(gradle);
        return Err(format!(
            "Gradle {gradle} does not run on JDK {release}: it fails in its own build script \
             with\n       `Unsupported class file major version {major_class}` before reading \
             this project.\n       Boot {boot} pins Gradle {gradle}, because Boot 2 does not run \
             on Gradle 9.x.\n       fix: {fix}",
            major_class = release + 44,
            fix = match known {
                Some(release) => format!(
                    "`--java {release}`, which is the pairing jails has run, or a `--boot` that \
                     takes\n            a newer Gradle."
                ),
                None => "pass `--java <release>` with a release this Gradle runs on, or a \
                         `--boot` that takes a newer Gradle."
                    .to_string(),
            }
        )
        .into());
    }
    let package = super::resolved_package(name, request.group, request.package);
    let group = super::group_of(request.group, &package);
    let class = super::application_class(name);
    // Two literals rather than one closure: `Plan` carries a single lifetime,
    // and the root differs between the preview (the name the reader typed) and
    // the run (the reserved directory, which does not exist yet at this point).
    macro_rules! plan_at {
        ($root:expr) => {
            Plan {
                root: $root,
                name,
                package: &package,
                group: &group,
                java,
                boot,
                gradle,
                jar_name: request.jar_name,
                jar_version: request.jar_version,
                deps,
            }
        };
    }
    // Rendered before anything is reserved, so an unknown dependency id or an
    // unreadable version fails with no directory to clean up.
    let named = Path::new(name);
    let build_file = build_file(&plan_at!(named))?;

    if request.pretend {
        for path in planned_paths(&plan_at!(named), &class) {
            println!("would create {}", path.display());
        }
        if request.git {
            println!("would run git init in ./{name}");
        }
        println!();
        println!(
            "--pretend: nothing was written. (Gradle {gradle}, Spring Boot {boot}, Java {java})"
        );
        return super::previewed(request.app);
    }

    let publication = super::publish::Publication::reserve(named)?;
    // Copied out rather than borrowed: `publication.publish()` consumes the
    // guard at the end of this function, and a `Plan` holding a reference into
    // it could not outlive the writes it describes.
    let tree = publication.tree();
    let reserved = tree.root().to_path_buf();
    let plan = plan_at!(&reserved);
    let source = tree.join("src/main/java").join(package.replace('.', "/"));
    let tests = tree.join("src/test/java").join(package.replace('.', "/"));
    tree.ensure_directory_at(&source)
        .map_err(|error| format!("failed to create {}: {error}", source.display()))?;
    tree.ensure_directory_at(&tests)
        .map_err(|error| format!("failed to create {}: {error}", tests.display()))?;

    tree.put_named("build.gradle", build_file, "build.gradle")?;
    write_build(&plan, &tree)?;
    write_wrapper(&plan, &tree, request.offline, request.debug)?;

    crate::generate::write_new_file(
        tree,
        &source.join(format!("{class}Application.java")),
        &crate::template::render(
            crate::template_here!("new/offline_application.java"),
            &[("package", &package), ("class", &class)],
        ),
    )?;
    crate::generate::write_new_file(
        tree,
        &tests.join(format!("{class}ApplicationTests.java")),
        &crate::template::render(
            crate::template_here!("new/offline_application_test.java"),
            &[("package", &package), ("class", &class)],
        ),
    )?;
    super::write_fixtures_dir(&tree)?;
    // **The same seeding the Maven path does, for the same reason.** A Gradle
    // project that only got `write_default_properties` was not canonical at
    // all, and its six defaults sat in `application.properties` as reader-owned
    // bytes -- so the first `jails add db`, which declares `server.shutdown`
    // too, refused over a key `jails new` had written seconds earlier. As `prop`
    // declarations the compiler owns them and writes the file itself.
    super::seed::seed_canonical_model(
        &tree,
        request.app,
        super::spring::seed_model(name, &package, java, major, "gradle"),
    )?;
    super::write_devtools_defaults(&tree)?;
    add_jspecify_to_gradle(&plan, &tree)?;
    super::write_mise(&tree, java)?;
    super::write_agents(&tree, java)?;
    if request.git {
        tree.put(".gitignore", GRADLE_GITIGNORE)?;
        super::git_init(&tree, request.debug);
    }
    let applied = super::seed(&publication, request.app, request.no_start, request.debug)?;

    publication.publish()?;
    println!("Created ./{name} (Gradle {gradle}, Spring Boot {boot}, Java {java}, deps: {deps})");
    super::reported(applied)
}

/// JSpecify, spliced into the build file rather than the POM.
///
/// Same reason as the Maven half: every generator writes a null-marked
/// `package-info.java`, and annotating a package whose `@NullMarked` cannot
/// resolve hands the reader a compile error for a file they did not ask for.
/// Boot's dependency management does not pin JSpecify in either build, hence
/// the version in both.
///
/// `add_dependency_ref` returning `None` means the file already declares it or
/// says something `gradle::` will not guess at -- both of which are "leave it
/// alone", which is why this is not an error.
fn add_jspecify_to_gradle(plan: &Plan<'_>, tree: &super::publish::Tree<'_>) -> Result<()> {
    let path = plan.root.join(jails_project::gradle::FILE);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let declaration = crate::pom::DependencyRef {
        group_id: "org.jspecify",
        artifact_id: "jspecify",
        version: Some("1.0.0"),
        scope: None,
        optional: false,
    };
    if let Some(updated) = jails_project::gradle::add_dependency_ref(&text, declaration)? {
        tree.put_named_at(&path, updated, jails_project::gradle::FILE)?;
    }
    Ok(())
}

/// Gradle puts its outputs in `build/` and its caches in `.gradle/`, and the
/// wrapper jar has to be *un*-ignored: it is the one binary a Gradle project
/// commits on purpose, and a checkout missing it cannot run `./gradlew`.
const GRADLE_GITIGNORE: &str = include_str!("../../templates/new/gradle/gitignore.txt");

#[cfg(test)]
mod tests {
    use super::*;

    fn plan<'a>(boot: &'a str, deps: &'a str, jar: Option<&'a str>) -> Plan<'a> {
        Plan {
            root: Path::new("spring"),
            name: "spring",
            package: "com.intercom.spring",
            group: "com.intercom",
            java: "21",
            boot,
            gradle: "8.5",
            jar_name: jar,
            jar_version: jar.map(|_| "0.1.0"),
            deps,
        }
    }

    /// The shape the project this was built against actually has.
    #[test]
    fn a_boot_2_pin_gets_the_buildscript_form() {
        let rendered =
            build_file(&plan("2.7.18", "web,data-jdbc,h2", Some("gs-rest-service"))).unwrap();
        assert!(rendered.contains("buildscript {"), "{rendered}");
        assert!(
            rendered.contains(
                "classpath(\"org.springframework.boot:spring-boot-gradle-plugin:2.7.18\")"
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains("apply plugin: 'org.springframework.boot'"),
            "{rendered}"
        );
        assert!(
            rendered.contains("archiveBaseName = 'gs-rest-service'"),
            "{rendered}"
        );
        assert!(rendered.contains("sourceCompatibility = 21"), "{rendered}");
        assert!(rendered.contains("targetCompatibility = 21"), "{rendered}");
        // Boot 2 predates the rename, and the Boot 4 name resolves to nothing
        // here.
        assert!(
            rendered.contains("implementation 'org.springframework.boot:spring-boot-starter-web'"),
            "{rendered}"
        );
        assert!(
            rendered.contains("runtimeOnly 'com.h2database:h2'"),
            "{rendered}"
        );
        assert!(!rendered.contains("plugins {"), "{rendered}");
    }

    #[test]
    fn a_current_pin_gets_the_plugins_block_and_a_versionless_dependency_management() {
        let rendered = build_file(&plan("4.1.0", "web", None)).unwrap();
        assert!(
            rendered.contains("id 'org.springframework.boot' version '4.1.0'"),
            "{rendered}"
        );
        // Applied with no version, and as a literal-coordinate build that
        // `gradle::declared` can read back.
        assert!(
            rendered.contains("apply plugin: 'io.spring.dependency-management'"),
            "{rendered}"
        );
        assert!(!rendered.contains("BOM_COORDINATES"), "{rendered}");
        assert!(
            rendered.contains("JavaLanguageVersion.of(21)"),
            "{rendered}"
        );
        // Boot 4's name for the servlet web starter.
        assert!(
            rendered.contains("spring-boot-starter-webmvc"),
            "{rendered}"
        );
        // Applied, but never with a version: the number is the one thing
        // jails has no source for.
        assert!(
            !rendered.contains("id 'io.spring.dependency-management'"),
            "{rendered}"
        );
    }

    /// Nobody asked for a jar name, so there is no block to disagree with the
    /// project name later.
    #[test]
    fn no_jar_name_writes_no_bootjar_block() {
        let rendered = build_file(&plan("4.1.0", "web", None)).unwrap();
        assert!(!rendered.contains("bootJar"), "{rendered}");
    }

    #[test]
    fn devtools_is_development_only_so_it_stays_out_of_the_jar() {
        let rendered = build_file(&plan("4.1.0", "web,devtools", None)).unwrap();
        assert!(
            rendered.contains("developmentOnly 'org.springframework.boot:spring-boot-devtools'"),
            "{rendered}"
        );
    }

    /// The refusal names what is known, so the next attempt succeeds.
    #[test]
    fn an_unknown_dependency_id_is_refused_by_name() {
        let error = build_file(&plan("4.1.0", "web,quartz", None)).unwrap_err();
        assert!(error.contains("`quartz`"), "{error}");
        assert!(error.contains("jails add"), "{error}");
    }

    #[test]
    fn a_version_that_cannot_be_read_is_refused_rather_than_defaulted() {
        let error = boot_major("latest").unwrap_err();
        assert!(error.contains("--boot latest"), "{error}");
    }

    /// Below 8.14 the Boot 4 plugin throws at configuration time, so the
    /// default cannot be one number for both.
    #[test]
    fn the_gradle_default_follows_what_the_boot_plugin_demands() {
        assert_eq!(default_gradle_version(2), "8.5");
        assert_eq!(default_gradle_version(4), "9.7.0");
    }
}
