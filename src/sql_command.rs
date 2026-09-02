//! CLI boundary for deterministic SQL contract checks.

use crate::SqlCommand;
use jails_project::model::Project;
use jails_support::Result;
use std::fs;
use std::path::Path;

pub(crate) fn run(command: SqlCommand, invocation: crate::Invocation) -> Result<()> {
    match command {
        SqlCommand::Check {
            target,
            offline: _,
            live,
            datasource,
            services,
            frozen,
            no_cache: _,
            manifest,
        } => {
            let project = Project::discover()?;
            let checked = jails_project::query_workspace::check_offline(
                &project,
                manifest.as_deref(),
                target.as_deref(),
            )?;
            if live {
                let datasource = datasource.as_deref().ok_or_else(|| {
                    "live SQL checking requires an explicit datasource.\n       fix: pass `--datasource postgres`."
                        .to_string()
                })?;
                let descriptions = jails_drive::live_sql::check(
                    &project,
                    datasource,
                    match services {
                        crate::cli::RunServicesArg::Existing => {
                            jails_drive::live_sql::LiveServices::Existing
                        }
                        crate::cli::RunServicesArg::Start => {
                            jails_drive::live_sql::LiveServices::Start
                        }
                        crate::cli::RunServicesArg::None => {
                            jails_drive::live_sql::LiveServices::None
                        }
                    },
                    &checked,
                    invocation.debug,
                )?;
                if frozen {
                    check_frozen_live(
                        &project,
                        datasource,
                        services,
                        manifest.as_deref(),
                        invocation.debug,
                    )?;
                }
                for (query, description) in checked.iter().zip(descriptions) {
                    if frozen {
                        check_frozen(&project, query)?;
                    }
                    println!(
                        "✓ {}.{}  :{}  {} param(s)  {} column(s)  verified-live (postgres {})",
                        query.source.id.slice.as_str(),
                        query.source.id.name.as_str(),
                        query.source.cardinality.label(),
                        query.contract.parameters.len(),
                        description.columns.len(),
                        description.server_major,
                    );
                }
                return Ok(());
            }
            for query in &checked {
                if frozen {
                    check_frozen(&project, query)?;
                }
                println!(
                    "✓ {}.{}  :{}  {} param(s)  {} column(s)  verified-offline",
                    query.source.id.slice.as_str(),
                    query.source.id.name.as_str(),
                    query.source.cardinality.label(),
                    query.contract.parameters.len(),
                    query.contract.columns.len(),
                );
            }
            Ok(())
        }
        SqlCommand::Generate {
            target,
            into_slice,
            manifest,
        } => generate(
            target.as_deref(),
            manifest.as_deref(),
            into_slice.as_deref(),
            invocation,
        ),
    }
}

fn check_frozen_live(
    project: &Project,
    datasource: &str,
    services: crate::cli::RunServicesArg,
    manifest: Option<&std::path::Path>,
    debug: bool,
) -> Result<()> {
    let expected_major = jails_drive::live_sql::declared_server_major(project, datasource)?;
    let live = jails_drive::live_sql::observe(
        project,
        datasource,
        match services {
            crate::cli::RunServicesArg::Existing => jails_drive::live_sql::LiveServices::Existing,
            crate::cli::RunServicesArg::Start => jails_drive::live_sql::LiveServices::Start,
            crate::cli::RunServicesArg::None => jails_drive::live_sql::LiveServices::None,
        },
        "public",
        debug,
    )?;
    let actual_major = match &live.provenance {
        jails_protocol::database::SchemaProvenance::Live { server_major, .. } => {
            u32::from(*server_major)
        }
        _ => unreachable!("live observer always records live provenance"),
    };
    if actual_major != expected_major {
        return Err(format!(
            "frozen live SQL evidence uses PostgreSQL {actual_major}, but the declared image is PostgreSQL {expected_major}.\n       fix: run the clean migrated database at the pinned major or review and update the declaration."
        )
        .into());
    }
    let migrations = jails_project::query_workspace::migration_schema(project, manifest)?;
    let drift = jails_project::schema::diff(&migrations, &live)?;
    if !drift.is_empty() {
        return Err(format!(
            "frozen live SQL catalog differs from the checked migration authority ({} schema operation(s)).\n       fix: migrate a clean database from the committed files or review the live drift with `jails schema diff`.",
            drift.len()
        )
        .into());
    }
    Ok(())
}

fn check_frozen(
    project: &Project,
    query: &jails_project::query_workspace::CheckedQuery,
) -> Result<()> {
    let packages = jails_project::named_query::NamedQueryPackages::under(&query.slice_package)?;
    let expected = jails_project::named_query::project(&query.source, &query.contract, &packages)?
        .into_iter()
        .find(|artifact| artifact.kind == "SQL contract")
        .expect("named query projection always emits its contract");
    let path = project.root().join(&expected.path);
    let actual = fs::read_to_string(&path).map_err(|error| {
        format!(
            "frozen SQL contract {} is unavailable: {error}.\n       fix: run `jails sql generate {}` and commit the contract.",
            expected.path.display(),
            query.source.id.name.as_str()
        )
    })?;
    if actual != expected.contents {
        return Err(format!(
            "frozen SQL contract {} differs from the query, migration catalog, dialect, or mappings.\n       fix: review `jails sql generate {} --pretend`, then regenerate and commit it.",
            expected.path.display(),
            query.source.id.name.as_str()
        )
        .into());
    }
    Ok(())
}
/// Project each checked named query into Java, as ordinary files.
///
/// **A one-shot, not a declaration**, which is why it writes directly rather
/// than through the compiler: a named query lives in the reader's `.sql`
/// manifest, and jails renders its adapter beside it. There is nothing here
/// for a model to hold and nothing a later `sync` would reconcile -- the
/// manifest is the authority, and re-running is how the output is refreshed.
fn generate(
    selector: Option<&str>,
    manifest: Option<&Path>,
    into_slice: Option<&str>,
    invocation: crate::Invocation,
) -> Result<()> {
    let project = Project::discover()?;
    let checked = jails_project::query_workspace::check_offline(&project, manifest, selector)?;
    if let Some(expected) = into_slice {
        for query in &checked {
            let slice = query.source.id.slice.as_str();
            if slice != expected {
                return Err(format!(
                    "query `{slice}.{}` belongs to slice `{slice}`, not `{expected}`.\n       fix: omit `--into-slice` or name the manifest slice that owns the query.",
                    query.source.id.name.as_str()
                )
                .into());
            }
        }
    }
    let mut written = 0usize;
    for query in checked {
        let packages = jails_project::named_query::NamedQueryPackages::under(&query.slice_package)?;
        for artifact in
            jails_project::named_query::project(&query.source, &query.contract, &packages)?
        {
            // **Every path, because a count is not an answer.** The preview
            // and the commit name the same files, so a reader who wants to
            // know whether their contract test moved has something to read
            // other than `git status`.
            let verb = match artifact.path.exists() {
                true => "write",
                false => "create",
            };
            if invocation.pretend {
                println!("  {verb:<8}{}", artifact.path.display());
            } else {
                jails_support::apply::put_one_shot(&artifact.path, artifact.contents)?;
                println!("  {verb:<8}{}", artifact.path.display());
            }
            written += 1;
        }
    }
    if invocation.pretend {
        println!("--pretend: {written} file(s) would be written");
    } else {
        println!("wrote {written} file(s)");
    }
    Ok(())
}
