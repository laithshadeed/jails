//! `jails new` — a Spring Boot project, from start.spring.io or from here.
//!
//! Two paths to one shape. The online one asks start.spring.io for a starter
//! zip; `--offline` writes the same project from templates in this binary, so a
//! machine with no network still gets one. Everything after the unpack is
//! shared and lives here too, because it is all *finishing a Spring project*:
//! the JSpecify dependency, the devtools defaults, the properties file, the
//! release level when Initializr would not serve the one that was asked for.
//!
//! `pending.md` §8.1's split. This is the half that knows what Spring is;
//! [`super::plain`] is the half that does not.

use super::plain::ensure_enforcer;
use super::seed::GITIGNORE;
use super::*;

pub fn new(request: Request<'_>) -> Result<()> {
    let (name, java, debug, pretend) = (request.name, request.java, request.debug, request.pretend);
    validate_project_name(name)?;
    if Path::new(name).exists() {
        return Err(jails_support::Failure::Told(publish::already_exists(
            Path::new(name),
        )));
    }
    gradle_project::require_gradle(&request)?;
    let (group, package, git, offline, app) = (
        request.group,
        request.package,
        request.git,
        request.offline,
        request.app,
    );
    let devtools = request.devtools;

    let deps_for_gradle = effective_deps(request.deps, devtools);
    if request.gradle {
        return gradle_project::create(
            &request,
            &deps_for_gradle,
            request.boot.unwrap_or(crate::pom::TARGET_BOOT),
        );
    }

    // Refused rather than ignored. The project `new` creates is whatever
    // start.spring.io returns, so the only honest preview would be to fetch
    // the zip -- and a `--pretend` that hits the network to tell you what it
    // would have done is not a preview. `new-cli` writes a file set jails
    // knows, so that one previews for real.
    if pretend && !offline {
        return Err(jails_support::Failure::Told(
            "`--pretend` is not supported for `new`: the project comes from start.spring.io, \
             so jails cannot say what is in it without downloading it.\n\n\
             fix: run `jails new-cli --pretend` to preview a project jails writes itself, or \
             run `jails new` and inspect the result."
                .to_string(),
        ));
    }

    let deps = deps_for_gradle.as_str();

    if offline {
        return new_offline(&request, deps);
    }

    let publication = publish::Publication::reserve(Path::new(name))?;
    let tree = publication.tree();
    download_starter(&publication, name, group, package, deps, java, debug)?;

    if initializr_java(java) != java {
        set_java_release(&tree, initializr_java(java), java)?;
    }
    write_fixtures_dir(&tree)?;
    finish_spring_project(&tree, deps)?;
    ensure_enforcer(&tree, java)?;
    write_mise(&tree, java)?;
    write_agents(&tree, java)?;

    // start.spring.io's zip already ships a .gitignore, so just init.
    if git {
        git_init(&tree, debug);
    }
    let applied = seed(&publication, app, request.no_start, debug)?;

    publication.publish()?;
    println!("Created ./{name} (deps: {deps}, Java {java})");
    reported(applied)
}

