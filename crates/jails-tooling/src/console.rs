//! `jails db` / `jails console` -- the Rails-shaped interactive commands.
//!
//! `db` is `rails dbconsole`: exec `psql` against the compose postgres
//! `add db` started, or `sqlite3` when given a file. `console` is `jshell`
//! with the project's classpath. That is not a Spring-booted REPL -- Java
//! has no equivalent -- but it is the closest thing that does not invent
//! a framework.

use crate::compose;
use crate::generate::find_project_root;
use crate::run;
use jails_support::Result;
use std::fs;
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
    if run::find_on_path(name) {
        return Ok(PathBuf::from(name));
    }
    Err(format!(
        "{name} not on PATH -- install the {name} client and try again"
    ))
}

/// `jshell` with compiled classes and Maven dependencies on the classpath.
pub fn console(no_build: bool, args: &[String], debug: bool) -> Result<()> {
    let root = find_project_root()?;
    // The classpath comes from `mvn dependency:build-classpath`; without it
    // jshell starts with nothing of the project on it.
    crate::build::require_maven_at(&root, "console")?;
    let jshell = find_jshell().ok_or_else(|| {
        "jshell not on PATH -- it ships with the JDK (`JAVA_HOME/bin/jshell`)".to_string()
    })?;
    if !no_build {
        let mut compile = Command::new(crate::maven::binary(&root));
        compile.arg("compile").current_dir(&root);
        run::run_inherited(compile, debug)?;
    }
    let classpath = project_classpath(&root, debug)?;
    let mut cmd = Command::new(&jshell);
    cmd.args(["--class-path", &classpath])
        .args(args)
        .current_dir(&root);
    run::run_inherited(cmd, debug)
}

fn find_jshell() -> Option<PathBuf> {
    if run::find_on_path("jshell") {
        return Some(PathBuf::from("jshell"));
    }
    let home = std::env::var_os("JAVA_HOME")?;
    let bin = PathBuf::from(home).join("bin").join("jshell");
    bin.is_file().then_some(bin)
}

fn project_classpath(root: &Path, debug: bool) -> Result<String> {
    let out = root.join("target/jails-classpath");
    if let Some(parent) = out.parent() {
        jails_support::apply::ensure_directory(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    let mut mvn = Command::new(crate::maven::binary(root));
    mvn.args([
        "-q",
        "dependency:build-classpath",
        &format!("-Dmdep.outputFile={}", out.display()),
        "-DincludeScope=runtime",
    ])
    .current_dir(root);
    run::run_inherited(mvn, debug)?;

    let mut entries = vec![root.join("target/classes")];
    if let Ok(deps) = fs::read_to_string(&out) {
        let deps = deps.trim();
        if !deps.is_empty() {
            entries.extend(std::env::split_paths(deps));
        }
    }
    std::env::join_paths(entries)
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| format!("failed to join classpath: {e}"))
}
