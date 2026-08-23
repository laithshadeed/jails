mod publish;

use jails_support::Result;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Port of the `spring-init` bash function: wraps start.spring.io's
/// starter.zip API. baseDir wraps the archive in a `$name/` folder
/// server-side, so extracting to "." lands the project at `./$name`.
/// Seed a freshly created project with a manifest and apply it.
///
/// plan.md §18 ends by asking which two commands should have been one, and
/// answers itself: `new` + `mkdir .jails` + `cp app.toml` + `app apply` is four
/// steps that only ever appear together. The manifest path is resolved against
/// the directory the *user* is standing in, not the project just created,
/// because that is where they are pointing from.
pub(crate) fn seed_manifest(root: &Path, manifest: &Path, debug: bool) -> Result<()> {
    let source = std::fs::read_to_string(manifest).map_err(|error| {
        format!(
            "failed to read the application manifest {}: {error}\n       \
             fix: pass `--app <path>` pointing at a readable `.jails/app.toml`.",
            manifest.display()
        )
    })?;
    crate::apply::put(root.join(".jails/app.toml"), &source)?;
    println!("  manifest {}", manifest.display());
    crate::app::apply_in(root, false, debug)
}

pub fn new(
    name: &str,
    deps: &str,
    java: &str,
    git: bool,
    devtools: bool,
    offline: bool,
    app: Option<&Path>,
    debug: bool,
    pretend: bool,
) -> Result<()> {
    if Path::new(name).exists() {
        return Err(publish::already_exists(Path::new(name)));
    }

    // Refused rather than ignored. The project `new` creates is whatever
    // start.spring.io returns, so the only honest preview would be to fetch
    // the zip -- and a `--pretend` that hits the network to tell you what it
    // would have done is not a preview. `new-cli` writes a file set jails
    // knows, so that one previews for real.
    if pretend && !offline {
        return Err(
            "`--pretend` is not supported for `new`: the project comes from start.spring.io, \
             so jails cannot say what is in it without downloading it.\n\n\
             fix: run `jails new-cli --pretend` to preview a project jails writes itself, or \
             run `jails new` and inspect the result."
                .to_string(),
        );
    }

    let deps = effective_deps(deps, devtools);
    let deps = deps.as_str();

    if offline {
        return new_offline(name, deps, java, git, app, debug, pretend);
    }

    let publication = publish::Publication::reserve(Path::new(name))?;
    let root = publication.root().to_path_buf();
    download_starter(&publication, name, deps, java, debug)?;

    if initializr_java(java) != java {
        set_java_release(&root, initializr_java(java), java)?;
    }
    write_fixtures_dir(&root)?;
    finish_spring_project(&root, deps)?;
    ensure_enforcer(&root, java)?;
    write_mise(&root, java)?;
    write_agents(&root, java)?;

    // start.spring.io's zip already ships a .gitignore, so just init.
    if git {
        git_init(&root, debug);
    }
    seed(&publication, app, debug)?;

    publication.publish()?;
    println!("Created ./{name} (deps: {deps}, Java {java})");
    Ok(())
}

/// Fetch and unpack start.spring.io's answer into the reserved scratch tree.
fn download_starter(
    publication: &publish::Publication,
    name: &str,
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
            "starter.zip request failed.\n       fix: retry when start.spring.io is reachable, or run the same command with `--offline`."
                .to_string(),
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
        return Err("failed to extract starter.zip".to_string());
    }
    Ok(())
}