/// Fetch and unpack start.spring.io's answer into the reserved scratch tree.
fn download_starter(
    publication: &publish::Publication,
    name: &str,
    group: Option<&str>,
    package: Option<&str>,
    deps: &str,
    java: &str,
    debug: bool,
) -> Result<()> {
    let zip_path = publication.enclosure().join("starter.zip");

    // Explicit future/EA choices may be newer than Initializr advertises.
    // Bootstrap with the newest version it accepts, then set the generated
    // Maven release to the version the user actually requested.
    let initializer_java = initializr_java(java);
    let mut curl = Command::new("curl");
    curl.args(["-sf", "https://start.spring.io/starter.zip"])
        .arg("-d")
        .arg(format!("dependencies={deps}"))
        .args(["-d", "type=maven-project"])
        .arg("-d")
        .arg(format!("javaVersion={initializer_java}"))
        .arg("-d")
        .arg(format!("artifactId={name}"))
        // Both only when asked. Initializr has its own defaults and jails
        // stating them again would be a second opinion that drifts the first
        // time either side changes one.
        .args(match group {
            Some(group) => vec!["-d".to_string(), format!("groupId={group}")],
            None => Vec::new(),
        })
        .args(match package {
            Some(package) => vec!["-d".to_string(), format!("packageName={package}")],
            None => Vec::new(),
        })
        .arg("-d")
        .arg(format!("name={name}"))
        .arg("-d")
        .arg(format!("baseDir={name}"))
        .arg("-o")
        .arg(&zip_path);
    if debug {
        jails_support::debug_cmd(&curl);
    }
    let status = curl
        .status()
        .map_err(|e| format!("failed to run curl: {e}"))?;

    if !status.success() {
        return Err(
            jails_support::Failure::Told("starter.zip request failed.\n       fix: retry when start.spring.io is reachable, or run the same command with `--offline`."
                .to_string()),
        );
    }

    // Unpacked into the enclosure rather than into the project root: the
    // archive carries its own `<name>/` folder, which `Publication` has
    // already created, so this lands the contents exactly there.
    let mut unzip = Command::new("unzip");
    unzip
        .args(["-q"])
        .arg(&zip_path)
        .arg("-d")
        .arg(publication.enclosure());
    if debug {
        jails_support::debug_cmd(&unzip);
    }
    let status = unzip
        .status()
        .map_err(|e| format!("failed to run unzip: {e}"))?;

    if !status.success() {
        return Err(jails_support::Failure::Told(
            "failed to extract starter.zip".to_string(),
        ));
    }
    Ok(())
}

fn new_offline(request: &Request<'_>, deps: &str) -> Result<()> {
    let Request {
        name,
        group,
        package,
        java,
        git,
        app,
        debug,
        pretend,
        ..
    } = *request;
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
    let package = resolved_package(name, group, package);
    let class = application_class(name);
    if pretend {
        let root = Path::new(name);
        let source = root.join("src/main/java").join(package.replace('.', "/"));
        let tests = root.join("src/test/java").join(package.replace('.', "/"));
        for path in [
            root.join("pom.xml"),
            source.join(format!("{class}Application.java")),
            tests.join(format!("{class}ApplicationTests.java")),
            root.join("src/main/resources/application.properties"),
            root.join("mise.toml"),
            root.join("AGENTS.md"),
        ] {
            println!("would create {}", path.display());
        }
        if git {
            println!("would run git init in ./{name}");
        }
        println!();
        println!("--pretend: nothing was written. (offline fixture, Java {java})");
        return previewed(app);
    }

    let publication = publish::Publication::reserve(Path::new(name))?;
    let tree = publication.tree();
    let source = tree.join("src/main/java").join(package.replace('.', "/"));
    let tests = tree.join("src/test/java").join(package.replace('.', "/"));

    tree.ensure_directory_at(&source)
        .map_err(|error| format!("failed to create {}: {error}", source.display()))?;
    tree.ensure_directory_at(&tests)
        .map_err(|error| format!("failed to create {}: {error}", tests.display()))?;
    let dependencies = offline_dependencies(deps)?;
    tree.put_named(
        "pom.xml",
        crate::template::render(
            crate::template_here!("new/offline_pom.xml"),
            &[
                ("artifact", name),
                ("java", java),
                ("boot", crate::pom::TARGET_BOOT),
                ("dependencies", &dependencies),
            ],
        ),
        "pom.xml",
    )?;
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
    write_fixtures_dir(&tree)?;
    finish_spring_project(&tree, deps)?;
    ensure_enforcer(&tree, java)?;
    write_mise(&tree, java)?;
    write_agents(&tree, java)?;
    if git {
        tree.put(".gitignore", GITIGNORE)?;
        git_init(&tree, debug);
    }
    let applied = seed(&publication, app, request.no_start, debug)?;

    publication.publish()?;
    println!("Created ./{name} offline (deps: {deps}, Java {java})");
    reported(applied)
}

