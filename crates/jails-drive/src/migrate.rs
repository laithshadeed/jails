//! `jails migrate --check` -- apply every migration to a scratch database and
//! report the first thing that fails.
//!
//! ## Why this is not a `doctor` check
//!
//! `doctor` is read-only by contract: it must never start, stop or write
//! anything, so it stays safe to run mid-debug. Applying migrations writes by
//! definition. `doctor` can only ever answer "are there .sql files and will
//! something run them", which is a different question from "do they work": a
//! misspelled type or column name in a file nothing has parsed says nothing
//! until the migration runs for real.
//!
//! ## Why a scratch database rather than a throwaway container
//!
//! A container would be more isolated and a great deal slower, and the
//! isolation that matters here is from **your data**, not from your postgres.
//! A uniquely-named database created and dropped around the run gives that:
//! the dev database is untouched, and the migrations run against the same
//! server, extensions and version they will run against for real -- which a
//! generic `postgres:latest` container would not guarantee.
//!
//! Migrations are applied in the same order Flyway would use, one statement
//! batch per file, stopping at the first failure and naming the file.

use jails_support::Result;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::compose;
use crate::find_project_root;
use jails_project::model::Project;
use jails_protocol::database::MigrationInputV1;
use jails_support::codec::sha256;
use jails_support::identity::ObjectId;

/// Apply one committed migration set to an already available datasource. No
/// service is started, and every live migration path is checked against the
/// committed descriptor before Flyway sees any bytes.
pub fn apply_effect(
    project: &Project,
    datasource: &str,
    migrations: &[MigrationInputV1],
    debug: bool,
) -> Result<()> {
    use crate::process::{CommandSpec, Diagnostics, OutputMode};

    if migrations.is_empty() {
        return Err(concat!(
            "the migration effect contains no inputs.\n       ",
            "fix: retry from the command that generated the migration receipt."
        )
        .into());
    }
    let candidate = crate::datasource::select(
        project,
        datasource,
        crate::datasource::LiveServices::Existing,
    )?;
    if !crate::process::on_path("flyway") {
        return Err(concat!(
            "Flyway is not on PATH, so the committed migration effect was not applied.\n       ",
            "fix: install the Flyway CLI, then retry the recorded effect."
        )
        .into());
    }

    let store = jails_commit::store::Store::at(project.root());
    let scratch = jails_support::scratch::ScratchDir::in_temp("jails-flyway-effect")?;
    for migration in migrations {
        let live_path = project.root().join(migration.path.as_str());
        let live = std::fs::read(&live_path).map_err(|error| {
            format!(
                "migration-effect-stale: `{}` no longer has the committed bytes ({error}).\n       fix: restore the committed migration history before retrying the effect.",
                migration.path
            )
        })?;
        let live_digest = ObjectId::from_bytes(sha256(&live));
        if live_digest != migration.content_digest {
            return Err(format!(
                "migration-effect-stale: `{}` changed after the effect was prepared.\n       fix: restore its committed bytes before retrying; published migrations are append-only.",
                migration.path
            )
            .into());
        }
        let frozen = jails_commit::store::read_object(&store.objects(), &migration.content_digest)?;
        let file_name = migration
            .path
            .as_str()
            .rsplit('/')
            .next()
            .ok_or_else(|| format!("migration `{}` has no file name", migration.path))?;
        jails_support::apply::put_in_scratch(scratch.path().join(file_name), frozen)?;
    }

    let connection = &candidate.connection;
    let jdbc = format!(
        "jdbc:postgresql://{}:{}/{}",
        connection.host, connection.port, connection.database
    );
    let locations = format!("filesystem:{}", scratch.path().display());
    let spec = CommandSpec::new("flyway")
        .arg("migrate")
        .env("FLYWAY_URL", &jdbc)
        .env("FLYWAY_USER", &connection.user)
        .secret_env("FLYWAY_PASSWORD", &connection.password)
        .env("FLYWAY_LOCATIONS", &locations)
        .output(OutputMode::Capture);
    let done = crate::process::run(&spec, Diagnostics::from_flag(debug))?;
    if !done.status.success() {
        return Err(format!(
            "migration effect failed for datasource at {}.\n       fix: inspect Flyway's database-side history and retry the recorded effect after correcting the forward migration.",
            candidate.endpoint_label()
        )
        .into());
    }
    scratch.close()
}

