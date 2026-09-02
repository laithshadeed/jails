//! `compose.yaml` -- the second file jails edits that the user owns.
//!
//! `add db` / `add kafka` splice a marked service block rather than rewriting
//! the file, so a hand-edited postgres stays intact when kafka is added and
//! `remove db` can take its block back out without touching kafka. Markers are
//! HTML-style comments (`# jails:db` … `# /jails:db`) so a round-trip through
//! a YAML parser is unnecessary -- the same reason pom.rs does not use an XML
//! crate.
//!
//! `jails run` starts whatever is in the file (`docker compose up -d`). Spring
//! Boot projects also get `spring-boot-docker-compose`, so `spring-boot:run`
//! does the same without going through jails.

use crate::spec::find_project_root;
use clap::ValueEnum;
use jails_support::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Compose-backed runtimes `jails start` / `jails stop` complete on. A
/// `ValueEnum` so `jails start <TAB>` lists `db` and `kafka` rather than
/// falling back to filenames. Bare `jails start` (no args) starts every
/// service in compose.yaml, including ones the user added by hand.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Runtime {
    /// PostgreSQL (the `add db` compose service)
    #[value(alias = "postgres")]
    Db,
    /// Apache Kafka (the `add kafka` compose service)
    Kafka,
    /// Redis (the `add redis` compose service)
    Redis,
}

impl Runtime {
    pub(crate) fn compose_name(self) -> &'static str {
        match self {
            Runtime::Db => "postgres",
            Runtime::Kafka => "kafka",
            Runtime::Redis => "redis",
        }
    }
}

/// A service `add` wants in compose.yaml. `body` is the mapping *under* the
/// service name, indented with four spaces; `volume` is an optional named
/// volume declared at the top level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Service {
    pub name: &'static str,
    pub marker: &'static str,
    pub body: &'static str,
    pub volume: Option<&'static str>,
}

/// A service by borrowed parts, which is all the splice needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceRef<'a> {
    pub name: &'a str,
    pub marker: &'a str,
    pub body: &'a str,
    pub volume: Option<&'a str>,
}

impl Service {
    /// This service as borrowed parts.
    pub fn borrowed(&self) -> ServiceRef<'_> {
        ServiceRef {
            name: self.name,
            marker: self.marker,
            body: self.body,
            volume: self.volume,
        }
    }
}

const CANDIDATES: [&str; 4] = [
    "compose.yaml",
    "compose.yml",
    "docker-compose.yml",
    "docker-compose.yaml",
];

/// The compose file jails should edit: an existing well-known name, otherwise
/// `compose.yaml` (the Compose spec's preferred spelling, and the first name
/// Spring Boot's docker-compose module looks for).
pub fn path(root: &Path) -> PathBuf {
    for name in CANDIDATES {
        let candidate = root.join(name);
        if candidate.is_file() {
            return candidate;
        }
    }
    root.join("compose.yaml")
}

pub fn exists(root: &Path) -> bool {
    CANDIDATES.iter().any(|name| root.join(name).is_file())
}

/// The block format lives in `codemod.rs`; the two-space indent is this
/// file's, because a marker at column zero inside a YAML mapping is a parse
/// error rather than a comment in the wrong place.
fn block(marker: &str) -> crate::codemod::Marked<'_> {
    crate::codemod::Marked::indented(marker, "  ")
}

/// `docker compose up -d` for the named services (or everything, when
/// `names` is empty). Best-effort: a machine without Docker just gets a
/// note, and `add`/`run` keep going -- the files are the capability; the
/// daemon is a convenience.
pub fn up(root: &Path, names: &[&str], debug: bool) -> bool {
    if !exists(root) {
        return true;
    }
    match invoke_compose(root, up_args(names), debug) {
        Ok(()) => true,
        Err(err) => {
            eprintln!("jails: {err}");
            false
        }
    }
}

/// `docker compose up -d` for an *exact* document, rather than for whatever
/// `compose.yaml` says by the time the effect runs.
///
/// **`--file` is the committed object and `--project-directory` is the
/// project**, and both halves are load-bearing. The effect is attempted after
/// the transition publishes, so between the two somebody may edit the live
/// file; running against what they wrote would start services this transition
/// never described, and a retry would not repeat what the first attempt did.
/// An explicit file list is also what disables compose's implicit override
/// discovery, so `compose.override.yaml` is not read. Pointing the project
/// directory at the object instead would silently relocate every relative bind
/// mount in it.
///
/// Best-effort like [`up`]: the files are the capability, the daemon is a
/// convenience.
pub fn up_document(root: &Path, document: &Path, names: &[&str], debug: bool) -> bool {
    let mut args: Vec<&str> = vec![
        "--project-directory",
        match root.to_str() {
            Some(text) => text,
            None => return false,
        },
        "--file",
        match document.to_str() {
            Some(text) => text,
            None => return false,
        },
    ];
    args.extend(up_args(names));
    match invoke_compose(root, args, debug) {
        Ok(()) => true,
        Err(err) => {
            eprintln!("jails: {err}");
            false
        }
    }
}

/// `jails start [db|kafka]...` -- require compose.yaml and Docker. No args
/// starts every service in the file.
pub fn start(services: &[Runtime], debug: bool) -> Result<()> {
    let root = find_project_root()?;
    require_compose(&root)?;
    let names = runtime_names(services);
    invoke_compose(&root, up_args(&names), debug)?;
    println!("{}", started_message(&names));
    Ok(())
}

/// `jails stop [db|kafka]...` -- require compose.yaml and Docker. No args
/// stops every service in the file.
pub fn stop_cmd(services: &[Runtime], debug: bool) -> Result<()> {
    let root = find_project_root()?;
    require_compose(&root)?;
    let names = runtime_names(services);
    invoke_compose(&root, stop_args(&names), debug)?;
    println!("{}", stopped_message(&names));
    Ok(())
}

