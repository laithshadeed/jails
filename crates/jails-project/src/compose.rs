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
    pub fn compose_name(self) -> &'static str {
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

/// Mailpit: an SMTP sink with a web inbox, for development.
///
/// The same image the integration test starts, so what you read in the browser
/// at :8025 and what the test reads over POP3 are the same server.
pub const MAILPIT: Service = Service {
    name: "mailpit",
    marker: "mail",
    body: r#"    image: axllent/mailpit:v1.21
    environment:
      MP_SMTP_AUTH_ACCEPT_ANY: "true"
      MP_SMTP_AUTH_ALLOW_INSECURE: "true"
      MP_POP3_AUTH: user:pass
    ports:
      - "1025:1025"
      - "1110:1110"
      - "8025:8025"
"#,
    volume: None,
};

pub const POSTGRES: Service = Service {
    name: "postgres",
    marker: "db",
    body: r#"    image: postgres:17-alpine
    environment:
      POSTGRES_DB: app
      POSTGRES_USER: app
      POSTGRES_PASSWORD: app
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U app -d app"]
      interval: 2s
      timeout: 5s
      retries: 10
    volumes:
      - postgres-data:/var/lib/postgresql/data
"#,
    volume: Some("postgres-data"),
};

pub const REDIS: Service = Service {
    name: "redis",
    marker: "redis",
    // No volume: this is a cache. Persisting it across `jails stop`/`start`
    // would hide the one bug a cache reliably has -- code that only works
    // because something was already cached.
    body: r#"    image: redis:7-alpine
    ports:
      - "6379:6379"
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 2s
      timeout: 5s
      retries: 10
"#,
    volume: None,
};

pub const KAFKA: Service = Service {
    name: "kafka",
    marker: "kafka",
    // Official Apache image, KRaft combined mode, host port 9092. Fixed ports
    // rather than ephemeral: Kafka has to advertise the listener the client
    // will dial, and a mapped random port cannot be known from compose.yaml.
    body: r#"    image: apache/kafka:4.1.0
    hostname: kafka
    ports:
      - "9092:9092"
    environment:
      KAFKA_NODE_ID: 1
      KAFKA_PROCESS_ROLES: broker,controller
      KAFKA_LISTENERS: CONTROLLER://:29093,PLAINTEXT_HOST://:9092,PLAINTEXT://:19092
      KAFKA_ADVERTISED_LISTENERS: PLAINTEXT_HOST://localhost:9092,PLAINTEXT://kafka:19092
      KAFKA_LISTENER_SECURITY_PROTOCOL_MAP: CONTROLLER:PLAINTEXT,PLAINTEXT:PLAINTEXT,PLAINTEXT_HOST:PLAINTEXT
      KAFKA_INTER_BROKER_LISTENER_NAME: PLAINTEXT
      KAFKA_CONTROLLER_LISTENER_NAMES: CONTROLLER
      KAFKA_CONTROLLER_QUORUM_VOTERS: 1@kafka:29093
      CLUSTER_ID: 4L6g3nShT-eMCtK--X86sw
      KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR: 1
      KAFKA_TRANSACTION_STATE_LOG_REPLICATION_FACTOR: 1
      KAFKA_TRANSACTION_STATE_LOG_MIN_ISR: 1
      KAFKA_GROUP_INITIAL_REBALANCE_DELAY_MS: 0
      KAFKA_LOG_DIRS: /tmp/kraft-combined-logs
    healthcheck:
      test: ["CMD-SHELL", "/opt/kafka/bin/kafka-topics.sh --bootstrap-server localhost:9092 --list"]
      interval: 5s
      timeout: 10s
      retries: 10
"#,
    volume: None,
};

const HEADER: &str = "\
# Local development services. `jails add` / `jails remove` own the marked
# blocks; `jails run` starts everything here.
";

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

/// True when any jails-managed (or otherwise declared) service is still in
/// the file -- used to keep or drop `spring-boot-docker-compose`.
pub fn has_services(text: &str) -> bool {
    service_names(text).next().is_some()
}

/// Every top-level service name a compose document declares.
///
/// The one reader of a compose document's service list, so §R3.3's
/// `all_service_names` and `has_services` cannot disagree about what is in a
/// file. Markers do not participate: a `# jails:` line is a comment, and the
/// document a runtime reconciliation stops services from is the document as
/// compose itself would read it.
pub fn all_service_names(text: &str) -> Vec<&str> {
    service_names(text).collect()
}

