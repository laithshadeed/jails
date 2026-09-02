//! Rails-shaped database, Spring console, and runner commands.
//!
//! `db` is `rails dbconsole`: exec `psql` against the compose postgres
//! `add db` started, or `sqlite3` when given a file. `console` is `jshell`
//! with the project's classpath and a booted application context.

mod h2;

pub use h2::Client as H2Client;

use crate::compose;
use crate::find_project_root;
use crate::run;
use jails_support::Result;
use std::fs;
use std::io::{IsTerminal as _, Read, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Open a database client for whichever database this project actually has.
///
/// A path argument is SQLite, an H2 datasource in `application.properties` is
/// H2, and otherwise the compose postgres from `jails add db`. The H2 arm is
/// keyed off the *declared URL* rather than off a recorded capability, for the
/// same reason `sql_dialect` is: a manifest records what was asked for, and a
/// datasource URL is a fact about the database this project will actually
/// meet.
pub fn db(
    file: Option<&Path>,
    web: bool,
    no_start: bool,
    args: &[String],
    debug: bool,
) -> Result<()> {
    let root = find_project_root()?;
    if let Some(path) = file {
        if web {
            return Err("`--web` opens H2's browser console; it does not apply to a SQLite file.\n       fix: drop `--web`, or drop the file argument to use the project's own datasource.".into());
        }
        return sqlite3(&root, path, args, debug);
    }
    let project = crate::project::Project::discover()?;
    if let Some(url) = h2::declared_url(&project) {
        let client = match web {
            true => H2Client::Web,
            false => H2Client::Shell,
        };
        return h2::open(&project, &url, client, args, debug);
    }
    if web {
        return Err("`--web` opens H2's browser console, and this project declares no `jdbc:h2:` datasource.\n       fix: `jails add h2`, or drop `--web` for the PostgreSQL client.".into());
    }
    psql(&root, no_start, args, debug)
}

fn psql(root: &Path, no_start: bool, args: &[String], debug: bool) -> Result<()> {
    let yaml = compose::read(root)?;
    let Some(conn) = compose::postgres_connect(&yaml) else {
        return Err(
            "no postgres in compose.yaml -- run `jails add db` first, or pass a SQLite file".into(),
        );
    };
    if !no_start {
        compose::up(root, &["postgres"], debug);
    }
    let bin = db_client("psql")?;
    let mut cmd = Command::new(&bin);
    cmd.args([
        "-h",
        &conn.host,
        "-p",
        &conn.port.to_string(),
        "-U",
        &conn.user,
        "-d",
        &conn.database,
    ])
    .args(args)
    .env("PGPASSWORD", &conn.password)
    .current_dir(root);
    run::run_inherited(cmd, debug)
}

/// Open the explicitly selected libpq client without starting a service.
pub fn postgres_console(client: &str, single_connection: bool, debug: bool) -> Result<()> {
    let root = find_project_root()?;
    let yaml = compose::read(&root)?;
    let conn = compose::postgres_connect(&yaml).ok_or_else(|| {
        "no declared PostgreSQL datasource exists.\n       fix: run `jails add db`, then start it explicitly with `jails start db`."
            .to_string()
    })?;
    let bin = db_client(client)?;
    let mut command = Command::new(bin);
    command
        .env("PGHOST", &conn.host)
        .env("PGPORT", conn.port.to_string())
        .env("PGUSER", &conn.user)
        .env("PGDATABASE", &conn.database)
        .env("PGPASSWORD", &conn.password)
        .current_dir(root);
    match client {
        "pgcli" => {
            command.arg("--warn");
            if single_connection {
                command.arg("--single-connection");
            }
        }
        "psql" if single_connection => {
            return Err(
                "`--single-connection` is supported only by pgcli.\n       fix: omit it or select `--client pgcli`."
                    .into(),
            );
        }
        "psql" => {}
        _ => unreachable!("CLI has a closed client vocabulary"),
    }
    run::run_inherited(command, debug)
}

fn sqlite3(root: &Path, file: &Path, args: &[String], debug: bool) -> Result<()> {
    let bin = db_client("sqlite3")?;
    let path = if file.is_absolute() {
        file.to_path_buf()
    } else {
        root.join(file)
    };
    let mut cmd = Command::new(&bin);
    cmd.arg(&path).args(args).current_dir(root);
    run::run_inherited(cmd, debug)
}

fn db_client(name: &str) -> Result<PathBuf> {
    if crate::process::on_path(name) {
        return Ok(PathBuf::from(name));
    }
    Err(format!("{name} not on PATH -- install the {name} client and try again").into())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebMode {
    None,
    Random,
    Configured,
}

/// `jails console` with the default profile, no web server and no
/// confirmation prompt.
pub fn console(no_build: bool, args: &[String], debug: bool) -> Result<()> {
    spring_console(&[], None, WebMode::None, !no_build, false, args, debug)
}

/// Boot the selected Spring application and enter an interactive JShell.
pub fn spring_console(
    profiles: &[String],
    main: Option<&str>,
    web: WebMode,
    compile: bool,
    yes: bool,
    args: &[String],
    debug: bool,
) -> Result<()> {
    let project = crate::project::Project::discover()?;
    let root = project.root();
    let jshell = selected_jshell(&project, debug)?;
    let main = main
        .map(str::to_owned)
        .map_or_else(|| spring_main(root), Ok)?;
    confirm_boot(&project, &main, profiles, web, yes)?;
    let resolved = run::runtime_classpath(
        &project,
        if compile {
            run::RunCompile::Build
        } else {
            run::RunCompile::None
        },
        debug,
    )?;
    let classpath = joined_classpath(&resolved)?;
    let temp = tempfile::Builder::new()
        .prefix("jails-console-")
        .tempdir()
        .map_err(|error| format!("could not reserve console scratch space: {error}"))?;
    let startup = temp.path().join("startup.jsh");
    write_private(&startup, spring_startup(&main, profiles, web).as_bytes())?;
    let mut cmd = Command::new(&jshell);
    cmd.args([
        "--execution",
        "local",
        "--class-path",
        &classpath,
        "--startup",
    ])
    .arg(startup)
    .args(args)
    .current_dir(root);
    run::run_inherited(cmd, debug)
}

/// Boot Spring through JShell, evaluate one trusted script, close the
/// application context, and propagate JShell failure.
pub fn runner(
    file: &Path,
    profiles: &[String],
    main: Option<&str>,
    web: WebMode,
    compile: bool,
    yes: bool,
    debug: bool,
) -> Result<()> {
    let project = crate::project::Project::discover()?;
    let root = project.root();
    let jshell = selected_jshell(&project, debug)?;
    let main = main
        .map(str::to_owned)
        .map_or_else(|| spring_main(root), Ok)?;
    confirm_boot(&project, &main, profiles, web, yes)?;
    let resolved = run::runtime_classpath(
        &project,
        if compile {
            run::RunCompile::Build
        } else {
            run::RunCompile::None
        },
        debug,
    )?;
    let classpath = joined_classpath(&resolved)?;
    let temp = tempfile::Builder::new()
        .prefix("jails-runner-")
        .tempdir()
        .map_err(|error| format!("could not reserve runner scratch space: {error}"))?;
    let startup = temp.path().join("startup.jsh");
    let script = temp.path().join("script.jsh");
    let startup_body = spring_startup(&main, profiles, web);
    write_private(&startup, startup_body.as_bytes())?;
    let mut body = Vec::new();
    if file == Path::new("-") {
        std::io::stdin()
            .read_to_end(&mut body)
            .map_err(|error| format!("could not read runner stdin: {error}"))?;
    } else {
        if file.is_absolute()
            || file
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err("runner files must be project-relative.\n       fix: pass a `.jsh` path below the project root or `--file -`.".into());
        }
        body.extend_from_slice(
            &fs::read(root.join(file)).map_err(|error| {
                format!("could not read runner file {}: {error}", file.display())
            })?,
        );
    }
    body.extend_from_slice(b"\njailsClose();\n/exit\n");
    write_private(&script, &body)?;
    let mut command = Command::new(jshell);
    command
        .args([
            "--execution",
            "local",
            "--class-path",
            &classpath,
            "--startup",
        ])
        .arg(startup)
        .arg(script)
        .current_dir(root);
    run_runner(command, debug)
}

fn run_runner(command: Command, debug: bool) -> Result<()> {
    let program = command.get_program().to_string_lossy().into_owned();
    let done = run::run_observed(command, debug)?;
    if jshell_failed(&done.stdout) || jshell_failed(&done.stderr) {
        return Err(
            "runner snippet or Spring context cleanup failed; see the JShell diagnostics above.\n       fix: correct the first reported snippet error, or fix the application's shutdown lifecycle, then rerun the script."
                .into(),
        );
    }
    if done.status.success() {
        return Ok(());
    }
    Err(format!(
        "{program} exited with {}.\n       fix: inspect the JShell diagnostics above, correct the boot or process failure, then rerun the script.",
        done.status
    )
    .into())
}

fn jshell_failed(output: &[u8]) -> bool {
    String::from_utf8_lossy(output).lines().any(|line| {
        let line = line.trim_end_matches('\r');
        line == "Error:"
            || line
                .strip_prefix("Exception ")
                .and_then(|rest| rest.split_whitespace().next())
                .is_some_and(|exception| exception.contains('.'))
    })
}

fn spring_startup(main: &str, profiles: &[String], web: WebMode) -> String {
    let profile_list = if profiles.is_empty() {
        "\"dev\"".to_string()
    } else {
        profiles
            .iter()
            .map(|profile| format!("\"{}\"", java_string(profile)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let web_application_type = match web {
        WebMode::None => "NONE",
        WebMode::Random | WebMode::Configured => "SERVLET",
    };
    let random_port = match web {
        WebMode::Random => "builder.properties(\"server.port=0\");\n",
        WebMode::None | WebMode::Configured => "",
    };
    format!(
        "import java.util.concurrent.TimeUnit;\nimport java.util.concurrent.atomic.AtomicBoolean;\nimport java.util.concurrent.locks.LockSupport;\nimport java.util.function.Supplier;\nimport java.util.stream.Stream;\nimport org.springframework.beans.factory.DisposableBean;\nimport org.springframework.beans.factory.support.BeanDefinitionRegistry;\nimport org.springframework.beans.factory.support.RootBeanDefinition;\nimport org.springframework.boot.WebApplicationType;\nimport org.springframework.boot.builder.SpringApplicationBuilder;\nimport org.springframework.context.ConfigurableApplicationContext;\nimport org.springframework.core.env.Environment;\nimport org.springframework.transaction.PlatformTransactionManager;\nimport org.springframework.transaction.support.TransactionTemplate;\nclass JailsShutdownProbe implements DisposableBean {{ static final AtomicBoolean clean = new AtomicBoolean(); public void destroy() {{ clean.set(true); }} }}\nConfigurableApplicationContext jailsBoot(SpringApplicationBuilder prepared) {{ try {{ return prepared.run(); }} catch (Throwable failure) {{ failure.printStackTrace(System.err); Runtime.getRuntime().halt(1); return null; }} }}\nvar builder = new SpringApplicationBuilder(Class.forName(\"{}\")).profiles({}).web(WebApplicationType.{});\nbuilder.initializers(applicationContext -> ((BeanDefinitionRegistry) applicationContext.getBeanFactory()).registerBeanDefinition(\"jailsShutdownProbe\", new RootBeanDefinition(JailsShutdownProbe.class)));\n{}var ctx = jailsBoot(builder);\nvoid jailsClose() {{ ctx.close(); if (!JailsShutdownProbe.clean.get()) throw new IllegalStateException(\"Spring context cleanup was not observed\"); }}\nRuntime.getRuntime().addShutdownHook(new Thread(() -> {{ long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(10); while (!JailsShutdownProbe.clean.get() && System.nanoTime() < deadline) LockSupport.parkNanos(TimeUnit.MILLISECONDS.toNanos(10)); if (!JailsShutdownProbe.clean.get()) {{ System.err.println(\"jails: Spring context cleanup was not observed\"); Runtime.getRuntime().halt(1); }} }}, \"jails-clean-shutdown\"));\n<T> T bean(Class<T> type) {{ return ctx.getBean(type); }}\nObject bean(String name) {{ return ctx.getBean(name); }}\nStream<String> beans() {{ return java.util.Arrays.stream(ctx.getBeanDefinitionNames()).sorted(); }}\nEnvironment env() {{ return ctx.getEnvironment(); }}\n<T> T tx(Supplier<T> work) {{ return new TransactionTemplate(ctx.getBean(PlatformTransactionManager.class)).execute(status -> work.get()); }}\n",
        java_string(main),
        profile_list,
        web_application_type,
        random_port
    )
}

fn confirm_boot(
    project: &crate::project::Project,
    main: &str,
    profiles: &[String],
    web: WebMode,
    yes: bool,
) -> Result<()> {
    let profiles = if profiles.is_empty() {
        vec!["dev"]
    } else {
        profiles.iter().map(String::as_str).collect()
    };
    if profiles
        .iter()
        .all(|profile| matches!(*profile, "dev" | "test"))
        && web != WebMode::Configured
    {
        return Ok(());
    }
    let release = project
        .java_release()
        .map_or_else(|| "unknown".to_string(), |release| release.to_string());
    let web = match web {
        WebMode::None => "none",
        WebMode::Random => "random",
        WebMode::Configured => "configured",
    };
    eprintln!(
        "Spring application preflight:\n  main: {main}\n  release: {release}\n  profiles: {}\n  web: {web}\n  datasource sources: {} (values redacted)",
        profiles.join(", "),
        datasource_sources(project)
    );
    if yes {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(
            "Spring boot confirmation requires a terminal.\n       fix: review the preflight above, then pass `--yes` to authorize that exact boot in automation."
                .into(),
        );
    }
    eprint!("Continue? [y/N] ");
    std::io::stderr()
        .flush()
        .map_err(|error| format!("could not display Spring boot confirmation: {error}"))?;
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|error| format!("could not read Spring boot confirmation: {error}"))?;
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        return Ok(());
    }
    Err(
        "Spring application boot cancelled.\n       fix: rerun the command and confirm, or pass `--yes` after reviewing the preflight."
            .into(),
    )
}

fn datasource_sources(project: &crate::project::Project) -> String {
    let root = project.root();
    let resources = root.join("src/main/resources");
    let mut sources = fs::read_dir(&resources)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("application")
                && matches!(
                    entry.path().extension().and_then(|ext| ext.to_str()),
                    Some("properties" | "yaml" | "yml")
                )
        })
        .filter_map(|entry| {
            let body = fs::read_to_string(entry.path()).ok()?;
            (body.contains("spring.datasource")
                || body.contains("jdbc:")
                || body.contains("r2dbc:"))
            .then(|| format!("src/main/resources/{}", entry.file_name().to_string_lossy()))
        })
        .collect::<Vec<_>>();
    for name in ["compose.yaml", "compose.yml"] {
        let path = root.join(name);
        if fs::read_to_string(path).is_ok_and(|body| body.contains("postgres")) {
            sources.push(name.to_string());
        }
    }
    sources.sort();
    sources.dedup();
    if sources.is_empty() {
        "none declared in project files".to_string()
    } else {
        sources.join(", ")
    }
}

