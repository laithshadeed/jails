//! Explicit, read-only PostgreSQL evidence for managed SQL queries.

use jails_project::compose::{self, PostgresConnect};
use jails_project::model::Project;
use jails_project::query_workspace::CheckedQuery;
use jails_support::Result;
use jails_support::process::{CommandSpec, Diagnostics, OutputMode};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveServices {
    Existing,
    Start,
    None,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveDescription {
    pub server_major: u32,
    pub columns: Vec<(String, String)>,
}

pub fn check(
    project: &Project,
    datasource: &str,
    services: LiveServices,
    queries: &[CheckedQuery],
    debug: bool,
) -> Result<Vec<LiveDescription>> {
    if datasource != "postgres" {
        return Err(format!(
            "unknown live datasource `{datasource}`.\n       fix: select the declared Compose datasource with `--datasource postgres`."
        )
        .into());
    }
    if services == LiveServices::None {
        return Err(
            "live SQL checking is disabled by `--services none`.\n       fix: use `--services existing`, or explicitly allow startup with `--services start`."
                .into(),
        );
    }
    if !crate::process::on_path("psql") {
        return Err(
            "psql not on PATH.\n       fix: install the PostgreSQL client and try again.".into(),
        );
    }
    let yaml = compose::read(project.root())?;
    let conn = compose::postgres_connect(&yaml).ok_or_else(|| {
        "datasource `postgres` is not declared in compose.yaml.\n       fix: run `jails add db`, or select a declared datasource."
            .to_string()
    })?;
    if services == LiveServices::Start {
        if !compose::up(project.root(), &["postgres"], debug) {
            return Err(
                "could not start datasource `postgres`.\n       fix: inspect the Compose error above, then retry."
                    .into(),
            );
        }
        wait_until_ready(&conn, debug)?;
    }

    let server_major = server_major(&conn, debug)?;
    queries
        .iter()
        .map(|query| describe(&conn, query, server_major, debug))
        .collect()
}

fn server_major(conn: &PostgresConnect, debug: bool) -> Result<u32> {
    let output = psql(conn, "SHOW server_version_num;\n", debug)?;
    let version = output.trim().parse::<u32>().map_err(|_| {
        format!(
            "postgres returned an invalid server_version_num `{}`.\n       fix: verify the selected datasource is PostgreSQL.",
            output.trim()
        )
    })?;
    Ok(version / 10_000)
}

fn describe(
    conn: &PostgresConnect,
    query: &CheckedQuery,
    server_major: u32,
    debug: bool,
) -> Result<LiveDescription> {
    let statement = jails_project::query_compiler::live_description_sql(&query.source)?;
    let script = format!("BEGIN READ ONLY;\n{statement}\n\\gdesc\nROLLBACK;\n");
    let output = psql(conn, &script, debug)?;
    let columns = output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.split_once('\t')
                .map(|(name, sql_type)| (name.to_string(), sql_type.to_string()))
                .ok_or_else(|| {
                    format!(
                        "postgres returned an invalid description row `{line}`.\n       fix: check that psql is compatible with the selected server."
                    )
                    .into()
                })
        })
        .collect::<Result<Vec<_>>>()?;
    let expected = query
        .contract
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>();
    let actual = columns
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    if actual != expected {
        return Err(format!(
            "live result columns for `{}.{}` differ (expected: {}; live: {}).\n       fix: reconcile the query or migrations before generating Java.",
            query.source.id.slice.as_str(),
            query.source.id.name.as_str(),
            expected.join(", "),
            actual.join(", ")
        )
        .into());
    }
    Ok(LiveDescription {
        server_major,
        columns,
    })
}

fn psql(conn: &PostgresConnect, sql: &str, debug: bool) -> Result<String> {
    let port = conn.port.to_string();
    let spec = CommandSpec::new("psql")
        .args([
            "-h",
            conn.host.as_str(),
            "-p",
            port.as_str(),
            "-U",
            conn.user.as_str(),
            "-d",
            conn.database.as_str(),
            "-v",
            "ON_ERROR_STOP=1",
            "--no-psqlrc",
            "-qAt",
            "-F",
            "\t",
            "-f",
            "-",
        ])
        .secret_env("PGPASSWORD", &conn.password)
        .stdin(sql.as_bytes().to_vec())
        .output(OutputMode::Capture);
    let done = crate::process::run(&spec, Diagnostics::from_flag(debug))?;
    if !done.status.success() {
        return Err(format!(
            "live PostgreSQL description failed: {}.\n       fix: reconcile the query and selected datasource, then retry.",
            String::from_utf8_lossy(&done.stderr).trim()
        )
        .into());
    }
    Ok(done.stdout_string())
}

fn wait_until_ready(conn: &PostgresConnect, debug: bool) -> Result<()> {
    let mut last = String::new();
    for attempt in 0..120 {
        match psql(conn, "SELECT 1;\n", debug && attempt == 0) {
            Ok(_) => return Ok(()),
            Err(error) => last = error.to_string(),
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!(
        "postgres at {}:{} did not accept connections within 30 seconds -- last error: {last}.\n       fix: inspect the declared service and retry when it is ready.",
        conn.host, conn.port
    )
    .into())
}