fn service_names(text: &str) -> impl Iterator<Item = &str> {
    let services = section_body(text, "services");
    services.lines().filter_map(|line| {
        let rest = line.strip_prefix("  ")?;
        if rest.starts_with('#') || rest.starts_with(' ') || rest.is_empty() {
            return None;
        }
        rest.strip_suffix(':')
            .map(str::trim)
            .filter(|s| !s.is_empty())
    })
}

/// Body of a top-level mapping (`services:`, `volumes:`) up to the next
/// top-level key or EOF. Missing section -> empty.
fn section_body<'a>(text: &'a str, key: &str) -> &'a str {
    let header = format!("{key}:");
    let Some(header_line) = text.lines().position(|line| line.trim_end() == header) else {
        return "";
    };
    let Some(line_start) = line_offset(text, header_line) else {
        return "";
    };
    let header_len = text.lines().nth(header_line).map(str::len).unwrap_or(0);
    let mut start = line_start + header_len;
    if text[start..].starts_with('\n') {
        start += 1;
    }
    let end = next_top_level_after(text, header_line).unwrap_or(text.len());
    &text[start.min(text.len())..end.min(text.len())]
}

fn line_offset(text: &str, line_idx: usize) -> Option<usize> {
    let mut offset = 0;
    for (i, line) in text.lines().enumerate() {
        if i == line_idx {
            return Some(offset);
        }
        offset += line.len() + 1;
    }
    None
}

/// The block format lives in `codemod.rs`; the two-space indent is this
/// file's, because a marker at column zero inside a YAML mapping is a parse
/// error rather than a comment in the wrong place.
fn block(marker: &str) -> crate::codemod::Marked<'_> {
    crate::codemod::Marked::indented(marker, "  ")
}

fn marked_block(svc: ServiceRef<'_>) -> String {
    // `svc.body` already carries its own two-space nesting under the service
    // name, so it is rendered as-is beneath a line the block indents.
    let mut inner = format!("{}:\n", svc.name);
    for line in svc.body.lines() {
        inner.push_str(line.strip_prefix("  ").unwrap_or(line));
        inner.push('\n');
    }
    block(svc.marker).render(&inner)
}

fn marked_volume(name: &str, marker: &str) -> String {
    block(marker).render(&format!("{name}:\n"))
}

pub fn has_service(text: &str, svc: &Service) -> bool {
    has_service_ref(text, svc.borrowed())
}

pub fn has_service_ref(text: &str, svc: ServiceRef<'_>) -> bool {
    block(svc.marker).present_in(text)
        || text
            .lines()
            .any(|line| line.trim_end() == format!("  {}:", svc.name))
}

/// Splice `svc` into `text`. `Ok(None)` when it is already there.
pub fn add_service(text: &str, svc: &Service) -> Option<String> {
    add_service_ref(text, svc.borrowed())
}

/// The same splice from borrowed parts, for a service that did not come from
/// a literal in this binary. One splice, two callers — see
/// `pom::add_dependency_ref` for the same reason.
pub fn add_service_ref(text: &str, svc: ServiceRef<'_>) -> Option<String> {
    if !text.trim().is_empty() && has_service_ref(text, svc) {
        return None;
    }
    if text.trim().is_empty() {
        return Some(render_new(&[svc]));
    }
    let mut out = insert_under(text, "services", &marked_block(svc));
    if let Some(volume) = svc.volume {
        out = ensure_volume(&out, volume, svc.marker);
    }
    Some(out)
}

fn render_new(services: &[ServiceRef<'_>]) -> String {
    let mut out = String::from(HEADER);
    out.push_str("services:\n");
    let mut volumes: Vec<(&str, &str)> = Vec::new();
    for svc in services {
        out.push_str(&marked_block(*svc));
        if let Some(volume) = svc.volume {
            volumes.push((volume, svc.marker));
        }
    }
    if !volumes.is_empty() {
        out.push_str("volumes:\n");
        for (name, marker) in volumes {
            out.push_str(&marked_volume(name, marker));
        }
    }
    out
}

/// Insert `block` as a child of the named top-level key, creating the key if
/// it is missing. Insertion is just above the next top-level key so sibling
/// sections stay in place.
fn insert_under(text: &str, key: &str, block: &str) -> String {
    let header = format!("{key}:");
    if let Some(header_line) = text.lines().position(|l| l.trim_end() == header) {
        let insert_at = next_top_level_after(text, header_line).unwrap_or(text.len());
        let mut out = String::with_capacity(text.len() + block.len());
        out.push_str(&text[..insert_at]);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(block);
        out.push_str(&text[insert_at..]);
        return out;
    }
    let mut out = text.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&header);
    out.push('\n');
    out.push_str(block);
    out
}