fn new_offline(
    name: &str,
    deps: &str,
    java: &str,
    git: bool,
    app: Option<&Path>,
    debug: bool,
    pretend: bool,
) -> Result<()> {
    let release = java
        .parse::<u32>()
        .map_err(|_| format!("--java must be a release number, got `{java}`"))?;
    if release < crate::pom::MIN_RELEASE {
        return Err(format!(
            "--java {java} is below Java {}, which jails' generated code needs",
            crate::pom::MIN_RELEASE
        ));
    }
    let package = sanitize_package(name);
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
    let root = publication.root();
    let source = root.join("src/main/java").join(package.replace('.', "/"));
    let tests = root.join("src/test/java").join(package.replace('.', "/"));

    jails_support::apply::ensure_directory(&source)
        .map_err(|error| format!("failed to create {}: {error}", source.display()))?;
    jails_support::apply::ensure_directory(&tests)
        .map_err(|error| format!("failed to create {}: {error}", tests.display()))?;
    let dependencies = offline_dependencies(deps)?;
    crate::apply::put_named(
        root.join("pom.xml"),
        crate::template::render(
            crate::template_here!("new/offline_pom.xml"),
            &[
                ("artifact", name),
                ("java", java),
                ("dependencies", &dependencies),
            ],
        ),
        "pom.xml",
    )?;
    crate::generate::write_new_file(
        root,
        &source.join(format!("{class}Application.java")),
        &crate::template::render(
            crate::template_here!("new/offline_application.java"),
            &[("package", &package), ("class", &class)],
        ),
    )?;
    crate::generate::write_new_file(
        root,
        &tests.join(format!("{class}ApplicationTests.java")),
        &crate::template::render(
            crate::template_here!("new/offline_application_test.java"),
            &[("package", &package), ("class", &class)],
        ),
    )?;
    write_fixtures_dir(root)?;
    finish_spring_project(root, deps)?;
    ensure_enforcer(root, java)?;
    write_mise(root, java)?;
    write_agents(root, java)?;
    if git {
        crate::apply::put(root.join(".gitignore"), GITIGNORE)?;
        git_init(root, debug);
    }
    seed(&publication, app, debug)?;

    publication.publish()?;
    println!("Created ./{name} offline (deps: {deps}, Java {java})");
    Ok(())
}

fn application_class(name: &str) -> String {
    let mut out = String::new();
    let mut uppercase = true;
    for character in name.chars() {
        if !character.is_ascii_alphanumeric() {
            uppercase = true;
            continue;
        }
        if uppercase {
            out.extend(character.to_uppercase());
            uppercase = false;
        } else {
            out.push(character);
        }
    }
    if out.is_empty() {
        "Application".to_string()
    } else {
        out
    }
}