fn runtime_names(services: &[Runtime]) -> Vec<&str> {
    services.iter().map(|s| s.compose_name()).collect()
}

/// `up -d --wait`, and the `--wait` is the whole point.
///
/// `docker compose up -d` returns when the container is *running*, which for
/// PostgreSQL is several seconds before it accepts a TCP connection. `jails
/// run` starts the services and then starts Spring Boot, so on a cold start
/// the application raced the database it had just asked for and died on
/// "Connection refused" -- intermittently, which is the worst way to fail.
///
/// Every service jails writes declares a `healthcheck`, so `--wait` waits for
/// *healthy* rather than merely started: `pg_isready` for PostgreSQL,
/// `redis-cli ping` for Redis, `kafka-topics.sh --list` for the broker. The
/// timeout is bounded so a service that never becomes healthy is a failure
/// with a message rather than a command that hangs.
fn up_args<'a>(names: &'a [&str]) -> Vec<&'a str> {
    let mut args = vec!["up", "-d", "--wait", "--wait-timeout", "120"];
    args.extend(names.iter().copied());
    args
}

fn stop_args<'a>(names: &'a [&str]) -> Vec<&'a str> {
    let mut args = vec!["stop"];
    args.extend(names.iter().copied());
    args
}

fn require_compose(root: &Path) -> Result<()> {
    if exists(root) {
        Ok(())
    } else {
        Err("no compose.yaml -- run `jails add db` (or kafka) first".into())
    }
}

fn started_message(names: &[&str]) -> String {
    if names.is_empty() {
        "started compose services".into()
    } else {
        format!("started {}", names.join(", "))
    }
}

fn stopped_message(names: &[&str]) -> String {
    if names.is_empty() {
        "stopped compose services".into()
    } else {
        format!("stopped {}", names.join(", "))
    }
}

fn invoke_compose(root: &Path, args: Vec<&str>, debug: bool) -> Result<()> {
    let mut cmd = compose_command().ok_or_else(|| {
        "docker not on PATH -- install Docker, or run `docker compose up -d` yourself".to_string()
    })?;
    cmd.args(&args).current_dir(root);
    if debug {
        jails_support::debug_cmd(&cmd);
    }
    let status = cmd
        .status()
        .map_err(|e| format!("failed to run docker compose: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("docker compose {} exited with {status}", args.join(" ")).into())
    }
}

/// `docker compose` (v2, a CLI plugin) or the standalone `docker-compose`.
///
/// Resolved in `process`, which is also what `doctor` probes with: a second
/// copy that hardcodes `docker` reports Docker missing on a machine with only
/// the standalone binary, where `jails start` works.
fn compose_command() -> Option<Command> {
    let (program, prefix) = crate::process::compose_program()?;
    let mut cmd = Command::new(program);
    cmd.args(prefix);
    Some(cmd)
}

pub fn read(root: &Path) -> Result<String> {
    let path = path(root);
    if !path.is_file() {
        return Ok(String::new());
    }
    Ok(std::fs::read_to_string(&path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?)
}

/// Connection parameters for the compose postgres that `jails add db` wrote.
/// `None` when the file has no postgres service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresConnect {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}

impl PostgresConnect {
    pub fn defaults() -> Self {
        Self {
            host: "localhost".into(),
            port: 5432,
            user: "app".into(),
            password: "app".into(),
            database: "app".into(),
        }
    }
}

pub fn postgres_connect(text: &str) -> Option<PostgresConnect> {
    if !has_postgres_service(text) {
        return None;
    }
    let mut c = PostgresConnect::defaults();
    for line in text.lines() {
        let t = line.trim();
        if let Some(v) = yaml_scalar(t, "POSTGRES_DB:") {
            c.database = v;
        } else if let Some(v) = yaml_scalar(t, "POSTGRES_USER:") {
            c.user = v;
        } else if let Some(v) = yaml_scalar(t, "POSTGRES_PASSWORD:") {
            c.password = v;
        } else if let Some(port) = host_port_for_container(t, 5432) {
            c.port = port;
        }
    }
    Some(c)
}

/// Whether a compose file carries the block jails writes for `marker`.
///
/// Public because `doctor` asks the same question about kafka, and answering
/// it there with `yaml.contains("# jails:kafka")` is a second place that
/// knows both the marker format *and* this file's two-space indent. The indent
/// is this module's fact, so the question belongs here.
pub fn declares(text: &str, marker: &str) -> bool {
    block(marker).present_in(text)
}

fn has_postgres_service(text: &str) -> bool {
    // Through `declares`, not `contains`: a substring match reads `# jails:dbx`
    // as this block, which is the prefix collision the marked block's exact
    // line match exists for.
    declares(text, "db")
        || text.lines().any(|l| {
            let t = l.trim_end();
            t == "  postgres:" || t.starts_with("  postgres:")
        })
}

fn yaml_scalar(trimmed: &str, key: &str) -> Option<String> {
    let rest = trimmed.strip_prefix(key)?.trim();
    if rest.is_empty() {
        return None;
    }
    Some(rest.trim_matches(|c| c == '"' || c == '\'').to_string())
}

/// Host port from a compose mapping like `- "5432:5432"` or `- "15432:5432"`.
fn host_port_for_container(trimmed: &str, container_port: u16) -> Option<u16> {
    let rest = trimmed.strip_prefix('-')?.trim();
    let rest = rest.trim_matches(|c| c == '"' || c == '\'');
    let (host, container) = rest.split_once(':')?;
    let container = container.split('/').next()?;
    if container.parse::<u16>().ok()? != container_port {
        return None;
    }
    host.parse().ok()
}

#[cfg(test)]
mod tests {}