fn next_top_level_after(text: &str, header_line: usize) -> Option<usize> {
    let mut offset = 0;
    for (i, line) in text.lines().enumerate() {
        let line_start = offset;
        offset += line.len() + 1;
        if i <= header_line {
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if !line.starts_with(' ') && !line.starts_with('#') {
            return Some(line_start);
        }
    }
    None
}

fn ensure_volume(text: &str, name: &str, marker: &str) -> String {
    if block(marker).present_in(text)
        && section_body(text, "volumes").contains(&format!("  {name}:"))
    {
        return text.to_string();
    }
    insert_under(text, "volumes", &marked_volume(name, marker))
}

/// Remove the marked block for `svc`. Returns `Some("")` when the file would
/// have no services left (caller should delete it), `None` when the service
/// was not there.
pub fn remove_service(text: &str, svc: &Service) -> Option<String> {
    remove_service_ref(text, svc.borrowed())
}

/// The same removal, from parts the caller owns.
///
/// A service being retired is named by a *recorded* resource rather than by
/// one of this module's constants, so its marker and volume arrive as runtime
/// strings. Leaking them to fit `Service`'s `&'static str` fields would be a
/// memory leak in the name of a type signature.
pub fn remove_service_ref(text: &str, svc: ServiceRef<'_>) -> Option<String> {
    let stripped = strip_marked(text, svc.marker)?;
    let stripped = if svc.volume.is_some() {
        strip_marked(&stripped, svc.marker).unwrap_or(stripped)
    } else {
        stripped
    };
    let stripped = drop_empty_section(&stripped, "volumes");
    if !has_services(&stripped) {
        return Some(String::new());
    }
    Some(stripped)
}

fn strip_marked(text: &str, marker: &str) -> Option<String> {
    block(marker).strip_from(text)
}

fn drop_empty_section(text: &str, key: &str) -> String {
    let header = format!("{key}:");
    let Some(header_line) = text.lines().position(|l| l.trim_end() == header) else {
        return text.to_string();
    };
    let body = section_body(text, key);
    let meaningful = body.lines().any(|l| {
        let t = l.trim();
        !t.is_empty() && !t.starts_with('#')
    });
    if meaningful {
        return text.to_string();
    }
    let start = line_offset(text, header_line).unwrap_or(0);
    let end = next_top_level_after(text, header_line).unwrap_or(text.len());
    let mut out = String::new();
    out.push_str(&text[..start]);
    out.push_str(&text[end..]);
    out
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

pub fn stop(root: &Path, names: &[&str], debug: bool) -> bool {
    if !exists(root) || names.is_empty() {
        return true;
    }
    match invoke_compose(root, stop_args(names), debug) {
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

fn up_args<'a>(names: &'a [&str]) -> Vec<&'a str> {
    let mut args = vec!["up", "-d"];
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
        Err(format!(
            "docker compose {} exited with {status}",
            args.join(" ")
        ))
    }
}

/// `docker compose` (v2, a CLI plugin) or the standalone `docker-compose`.
///
/// Resolved in `process`, which is also what `doctor` probes with -- the two
/// had separate copies, and `doctor` hardcoded `docker`, so on a machine with
/// only the standalone binary `jails start` worked while `doctor` reported
/// Docker missing.
fn compose_command() -> Option<Command> {
    let (program, prefix) = crate::process::compose_program()?;
    let mut cmd = Command::new(program);
    cmd.args(prefix);
    Some(cmd)
}

/// Used by `add` after a failed docker start so the message stays one line.
pub fn missing_docker_hint(names: &[&str]) -> String {
    if names.is_empty() {
        "jails start".into()
    } else if names.len() == 1 && names[0] == "postgres" {
        "jails start db".into()
    } else if names.len() == 1 && names[0] == "kafka" {
        "jails start kafka".into()
    } else {
        format!("jails start {}", names.join(" "))
    }
}

pub fn read(root: &Path) -> Result<String> {
    let path = path(root);
    if !path.is_file() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))
}