pub(super) fn offline_dependencies(deps: &str) -> Result<String> {
    let mut out = String::new();
    for id in deps.split(',').map(str::trim).filter(|id| !id.is_empty()) {
        let artifact = match id {
            "web" => "spring-boot-starter-webmvc",
            "validation" => "spring-boot-starter-validation",
            "jdbc" => "spring-boot-starter-jdbc",
            "actuator" => "spring-boot-starter-actuator",
            "security" => "spring-boot-starter-security",
            "data-jpa" => "spring-boot-starter-data-jpa",
            "devtools" => "spring-boot-devtools",
            other => {
                return Err(format!(
                    "offline fixture does not know Initializr dependency `{other}`.\n       \
                     fix: use one of web, validation, jdbc, actuator, security, data-jpa, devtools; or create online."
                ).into());
            }
        };
        out.push_str("        <dependency>\n");
        out.push_str("            <groupId>org.springframework.boot</groupId>\n");
        out.push_str(&format!(
            "            <artifactId>{artifact}</artifactId>\n"
        ));
        if id == "devtools" {
            out.push_str("            <scope>runtime</scope>\n");
            out.push_str("            <optional>true</optional>\n");
        }
        out.push_str("        </dependency>\n");
    }
    Ok(out)
}

/// The three things a freshly bootstrapped Spring project needs and
/// start.spring.io does not provide.
///
/// Run once, after the zip is extracted and before git init, so the initial
/// commit is of a project that is already in the shape jails maintains.
fn finish_spring_project(tree: &publish::Tree<'_>, requested_deps: &str) -> Result<()> {
    verify_requested_deps(tree, requested_deps);
    drop_initializr_help(tree);
    add_jspecify(tree)?;
    // Read rather than assumed: the online path takes whatever Boot line
    // start.spring.io is currently serving, and the properties written below
    // are the ones that line actually has.
    let major = crate::pom::read(tree.root())
        .map(|pom| crate::pom::spring_boot_major_of(&pom))
        .unwrap_or(4);
    write_default_properties(tree, major)?;
    write_devtools_defaults(tree)
}

/// Remove the `HELP.md` start.spring.io ships.
///
/// Its own `.gitignore` lists `HELP.md`, so every new project arrives with a
/// file that looks tracked and is not -- it shows up in an editor, is never
/// committed, and describes a project that has since been reshaped by `jails
/// new`.
///
/// Through `apply::remove` rather than `fs::remove_file`, because the write
/// layer is the only thing that mutates a project and a deletion is a
/// mutation. The first version of this reached for `std::fs` and reopened a
/// gate `tests/architecture.rs` holds closed at zero.
///
/// Best-effort: a project without one is the ordinary case for `--offline`,
/// and failing to delete a file nobody asked for is not a reason to fail
/// creating the project.
fn drop_initializr_help(tree: &publish::Tree<'_>) {
    let help = tree.root().join("HELP.md");
    if help.is_file() {
        let _ = tree.remove_at(&help);
    }
}

