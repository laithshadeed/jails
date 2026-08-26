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
            frozen,
            no_cache: _,
            manifest,
        } => {
            if live {
                return Err(
                    "live SQL evidence requires an explicit datasource and is not available in the offline compiler.\n       fix: use `--offline`; live description is introduced with datasource selection."
                        .into(),
                );
            }
            let project = Project::discover()?;
            let checked = jails_project::query_workspace::check_offline(
                &project,
                manifest.as_deref(),
                target.as_deref(),
            )?;
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
