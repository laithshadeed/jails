//! `jails migrate --check` -- apply every migration to a scratch database and
//! report the first thing that fails.
//!
//! ## Why this is not a `doctor` check
//!
//! `doctor` is read-only by contract: it must never start, stop or write
//! anything, so it stays safe to run mid-debug. Applying migrations writes by
//! definition. `doctor` can only ever answer "are there .sql files and will
//! something run them" -- which it now does -- and that is a different
//! question from "do they work".
//!
//! It was the gap between those two questions that hurt: a project shipped a
//! migration with `timestampz` for `timestamptz` and a column name misspelled
//! against the index below it, and nothing said a word, because nothing ever
//! parsed the file. Two bugs hid each other -- Flyway was not wired in either,
//! so the broken SQL never ran to fail.
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

use std::path::{Path, PathBuf};

use crate::compose;
use crate::generate::find_project_root;
use crate::run;

type Result<T> = std::result::Result<T, String>;

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
    if !run::find_on_path("psql") {
        return Err("psql not on PATH -- install the postgres client and try again".into());
    }
    if !no_start {
        compose::up(&root, &["postgres"], debug);
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
            Err(String::new())
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
    // mode that skips the work: a `--debug migrate` that returned early here
    // reported "applied cleanly" over SQL that had not been near a database.
    let done = crate::process::run(&spec, Diagnostics::from_flag(debug))?;
    if done.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&done.stderr).trim().to_string())
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
        let dir = std::env::temp_dir().join(format!("jails-migrate-order-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
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
}