fn java_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn spring_main(project_root: &Path) -> Result<String> {
    let candidates = crate::java::source_files(&project_root.join("src/main/java"))
        .into_iter()
        .filter_map(|path| {
            let source = fs::read_to_string(path).ok()?;
            if !source.contains("@SpringBootApplication") {
                return None;
            }
            let info = crate::java::type_info(&source)?;
            Some(if info.package.is_empty() {
                info.name
            } else {
                format!("{}.{}", info.package, info.name)
            })
        })
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [main] => Ok(main.clone()),
        [] => Err("no @SpringBootApplication type was found.\n       fix: pass `--main <qualified-type>`.".into()),
        _ => Err("more than one @SpringBootApplication type was found.\n       fix: select one with `--main <qualified-type>`.".into()),
    }
}

fn write_private(path: &Path, body: &[u8]) -> Result<()> {
    jails_support::apply::put_in_scratch(path, body)
}

fn selected_jshell(project: &crate::project::Project, debug: bool) -> Result<PathBuf> {
    let java = run::selected_java(project, debug)?;
    let jshell = java.with_file_name(if cfg!(windows) {
        "jshell.exe"
    } else {
        "jshell"
    });
    let output = Command::new(&jshell)
        .arg("--version")
        .output()
        .map_err(|error| {
            format!(
                "selected JShell executable `{}` is unavailable: {error}\n       fix: set JAVA_HOME to a full JDK that supports this project",
                jshell.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "selected JShell executable `{}` rejected `--version`\n       fix: set JAVA_HOME to a working full JDK",
            jshell.display()
        )
        .into());
    }
    Ok(jshell)
}