/// Make the restart loop as fast as devtools can make it.
///
/// `META-INF/spring-devtools.properties` is read by `DevToolsSettings`, and
/// its `defaults.<property>` entries are added to the environment as the
/// **last** property source -- so anything the project or the reader sets
/// still wins. It is also applied only when devtools is active locally, which
/// means **zero effect on the packaged jar** and zero effect in tests. That
/// combination is what makes it the right place for a machine-loop setting
/// rather than `application.properties`, where it would follow the artifact
/// into production.
///
/// The two values are the ones the loop is actually waiting on: devtools
/// polls the classpath every second and waits 400 ms of quiet before
/// restarting, so a saved file costs up to 1.4 s before the restart even
/// begins. 200 ms and 50 ms are well inside the time a compile takes, and the
/// quiet period only has to outlast the gap between one file being written
/// and the next.
///
/// Not written here: `spring.docker.compose.enabled=false`. `add db` owns
/// that property in its own marked block, and a second owner is how a
/// property ends up with two values and no obvious winner.
pub(super) fn write_devtools_defaults(tree: &publish::Tree<'_>) -> Result<()> {
    let path = tree
        .root()
        .join("src/main/resources/META-INF/spring-devtools.properties");
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        tree.ensure_directory_at(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    tree.put_at(&path, DEVTOOLS_DEFAULTS)
}

const DEVTOOLS_DEFAULTS: &str =
    "# Applied only when spring-boot-devtools is on the classpath and the
# application is running locally -- never in a packaged jar, never in tests.
# These are `defaults.`, so they are added as the last property source and
# anything you set yourself still wins.
#
# Boot's own defaults are 1s and 400ms, which is up to 1.4s of waiting after
# a save before the restart begins.
defaults.spring.devtools.restart.poll-interval=200ms
defaults.spring.devtools.restart.quiet-period=50ms
";

/// Report any `--deps` that did not arrive.
///
/// Initializr silently drops a dependency id it does not recognise -- a typo,
/// or an id that was renamed between Boot versions -- and returns 200 with a
/// project that is missing it. The failure then surfaces much later as an
/// unresolvable import. A warning here is cheap and turns a puzzling compile
/// error into a line at creation time.
///
/// A warning rather than an error: the mapping from an Initializr id to the
/// artifact it contributes is not always one to one, so a false positive must
/// not stop a project from being created.
fn verify_requested_deps(tree: &publish::Tree<'_>, requested: &str) {
    let Ok(pom) = crate::pom::read(tree.root()) else {
        return;
    };
    let missing: Vec<&str> = requested
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        // Initializr ids mostly map to `spring-boot-starter-<id>`, and where
        // they do not, the id itself appears in the artifactId often enough
        // to make this a low-noise check.
        .filter(|id| !pom.contains(&format!("spring-boot-starter-{id}")) && !pom.contains(*id))
        .collect();
    if !missing.is_empty() {
        eprintln!(
            "jails: warning: start.spring.io did not include: {}. Check the dependency id, \
             or add it with `jails add`.",
            missing.join(", ")
        );
    }
}

/// JSpecify, so the null-marked `package-info.java` every generator writes
/// compiles. Boot's dependency management does not pin it, hence the version.
pub(super) fn add_jspecify(tree: &publish::Tree<'_>) -> Result<()> {
    let pom = crate::pom::read(tree.root())?;
    if crate::pom::has_dependency(&pom, "org.jspecify", "jspecify") {
        return Ok(());
    }
    let dep = crate::pom::Dependency {
        group_id: "org.jspecify",
        artifact_id: "jspecify",
        version: Some("1.0.0"),
        scope: None,
        optional: false,
    };
    if let Some(updated) = crate::pom::add_dependency(&pom, &dep)? {
        tree.put_named("pom.xml", updated, "pom.xml")?;
    }
    Ok(())
}

/// Settings both persona files call the default posture, and which an empty
/// `application.properties` leaves off.
///
/// None is discoverable from a failure: an unbounded execution model can
/// quietly overload a downstream pool, abrupt shutdown drops in-flight work,
/// and problem-details absent means clients learn Boot's ad-hoc error map.
///
/// **Keyed by the project's Boot major, because a property that does not exist
/// is silent.** Boot binds what it recognises and ignores the rest, so writing
/// Boot 3 spellings into a Boot 2.7 project produces a file that reads as
/// configured and configures nothing -- the confident wrong answer, in the one
/// place nothing checks. Every entry below is dated from Boot's own
/// `additional-spring-configuration-metadata.json` under `deps/spring-boot`,
/// which records `server.max-http-header-size` as replaced by
/// `server.max-http-request-header-size` at 3.0.0.
pub(super) fn write_default_properties(tree: &publish::Tree<'_>, boot_major: u32) -> Result<()> {
    let path = tree
        .root()
        .join("src/main/resources/application.properties");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let modern = boot_major >= 3;
    let defaults = [
        (
            "# Explicit by design: virtual threads move the concurrency bound to every\n\
             # downstream dependency. Enable them only with measured pool and rate limits.",
            "spring.threads.virtual.enabled=false",
            // Boot 3.2. There is no 2.x spelling to fall back to; the feature
            // is not there.
            modern,
        ),
        (
            "# RFC 9457 problem+json error bodies instead of Boot's default error map.",
            "spring.mvc.problemdetails.enabled=true",
            modern,
        ),
        (
            "# Large signed tokens and tracing baggage can exceed the older 8KB default.",
            "server.max-http-request-header-size=16KB",
            modern,
        ),
        (
            "# Large signed tokens and tracing baggage can exceed the older 8KB default.",
            "server.max-http-header-size=16KB",
            !modern,
        ),
        (
            "# Stop accepting work, then give in-flight requests and transactions time to finish.",
            "server.shutdown=graceful",
            true,
        ),
        (
            "# Bound graceful shutdown so an unhealthy instance cannot stall a rollout forever.",
            "spring.lifecycle.timeout-per-shutdown-phase=30s",
            true,
        ),
    ];
    let mut addition = String::new();
    for (comment, property, applies) in defaults {
        if !applies {
            continue;
        }
        let key = property.split('=').next().unwrap_or(property);
        if existing.contains(key) {
            continue;
        }
        addition.push('\n');
        addition.push_str(comment);
        addition.push('\n');
        addition.push_str(property);
        addition.push('\n');
    }
    if addition.is_empty() {
        return Ok(());
    }
    // `trim_start` on the addition when there was nothing before it: the
    // separator blank line exists to keep jails' block off the end of somebody
    // else's last property, and a file that did not exist has no last property
    // to keep off. Without this the Gradle path, which writes this file from
    // nothing, opened every project with two blank lines.
    let mut next = existing.trim_end().to_string();
    if next.is_empty() {
        return write_properties(tree, &path, addition.trim_start().to_string());
    }
    next.push('\n');
    next.push_str(&addition);
    write_properties(tree, &path, next)
}

fn write_properties(tree: &publish::Tree<'_>, path: &Path, body: String) -> Result<()> {
    if let Some(parent) = path.parent() {
        tree.ensure_directory_at(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    tree.put_at(path, body)
}

pub(super) fn initializr_java(requested: &str) -> &str {
    if requested.parse::<u32>().is_ok_and(|release| release > 26) {
        "26"
    } else {
        requested
    }
}

pub(super) fn set_java_release(tree: &publish::Tree<'_>, from: &str, to: &str) -> Result<()> {
    let path = tree.root().join("pom.xml");
    let pom =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let old = format!("<java.version>{from}</java.version>");
    if !pom.contains(&old) {
        return Err(format!(
            "could not set Java {to}: {} does not contain {old}",
            path.display()
        )
        .into());
    }
    tree.put_at(
        &path,
        pom.replacen(&old, &format!("<java.version>{to}</java.version>"), 1),
    )
}

/// devtools is on by default (fast restart-on-recompile + LiveReload,
/// needed for `jails run --watch` to do anything) -- append it unless
/// already present or explicitly opted out.
pub(super) fn effective_deps(deps: &str, devtools: bool) -> String {
    if !devtools || deps.split(',').any(|d| d.trim() == "devtools") {
        return deps.to_string();
    }
    if deps.trim().is_empty() {
        "devtools".to_string()
    } else {
        format!("{deps},devtools")
    }
}