pub fn write(root: &Path, text: &str) -> Result<()> {
    let path = path(root);
    if text.is_empty() {
        if path.is_file() {
            jails_support::apply::remove(&path)
                .map_err(|e| format!("failed to remove {}: {e}", path.display()))?;
        }
        return Ok(());
    }
    crate::apply::put(&path, text)
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

fn has_postgres_service(text: &str) -> bool {
    text.contains("# jails:db")
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
mod tests {
    use super::*;

    #[test]
    fn add_service_to_empty_file_renders_header_and_markers() {
        let out = add_service("", &POSTGRES).unwrap();
        assert!(out.starts_with("# Local development services."));
        assert!(out.contains("# jails:db"));
        assert!(out.contains("# /jails:db"));
        assert!(out.contains("image: postgres:17-alpine"));
        assert!(out.contains("POSTGRES_DB: app"));
        assert!(out.contains("postgres-data:"));
        assert!(!out.contains("hibernate"), "no ORM in compose either");
    }

    #[test]
    fn add_service_is_idempotent() {
        let once = add_service("", &POSTGRES).unwrap();
        assert!(add_service(&once, &POSTGRES).is_none());
    }

    #[test]
    fn capabilities_stack_in_one_compose_file() {
        let db = add_service("", &POSTGRES).unwrap();
        let both = add_service(&db, &KAFKA).unwrap();
        assert!(both.contains("  postgres:"));
        assert!(both.contains("  kafka:"));
        assert!(both.contains("# jails:db"));
        assert!(both.contains("# jails:kafka"));
        // Postgres comments and env survive the kafka splice.
        assert!(both.contains("POSTGRES_PASSWORD: app"));
        assert!(both.contains("apache/kafka:4.1.0"));
        // Top-level volumes stay after both services (the postgres service
        // also has an indented `volumes:` key, so match the document one).
        let services_at = both.find("\nservices:").unwrap();
        let kafka_at = both.find("\n  kafka:").unwrap();
        let volumes_at = both.find("\nvolumes:").unwrap();
        assert!(services_at < kafka_at);
        assert!(kafka_at < volumes_at);
    }

    #[test]
    fn remove_one_service_leaves_the_other() {
        let both = add_service(&add_service("", &POSTGRES).unwrap(), &KAFKA).unwrap();
        let kafka_only = remove_service(&both, &POSTGRES).unwrap();
        assert!(!kafka_only.contains("postgres:"));
        assert!(!kafka_only.contains("postgres-data"));
        assert!(kafka_only.contains("  kafka:"));
        assert!(!kafka_only.contains("volumes:"), "{kafka_only}");
    }

    #[test]
    fn remove_last_service_yields_an_empty_file() {
        let db = add_service("", &POSTGRES).unwrap();
        let gone = remove_service(&db, &POSTGRES).unwrap();
        assert!(gone.is_empty());
    }

    #[test]
    fn remove_is_a_no_op_when_the_service_is_absent() {
        let db = add_service("", &POSTGRES).unwrap();
        assert!(remove_service(&db, &KAFKA).is_none());
    }

    #[test]
    fn add_preserves_a_hand_written_comment() {
        let starter = "# keep me\nservices:\n  redis:\n    image: redis:7\n";
        let out = add_service(starter, &POSTGRES).unwrap();
        assert!(out.contains("# keep me"));
        assert!(out.contains("  redis:"));
        assert!(out.contains("  postgres:"));
    }

    #[test]
    fn postgres_connect_reads_the_jails_db_block() {
        let yaml = add_service("", &POSTGRES).unwrap();
        assert_eq!(postgres_connect(&yaml), Some(PostgresConnect::defaults()));
        let kafka_only = add_service("", &KAFKA).unwrap();
        assert!(postgres_connect(&kafka_only).is_none());
        assert!(postgres_connect("").is_none());

        let remapped = yaml.replace("\"5432:5432\"", "\"15432:5432\"");
        let c = postgres_connect(&remapped).unwrap();
        assert_eq!(c.port, 15432);
        assert_eq!(c.user, "app");
    }

    #[test]
    fn kafka_advertises_localhost_on_a_fixed_port() {
        assert!(KAFKA.body.contains("PLAINTEXT_HOST://localhost:9092"));
        assert!(KAFKA.body.contains("\"9092:9092\""));
        assert!(
            !KAFKA.body.contains("zookeeper"),
            "KRaft, not a ZooKeeper sidecar"
        );
    }

    #[test]
    fn runtime_maps_capability_names_onto_compose_services() {
        assert_eq!(Runtime::Db.compose_name(), "postgres");
        assert_eq!(Runtime::Kafka.compose_name(), "kafka");
        assert_eq!(missing_docker_hint(&["postgres"]), "jails start db");
        assert_eq!(missing_docker_hint(&["kafka"]), "jails start kafka");
        assert_eq!(missing_docker_hint(&[]), "jails start");
    }
}