fn joined_classpath(resolved: &run::RuntimeClasspath) -> Result<String> {
    Ok(std::env::join_paths(&resolved.entries)
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| format!("failed to join classpath: {e}"))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_startup_exposes_only_the_documented_helpers() {
        let startup = spring_startup(
            "com.example.DemoApplication",
            &["test".into()],
            WebMode::Random,
        );
        for helper in [
            "var ctx = jailsBoot(builder)",
            "bean(Class<T> type)",
            "bean(String name)",
            "Stream<String> beans()",
            "Environment env()",
            "tx(Supplier<T> work)",
        ] {
            assert!(startup.contains(helper), "missing `{helper}`:\n{startup}");
        }
        assert!(startup.contains("profiles(\"test\")"));
        assert!(startup.contains("WebApplicationType.SERVLET"));
        assert!(startup.contains("server.port=0"));
        assert!(startup.contains("addShutdownHook"));
        assert!(startup.contains("registerBeanDefinition"));
        assert!(startup.contains("jailsClose()"));
    }

    #[test]
    fn console_defaults_to_dev_without_a_web_server() {
        let startup = spring_startup("com.example.DemoApplication", &[], WebMode::None);
        assert!(startup.contains("profiles(\"dev\")"));
        assert!(startup.contains("WebApplicationType.NONE"));
        assert!(!startup.contains("server.port=0"));
    }

    #[test]
    fn batch_jshell_diagnostics_distinguish_failures_from_application_output() {
        assert!(jshell_failed(b"Error:\nillegal start of expression\n"));
        assert!(jshell_failed(
            b"Exception java.lang.IllegalStateException: failed\n"
        ));
        assert!(!jshell_failed(
            b"application Error: recovered\nException count: 0\n"
        ));
    }
}
