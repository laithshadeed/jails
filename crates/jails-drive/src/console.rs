//! Rails-shaped database, Spring console, and runner commands.
//!
//! `db` is `rails dbconsole`: exec `psql` against the compose postgres
//! `add db` started, or `sqlite3` when given a file. `console` is `jshell`
//! with the project's classpath and a booted application context.

use crate::compose;
use crate::generate::find_project_root;
use crate::run;
use jails_support::Result;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Open a database client. A path argument is SQLite; otherwise the
/// compose postgres from `jails add db`.
pub fn db(file: Option<&Path>, no_start: bool, args: &[String], debug: bool) -> Result<()> {
    let root = find_project_root()?;
    if let Some(path) = file {
        return sqlite3(&root, path, args, debug);
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

/// Compatibility entry point for the former console API.
pub fn console(no_build: bool, args: &[String], debug: bool) -> Result<()> {
    spring_console(&[], None, WebMode::None, !no_build, args, debug)
}

/// Boot the selected Spring application and enter an interactive JShell.
pub fn spring_console(
    profiles: &[String],
    main: Option<&str>,
    web: WebMode,
    compile: bool,
    args: &[String],
    debug: bool,
) -> Result<()> {
    let project = crate::model::Project::discover()?;
    let root = project.root();
    let jshell = selected_jshell(&project, debug)?;
    let main = main
        .map(str::to_owned)
        .map_or_else(|| spring_main(root), Ok)?;
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
    cmd.args(["--class-path", &classpath, "--startup"])
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
    debug: bool,
) -> Result<()> {
    let project = crate::model::Project::discover()?;
    let root = project.root();
    let jshell = selected_jshell(&project, debug)?;
    let main = main
        .map(str::to_owned)
        .map_or_else(|| spring_main(root), Ok)?;
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
        body = fs::read(root.join(file))
            .map_err(|error| format!("could not read runner file {}: {error}", file.display()))?;
    }
    body.extend_from_slice(b"\nctx.close();\n/exit\n");
    write_private(&script, &body)?;
    let mut command = Command::new(jshell);
    command
        .args(["--class-path", &classpath, "--startup"])
        .arg(startup)
        .arg(script)
        .current_dir(root);
    run::run_inherited(command, debug)
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
        "import java.util.function.Supplier;\nimport java.util.stream.Stream;\nimport org.springframework.boot.WebApplicationType;\nimport org.springframework.boot.builder.SpringApplicationBuilder;\nimport org.springframework.context.ConfigurableApplicationContext;\nimport org.springframework.core.env.Environment;\nimport org.springframework.transaction.PlatformTransactionManager;\nimport org.springframework.transaction.support.TransactionTemplate;\nvar builder = new SpringApplicationBuilder(Class.forName(\"{}\")).profiles({}).web(WebApplicationType.{});\n{}var ctx = builder.run();\nRuntime.getRuntime().addShutdownHook(new Thread(ctx::close));\n<T> T bean(Class<T> type) {{ return ctx.getBean(type); }}\nObject bean(String name) {{ return ctx.getBean(name); }}\nStream<String> beans() {{ return java.util.Arrays.stream(ctx.getBeanDefinitionNames()).sorted(); }}\nEnvironment env() {{ return ctx.getEnvironment(); }}\n<T> T tx(Supplier<T> work) {{ return new TransactionTemplate(ctx.getBean(PlatformTransactionManager.class)).execute(status -> work.get()); }}\n",
        java_string(main),
        profile_list,
        web_application_type,
        random_port
    )
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

fn selected_jshell(project: &crate::model::Project, debug: bool) -> Result<PathBuf> {
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
            "var ctx = builder.run()",
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
    }

    #[test]
    fn console_defaults_to_dev_without_a_web_server() {
        let startup = spring_startup("com.example.DemoApplication", &[], WebMode::None);
        assert!(startup.contains("profiles(\"dev\")"));
        assert!(startup.contains("WebApplicationType.NONE"));
        assert!(!startup.contains("server.port=0"));
    }
}