fn offline_dependencies(deps: &str) -> Result<String> {
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
                ));
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
fn finish_spring_project(root: &Path, requested_deps: &str) -> Result<()> {
    verify_requested_deps(root, requested_deps);
    add_jspecify(root)?;
    write_default_properties(root)?;
    write_devtools_defaults(root)
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
fn write_devtools_defaults(root: &Path) -> Result<()> {
    let path = root.join("src/main/resources/META-INF/spring-devtools.properties");
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        jails_support::apply::ensure_directory(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    crate::apply::put(&path, DEVTOOLS_DEFAULTS)
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
fn verify_requested_deps(root: &Path, requested: &str) {
    let Ok(pom) = crate::pom::read(root) else {
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
fn add_jspecify(root: &Path) -> Result<()> {
    let pom = crate::pom::read(root)?;
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
        crate::apply::put_named(root.join("pom.xml"), updated, "pom.xml")?;
    }
    Ok(())
}

/// Two settings both persona files call the default posture, and which an
/// empty `application.properties` leaves off.
///
/// None is discoverable from a failure: an unbounded execution model can
/// quietly overload a downstream pool, abrupt shutdown drops in-flight work,
/// and problem-details absent means clients learn Boot's ad-hoc error map.
fn write_default_properties(root: &Path) -> Result<()> {
    let path = root.join("src/main/resources/application.properties");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let defaults = [
        (
            "# Explicit by design: virtual threads move the concurrency bound to every\n\
             # downstream dependency. Enable them only with measured pool and rate limits.",
            "spring.threads.virtual.enabled=false",
        ),
        (
            "# RFC 9457 problem+json error bodies instead of Boot's default error map.",
            "spring.mvc.problemdetails.enabled=true",
        ),
        (
            "# Large signed tokens and tracing baggage can exceed the older 8KB default.",
            "server.max-http-request-header-size=16KB",
        ),
        (
            "# Stop accepting work, then give in-flight requests and transactions time to finish.",
            "server.shutdown=graceful",
        ),
        (
            "# Bound graceful shutdown so an unhealthy instance cannot stall a rollout forever.",
            "spring.lifecycle.timeout-per-shutdown-phase=30s",
        ),
    ];
    let mut addition = String::new();
    for (comment, property) in defaults {
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
    let mut next = existing.trim_end().to_string();
    next.push('\n');
    next.push_str(&addition);
    if let Some(parent) = path.parent() {
        jails_support::apply::ensure_directory(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    crate::apply::put(&path, next)
}

fn initializr_java(requested: &str) -> &str {
    if requested.parse::<u32>().is_ok_and(|release| release > 26) {
        "26"
    } else {
        requested
    }
}

fn set_java_release(root: &Path, from: &str, to: &str) -> Result<()> {
    let path = root.join("pom.xml");
    let pom =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let old = format!("<java.version>{from}</java.version>");
    if !pom.contains(&old) {
        return Err(format!(
            "could not set Java {to}: {} does not contain {old}",
            path.display()
        ));
    }
    crate::apply::put(
        &path,
        pom.replacen(&old, &format!("<java.version>{to}</java.version>"), 1),
    )
}

/// Plain Maven CLI project, written directly -- no `mvn archetype:generate`
/// (slow, needs network, and falls into an interactive catalog picker
/// without exact archetype coordinates).
pub fn new_cli(
    name: &str,
    java: &str,
    git: bool,
    app: Option<&Path>,
    debug: bool,
    pretend: bool,
) -> Result<()> {
    if Path::new(name).exists() {
        return Err(publish::already_exists(Path::new(name)));
    }

    // A generic tool cannot hardcode one release level: the LTS most people
    // run and the newest JDK are rarely the same number.
    match java.parse::<u32>() {
        Ok(level) if level < crate::pom::MIN_RELEASE => {
            return Err(format!(
                "--release {java} is below Java {}, which is what jails' generated code needs",
                crate::pom::MIN_RELEASE
            ));
        }
        Ok(_) => {}
        Err(_) => return Err(format!("--release must be a number, got '{java}'")),
    }

    let package = sanitize_package(name);

    // Every path below is written unconditionally, so the preview is the
    // list itself rather than a second description of it that can drift.
    if pretend {
        let root = Path::new(name);
        let mut planned = vec![
            root.join("pom.xml"),
            root.join("src/main/java")
                .join(package.replace('.', "/"))
                .join("App.java"),
            root.join("src/test/java")
                .join(package.replace('.', "/"))
                .join("AppTest.java"),
            root.join("src/test/resources/fixtures/.gitkeep"),
            root.join("mise.toml"),
            root.join("AGENTS.md"),
        ];
        if git {
            planned.push(root.join(".gitignore"));
        }
        for path in planned {
            println!("would create {}", path.display());
        }
        if git {
            println!("would run git init in ./{name}");
        }
        println!();
        println!("--pretend: nothing was written. (package: {package}, Java {java})");
        return previewed(app);
    }

    let publication = publish::Publication::reserve(Path::new(name))?;
    let root = publication.root();
    let src_dir = root.join("src/main/java").join(package.replace('.', "/"));
    let test_dir = root.join("src/test/java").join(package.replace('.', "/"));

    jails_support::apply::ensure_directory(&src_dir)
        .map_err(|e| format!("failed to create {}: {e}", src_dir.display()))?;
    jails_support::apply::ensure_directory(&test_dir)
        .map_err(|e| format!("failed to create {}: {e}", test_dir.display()))?;

    crate::apply::put_named(
        root.join("pom.xml"),
        pom_xml(name, &package, java),
        "pom.xml",
    )?;
    // Through write_new_file, not fs::write, so the entry point and its test
    // get the same import ordering as everything jails generates later --
    // otherwise `add format` finds violations in files jails itself wrote.
    //
    // App.java *is* the command dispatcher, not a Hello World stub. A command
    // called `new-cli` that produces a project unable to dispatch commands
    // makes `jails generate command` -- the obvious next step -- report that
    // it has nothing to register into, and leaves you with two `main`s the
    // moment you fix that by hand.
    // `root` is the project being created, not the process CWD. Passing it
    // is what gives a new-cli project's own base package the null-marked
    // `package-info.java` every other package gets -- the lookup this
    // replaced either found the surrounding project or found nothing.
    crate::generate::write_new_file(
        root,
        &src_dir.join("App.java"),
        &crate::generate::cli_java(&package, "App", name),
    )?;
    crate::generate::write_new_file(
        root,
        &test_dir.join("AppTest.java"),
        &crate::generate::cli_test(&package, "App"),
    )?;

    write_fixtures_dir(root)?;
    write_mise(root, java)?;
    write_agents(root, java)?;

    if git {
        crate::apply::put(root.join(".gitignore"), GITIGNORE)?;
        git_init(root, debug);
    }
    seed(&publication, app, debug)?;

    publication.publish()?;
    println!("Created ./{name} (package: {package}, Java {java})");
    Ok(())
}

/// Apply `--app <manifest>` to the project being created, before it is
/// published.
///
/// plan.md §R6.5 puts the manifest inside the publication rather than after
/// it: `new --app` is one command, and a destination holding a project whose
/// manifest half-applied is exactly the state publication-by-rename exists to
/// remove. The manifest path is resolved against the directory the *user* is
/// standing in, which is why it is read before anything is written.
fn seed(publication: &publish::Publication, app: Option<&Path>, debug: bool) -> Result<()> {
    match app {
        Some(manifest) => seed_manifest(publication.root(), manifest, debug),
        None => Ok(()),
    }
}

/// What `--app` does under `--pretend`: nothing, and says so.
///
/// A preview created no project, so there is nothing to apply a manifest to,
/// and saying that beats failing to find a `pom.xml` that was never written.
fn previewed(app: Option<&Path>) -> Result<()> {
    if let Some(manifest) = app {
        println!(
            "--pretend: no project was created, so {} was not applied.",
            manifest.display()
        );
    }
    Ok(())
}

/// Test fixtures land on the test classpath, so they belong under
/// `src/test/resources`. Git can't track an empty directory, so seed it with
/// a `.gitkeep` -- otherwise the folder vanishes on the first clone.
fn write_fixtures_dir(root: &Path) -> Result<()> {
    let dir = root.join("src/test/resources/fixtures");
    jails_support::apply::ensure_directory(&dir)?;
    crate::apply::put(dir.join(".gitkeep"), "")?;
    Ok(())
}

fn write_mise(root: &Path, java: &str) -> Result<()> {
    let path = root.join("mise.toml");
    crate::apply::put(&path, format!("[tools]\njava = \"{java}\"\n"))
}

fn write_agents(root: &Path, java: &str) -> Result<()> {
    let path = root.join("AGENTS.md");
    if path.exists() {
        return Ok(());
    }
    let package = crate::generate::base_package(root)
        .unwrap_or_else(|_| "the package declared by the application entry point".to_string());
    let rules = crate::lint::agents_rules();
    let body = format!(
        r#"# Working on this project

This project targets Java {java}. Its base package is `{package}`.

## Commands

- Run one test with `jails test <Name>` or place the cursor in it and pass `path:line`.
- Run `jails check` before handing work off. It performs a clean Maven verify so stale class files cannot hide a regression.
- Run `jails doctor` before debugging the machine, container runtime, ports, or dependency injection.
- Use `jails g <kind> --pretend` and `jails add <capability> --pretend` to inspect changes.

## Design

- Domain values are immutable records. Use no ORM and no Lombok.
- Persistence is a repository port plus an explicit JDBC adapter and forward-only SQL migrations.
- Field specs use `name:type`, `name:type!` for non-blank text, `name:type?` for nullable values, and `@pk`, `@unique`, `@index`, or numeric constraints where applicable.
- Keep domain, application, service, adapter, web/API, messaging, job, and testkit packages separate. `jails about --json` reports the configured names.
- Generated applications must remain operable without jails installed.

## Checked stale APIs

`jails lint` enforces this exact list:

{rules}
"#
    );
    crate::apply::put(&path, body)
}

fn ensure_enforcer(root: &Path, java: &str) -> Result<()> {
    let pom = crate::pom::read(root)?;
    let plugin = format!(
        r#"<plugin>
    <groupId>org.apache.maven.plugins</groupId>
    <artifactId>maven-enforcer-plugin</artifactId>
    <version>3.6.3</version>
    <executions>
        <execution>
            <id>jails-toolchain</id>
            <goals><goal>enforce</goal></goals>
            <configuration>
                <rules>
                    <requireJavaVersion>
                        <version>[{java},)</version>
                        <message>This project requires Java {java}+; select it with mise use java@{java}.</message>
                    </requireJavaVersion>
                    <requireMavenVersion>
                        <version>[3.9,)</version>
                        <message>This project requires Maven 3.9+; use the project wrapper or upgrade Maven.</message>
                    </requireMavenVersion>
                </rules>
            </configuration>
        </execution>
    </executions>
</plugin>"#
    );
    if let Some(updated) = crate::pom::add_plugin(&pom, "maven-enforcer-plugin", &plugin)? {
        crate::apply::put_named(root.join("pom.xml"), updated, "pom.xml")?;
    }
    Ok(())
}

const GITIGNORE: &str = "target/\n*.class\n.idea/\n*.iml\n.DS_Store\n";

/// Best-effort: a missing/broken git shouldn't fail project creation, just
/// skip repo setup with a warning.
fn git_init(root: &Path, debug: bool) {
    let mut cmd = Command::new("git");
    cmd.args(["init", "-q"]).current_dir(root);
    if debug {
        jails_support::debug_cmd(&cmd);
    }
    match cmd.status() {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("jails: git init exited with {status}, skipping"),
        Err(e) => eprintln!("jails: failed to run git init: {e}"),
    }
}

/// devtools is on by default (fast restart-on-recompile + LiveReload,
/// needed for `jails run --watch` to do anything) -- append it unless
/// already present or explicitly opted out.
fn effective_deps(deps: &str, devtools: bool) -> String {
    if !devtools || deps.split(',').any(|d| d.trim() == "devtools") {
        return deps.to_string();
    }
    if deps.trim().is_empty() {
        "devtools".to_string()
    } else {
        format!("{deps},devtools")
    }
}

/// artifactId -> a lowercase, dot-free Java package segment under com.example.
fn sanitize_package(name: &str) -> String {
    let segment: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase();
    format!("com.example.{segment}")
}

fn pom_xml(artifact: &str, package: &str, java: &str) -> String {
    format!(
        r#"<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>

    <groupId>com.example</groupId>
    <artifactId>{artifact}</artifactId>
    <version>0.1.0</version>
    <packaging>jar</packaging>

    <properties>
        <maven.compiler.release>{java}</maven.compiler.release>
        <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
    </properties>

    <dependencies>
        <!--
          JSpecify's @NullMarked is a package-level opt-in, and every package
          jails generates carries one. Without this dependency those
          package-info.java files do not compile.
        -->
        <dependency>
            <groupId>org.jspecify</groupId>
            <artifactId>jspecify</artifactId>
            <version>1.0.0</version>
        </dependency>
        <dependency>
            <groupId>org.junit.jupiter</groupId>
            <artifactId>junit-jupiter</artifactId>
            <version>6.1.2</version>
            <scope>test</scope>
        </dependency>
        <dependency>
            <groupId>org.assertj</groupId>
            <artifactId>assertj-core</artifactId>
            <version>3.27.7</version>
            <scope>test</scope>
        </dependency>
    </dependencies>

    <build>
        <finalName>{artifact}</finalName>
        <plugins>
            <plugin>
                <groupId>org.apache.maven.plugins</groupId>
                <artifactId>maven-compiler-plugin</artifactId>
                <version>3.13.0</version>
            </plugin>
            <plugin>
                <groupId>org.apache.maven.plugins</groupId>
                <artifactId>maven-surefire-plugin</artifactId>
                <version>3.2.5</version>
            </plugin>
            <plugin>
                <groupId>org.apache.maven.plugins</groupId>
                <artifactId>maven-jar-plugin</artifactId>
                <version>3.4.1</version>
                <configuration>
                    <archive>
                        <manifest>
                            <mainClass>{package}.App</mainClass>
                        </manifest>
                    </archive>
                </configuration>
            </plugin>
        </plugins>
    </build>
</project>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_support::CWD_LOCK;
    use std::path::PathBuf;

    fn scratch(label: &str) -> PathBuf {
        crate::scratch::ScratchDir::in_temp(&format!("jails-new-test-{label}"))
            .unwrap()
            .keep()
    }

    #[test]
    fn effective_deps_appends_devtools_by_default() {
        assert_eq!(effective_deps("web", true), "web,devtools");
        assert_eq!(effective_deps("web,jdbc", true), "web,jdbc,devtools");
    }

    #[test]
    fn effective_deps_skips_devtools_when_disabled() {
        assert_eq!(effective_deps("web", false), "web");
    }

    #[test]
    fn effective_deps_does_not_duplicate_an_explicit_devtools() {
        assert_eq!(effective_deps("web,devtools", true), "web,devtools");
    }

    #[test]
    fn effective_deps_handles_an_empty_deps_string() {
        assert_eq!(effective_deps("", true), "devtools");
    }

    #[test]
    fn initializr_uses_its_newest_supported_release_for_java_27() {
        assert_eq!(initializr_java("27"), "26");
        assert_eq!(initializr_java("26"), "26");
        assert_eq!(initializr_java("21"), "21");
    }

    #[test]
    fn generated_spring_pom_is_retargeted_after_bootstrapping() {
        let root = scratch("retarget-java");
        fs::write(
            root.join("pom.xml"),
            "<project><properties><java.version>26</java.version></properties></project>",
        )
        .unwrap();

        set_java_release(&root, "26", "27").unwrap();

        let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
        assert!(pom.contains("<java.version>27</java.version>"));
        assert!(!pom.contains("<java.version>26</java.version>"));
    }

    #[test]
    fn sanitize_package_strips_non_alphanumerics_and_lowercases() {
        assert_eq!(sanitize_package("my-app"), "com.example.myapp");
        assert_eq!(sanitize_package("MyApp2"), "com.example.myapp2");
    }

    #[test]
    fn pom_xml_pins_the_requested_java_release_and_main_class() {
        let pom = pom_xml("demo", "com.example.demo", crate::pom::TARGET_RELEASE);
        assert!(pom.contains(&format!(
            "<maven.compiler.release>{}</maven.compiler.release>",
            crate::pom::TARGET_RELEASE
        )));
        // The release is whatever the caller asked for, not a baked-in constant.
        assert!(
            pom_xml("demo", "com.example.demo", "21")
                .contains("<maven.compiler.release>21</maven.compiler.release>")
        );
        assert!(pom.contains("<mainClass>com.example.demo.App</mainClass>"));
        assert!(pom.contains("<artifactId>demo</artifactId>"));
    }

    #[test]
    fn pom_xml_declares_junit_and_assertj_as_test_dependencies() {
        let pom = pom_xml("demo", "com.example.demo", crate::pom::TARGET_RELEASE);
        assert!(pom.contains("<artifactId>junit-jupiter</artifactId>"));
        assert!(pom.contains("<artifactId>assertj-core</artifactId>"));
    }

    /// The entry point is a dispatcher, not a Hello World stub -- otherwise
    /// `generate command` has nothing to register into, which is the whole
    /// point of `new-cli`.
    #[test]
    fn app_java_is_a_command_dispatcher() {
        let src = crate::generate::cli_java("com.example.demo", "App", "demo");
        assert!(src.contains("package com.example.demo;"));
        assert!(src.contains("public static void main(String[] args)"));
        assert!(src.contains("public final class App"), "{src}");
        assert!(
            src.contains("usage: demo <command> [args]"),
            "the program name should be the project's"
        );
        assert!(
            crate::generate::is_dispatcher(&src),
            "generate command must be able to find this"
        );
    }

    #[test]
    fn app_test_java_drives_the_dispatcher() {
        let src = crate::generate::cli_test("com.example.demo", "App");
        assert!(src.contains("import org.junit.jupiter.api.Test;"));
        assert!(src.contains("class AppTest"));
        assert!(src.contains("App.run("));
    }

    #[test]
    fn new_cli_writes_pom_and_sources_under_the_target_directory() {
        let _guard = CWD_LOCK.lock().unwrap();
        let workdir = scratch("new-cli");
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli(
            "demo-app",
            crate::pom::TARGET_RELEASE,
            false,
            None,
            false,
            false,
        );
        std::env::set_current_dir(&original_cwd).unwrap();
        result.unwrap();

        let root = workdir.join("demo-app");
        assert!(root.join("pom.xml").is_file());
        let app = root.join("src/main/java/com/example/demoapp/App.java");
        let test = root.join("src/test/java/com/example/demoapp/AppTest.java");
        assert!(app.is_file(), "expected {}", app.display());
        assert!(test.is_file(), "expected {}", test.display());
        let fixtures = root.join("src/test/resources/fixtures");
        assert!(fixtures.is_dir(), "expected {}", fixtures.display());
        assert!(
            fixtures.join(".gitkeep").is_file(),
            "fixtures dir needs a .gitkeep to survive a clone"
        );
    }

    #[test]
    fn new_cli_refuses_to_overwrite_an_existing_directory() {
        let _guard = CWD_LOCK.lock().unwrap();
        let workdir = scratch("new-cli-exists");
        fs::create_dir_all(workdir.join("demo-app")).unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli(
            "demo-app",
            crate::pom::TARGET_RELEASE,
            false,
            None,
            false,
            false,
        );
        std::env::set_current_dir(&original_cwd).unwrap();

        assert!(result.is_err());
    }

    #[test]
    fn new_cli_skips_git_setup_when_disabled() {
        let _guard = CWD_LOCK.lock().unwrap();
        let workdir = scratch("new-cli-no-git");
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli(
            "demo-app",
            crate::pom::TARGET_RELEASE,
            false,
            None,
            false,
            false,
        );
        std::env::set_current_dir(&original_cwd).unwrap();
        result.unwrap();

        let root = workdir.join("demo-app");
        assert!(!root.join(".gitignore").exists());
        assert!(!root.join(".git").exists());
    }

    #[test]
    fn new_cli_sets_up_git_by_default() {
        let _guard = CWD_LOCK.lock().unwrap();
        let workdir = scratch("new-cli-git");
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli(
            "demo-app",
            crate::pom::TARGET_RELEASE,
            true,
            None,
            false,
            false,
        );
        std::env::set_current_dir(&original_cwd).unwrap();
        result.unwrap();

        let root = workdir.join("demo-app");
        let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(gitignore.contains("target/"));
        assert!(root.join(".git").is_dir());
    }
}