/// Apply the project's migrations to a scratch database, then drop it.
///
/// Returns an *empty* `Err` when a migration fails, so `main` prints no
/// redundant `jails: ` line over the report this already printed -- the same
/// convention `doctor` uses.
pub fn check(no_start: bool, debug: bool) -> Result<()> {
    let root = find_project_root()?;
    let dir = root.join("src/main/resources/db/migration");
    let migrations = ordered_migrations(&dir)?;
    if migrations.is_empty() {
        println!("no migrations in {}", rel(&root, &dir));
        return Ok(());
    }

    let yaml = compose::read(&root)?;
    let conn = compose::postgres_connect(&yaml).ok_or_else(|| {
        "no postgres in compose.yaml -- `jails add db` first, or there is nothing to apply \
         migrations to"
            .to_string()
    })?;
    if !crate::process::on_path("psql") {
        return Err("psql not on PATH -- install the postgres client and try again".into());
    }
    if !no_start {
        // A reachable server is already ready; asking Compose to start a
        // second postgres can only produce a misleading port-conflict error.
        // The quiet probe is captured, so an ordinary not-ready result is not
        // printed as a failed command before startup gets a chance to help.
        if psql(&conn, &conn.database, "select 1", false).is_err() {
            compose::up(&root, &["postgres"], debug);
        }
        wait_until_ready(&conn, debug)?;
    }

    // Named for this run so a crashed earlier run cannot collide with this
    // one, and so a leftover is obviously jails' rather than someone's.
    let scratch = format!("jails_migration_check_{}", std::process::id());

    psql(
        &conn,
        &conn.database,
        &format!("create database {scratch}"),
        debug,
    )
    .map_err(|e| format!("could not create the scratch database: {e}"))?;

    let mut failure = None;
    for migration in &migrations {
        let name = migration
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let sql = std::fs::read_to_string(migration)
            .map_err(|e| format!("failed to read {}: {e}", migration.display()))?;
        match psql(&conn, &scratch, &sql, debug) {
            Ok(()) => println!("  ok    {name}"),
            Err(error) => {
                println!("  FAIL  {name}");
                failure = Some((name.to_string(), error));
                break;
            }
        }
    }

    // Dropped whether or not the run succeeded: leaving the scratch database
    // behind on failure would make the *next* run collide with it.
    let _ = psql(
        &conn,
        &conn.database,
        &format!("drop database if exists {scratch}"),
        debug,
    );

    match failure {
        None => {
            println!(
                "\n{} migration(s) applied cleanly to a scratch database.",
                migrations.len()
            );
            Ok(())
        }
        Some((name, error)) => {
            println!("\n{name} did not apply:\n");
            for line in error.lines() {
                println!("  {line}");
            }
            println!(
                "\nfix: edit {name} and re-run `jails migrate --check`. Migrations are \
                 forward-only, so fix the file rather than adding one that undoes it -- \
                 nothing has run anywhere yet."
            );
            Err(jails_support::Failure::Reported)
        }
    }
}

/// Migrations in the order Flyway applies them.
///
/// Flyway orders by version, and `V10` sorts before `V9` as a string -- so
/// this compares the numeric version, and falls back to the filename for
/// anything that does not parse as one (a repeatable `R__` migration, which
/// Flyway runs after the versioned ones anyway).
fn ordered_migrations(dir: &Path) -> Result<Vec<PathBuf>> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(Vec::new());
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sql"))
        .collect();
    found.sort_by_key(|path| {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        (version_of(&name), name)
    });
    Ok(found)
}

/// The numeric version in `V001__description.sql`. `None` sorts last.
fn version_of(file_name: &str) -> Option<u32> {
    file_name
        .strip_prefix('V')?
        .split("__")
        .next()?
        .parse()
        .ok()
}

fn psql(conn: &compose::PostgresConnect, database: &str, sql: &str, debug: bool) -> Result<()> {
    use crate::process::{CommandSpec, Diagnostics, OutputMode};

    let spec = CommandSpec::new("psql")
        .args([
            "-h",
            &conn.host,
            "-p",
            &conn.port.to_string(),
            "-U",
            &conn.user,
            "-d",
            database,
            // Stop at the first error and report it, rather than plodding on
            // and reporting the twelve that follow from it.
            "-v",
            "ON_ERROR_STOP=1",
            "--no-psqlrc",
            "-q",
            "-f",
            "-",
        ])
        // `secret_env`, so `--debug` prints `PGPASSWORD=<redacted>`: the
        // reader needs to know it was set, never what it is.
        .secret_env("PGPASSWORD", &conn.password)
        .stdin(sql.as_bytes().to_vec())
        .output(OutputMode::Capture);

    // The executor prints and then runs. `--debug` is observability, never a
    // mode that skips the work: returning early here would report "applied
    // cleanly" over SQL that had not been near a database.
    let done = crate::process::run(&spec, Diagnostics::from_flag(debug))?;
    if done.status.success() {
        return Ok(());
    }
    Err(jails_support::Failure::Told(
        String::from_utf8_lossy(&done.stderr).trim().to_string(),
    ))
}

