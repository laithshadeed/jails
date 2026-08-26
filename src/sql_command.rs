//! CLI boundary for deterministic SQL contract checks.

use crate::SqlCommand;
use jails_generate::named_query::{self, NamedQueryPackages};
use jails_project::model::Project;
use jails_protocol::identity::Package;
use jails_support::Result;
use std::fs;

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
        } => crate::dispatch::mutate(invocation, false, |run| {
            jails_engine::route::sql_generate(
                run,
                target.as_deref(),
                manifest.as_deref(),
                into_slice.as_deref(),
            )
        }),
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
    let packages = packages(&query.slice_package)?;
    let expected = named_query::project(&query.source, &query.contract, &packages)?
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

fn packages(base: &Package) -> Result<NamedQueryPackages> {
    Ok(NamedQueryPackages {
        application_query: base.join(&Package::parse("application.query")?),
        jdbc_adapter: base.join(&Package::parse("adapter.jdbc")?),
        fake_adapter: base.join(&Package::parse("adapter.query")?),
    })
}