/// Wait for postgres to answer, rather than treating a container that has been
/// *started* as a database that is *listening*.
///
/// `compose up` returns once the container is running, which is a few seconds
/// before postgres accepts a connection -- and the very next thing this
/// command does is connect. Connecting too early fails with "server closed the
/// connection unexpectedly", which reads like a broken database rather than
/// one still starting up, and sends the reader to the migrations, which are
/// fine.
///
/// Only when jails started the service itself. Under `--no-start` the caller
/// has asserted the database is already up, and half a minute of polling a
/// port with nothing behind it is a worse answer than the connection error.
fn wait_until_ready(conn: &compose::PostgresConnect, debug: bool) -> Result<()> {
    const ATTEMPTS: u32 = 120;
    const PAUSE: Duration = Duration::from_millis(250);

    Ok(wait_for(
        // Only the first probe is announced: a poll that prints the same
        // command 120 times buries the run it is part of.
        |attempt| psql(conn, &conn.database, "select 1", debug && attempt == 0),
        ATTEMPTS,
        PAUSE,
    )
    .map_err(|last| {
        format!(
            "postgres at {}:{} did not accept connections within {} seconds -- last error: {last}",
            conn.host,
            conn.port,
            u128::from(ATTEMPTS) * PAUSE.as_millis() / 1000,
        )
    })?)
}

/// Retry `probe` until it succeeds or the budget runs out.
///
/// Reports the **last** error rather than the first: the first one is what a
/// starting service looks like, and the last one is the state the caller is
/// actually in.
fn wait_for(
    mut probe: impl FnMut(u32) -> Result<()>,
    attempts: u32,
    pause: Duration,
) -> Result<()> {
    let mut last: Result<()> = Err("no attempt was made".into());
    for attempt in 0..attempts {
        last = probe(attempt);
        if last.is_ok() {
            return last;
        }
        // Not after the final attempt: a pause nothing follows is dead time
        // added to the failure the caller is already waiting for.
        if attempt + 1 < attempts {
            std::thread::sleep(pause);
        }
    }
    last
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flyway orders by version number. String order puts `V10` before `V9`,
    /// which would apply migrations in an order that has never been tested
    /// and may not even work.
    #[test]
    fn migrations_are_ordered_numerically_not_lexically() {
        let dir = jails_support::scratch::ScratchDir::in_temp("jails-migrate-order")
            .unwrap()
            .keep();
        for name in ["V9__nine.sql", "V10__ten.sql", "V1__one.sql"] {
            std::fs::write(dir.join(name), "select 1;").unwrap();
        }
        let ordered: Vec<String> = ordered_migrations(&dir)
            .unwrap()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(ordered, vec!["V1__one.sql", "V9__nine.sql", "V10__ten.sql"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn version_of_reads_the_number_and_ignores_the_description() {
        assert_eq!(version_of("V001__create_rewards.sql"), Some(1));
        assert_eq!(version_of("V42__x.sql"), Some(42));
        // A repeatable migration has no version; Flyway runs those last.
        assert_eq!(version_of("R__views.sql"), None);
    }

    #[test]
    fn a_missing_migration_directory_is_empty_rather_than_an_error() {
        assert!(
            ordered_migrations(Path::new("/nonexistent/db/migration"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_database_that_answers_on_the_first_probe_is_not_polled_again() {
        let mut probes = 0;
        let outcome = wait_for(
            |_| {
                probes += 1;
                Ok(())
            },
            120,
            Duration::from_millis(250),
        );
        assert!(outcome.is_ok());
        // 250ms x 120 is half a minute; a second probe here would mean every
        // healthy run paid for the unhealthy one.
        assert_eq!(probes, 1);
    }

    #[test]
    fn a_database_that_starts_slowly_is_waited_for_rather_than_failed() {
        let mut probes = 0;
        let outcome = wait_for(
            |_| {
                probes += 1;
                if probes < 3 {
                    Err(jails_support::Failure::Told(
                        "server closed the connection unexpectedly".to_string(),
                    ))
                } else {
                    Ok(())
                }
            },
            10,
            Duration::from_millis(0),
        );
        assert!(outcome.is_ok());
        assert_eq!(probes, 3);
    }

    #[test]
    fn exhausting_the_budget_reports_the_last_failure_not_the_first() {
        let outcome = wait_for(
            |attempt| Err(format!("attempt {attempt}").into()),
            4,
            Duration::from_millis(0),
        );
        assert_eq!(outcome, Err("attempt 3".into()));
    }

    #[test]
    fn only_the_first_probe_is_announced() {
        let mut announced = Vec::new();
        let debug = true;
        let _ = wait_for(
            |attempt| {
                announced.push(debug && attempt == 0);
                Err(jails_support::Failure::Told("not yet".to_string()))
            },
            3,
            Duration::from_millis(0),
        );
        assert_eq!(announced, vec![true, false, false]);
    }
}
