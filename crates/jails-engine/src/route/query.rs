//! Query-owned Java and contract projections as one prepared transition.

use super::*;
use jails_generate::named_query::{self, NamedQueryPackages};
use jails_protocol::database::QueryId;
use jails_protocol::identity::Package;
use std::path::Path;

pub fn sql_generate(
    run: &Run,
    selector: Option<&str>,
    manifest: Option<&Path>,
    into_slice: Option<&str>,
) -> Result<Outcome> {
    let project = run.project();
    let checked = jails_project::query_workspace::check_offline(project, manifest, selector)?;
    if let Some(expected) = into_slice {
        for query in &checked {
            if query.source.id.slice.as_str() != expected {
                return Err(format!(
                    "query `{}.{}` belongs to slice `{}`, not `{expected}`.\n       fix: omit `--into-slice` or name the manifest slice that owns the query.",
                    query.source.id.slice.as_str(),
                    query.source.id.name.as_str(),
                    query.source.id.slice.as_str()
                )
                .into());
            }
        }
    }

    let mut ordered = Vec::new();
    let mut resources_after = Vec::new();
    let mut queries = BTreeSet::<QueryId>::new();
    let mut reads = capture::capability_reads()?;
    for query in checked {
        reads = query
            .inputs
            .iter()
            .cloned()
            .fold(reads, ReadDeclaration::file);
        let owner = ResourceOwner::Query(query.source.id.clone());
        let mut change = DesiredChange::owned_by(owner.clone());
        let query_key = ResourceKey::Query(query.source.id.clone());
        change.resources.push(DesiredResource::new(
            query_key,
            BTreeSet::from([owner.clone()]),
            ResourceValue::Query,
        )?);
        for artifact in named_query::project(
            &query.source,
            &query.contract,
            &packages(&query.slice_package)?,
        )? {
            let path = ProjectPath::parse(&artifact.path.to_string_lossy())?;
            let key = ResourceKey::WholeFile(path.clone());
            change.resources.push(DesiredResource::new(
                key.clone(),
                BTreeSet::from([owner.clone()]),
                ResourceValue::WholeFile,
            )?);
            change.files.push(DesiredFile {
                path: path.clone(),
                body: DesiredBody::Bytes(artifact.contents.as_bytes().into()),
                mode: None,
                resource: Some(key),
                renderer: None,
            });
            reads = reads.file(path);
        }
        resources_after.extend(change.resources.clone());
        queries.insert(query.source.id);
        ordered.push(change);
    }

    let observed = observed(project)?;
    let set = resource_change_set(
        observed.generation(),
        ordered,
        resources_after,
        PlannedSubject::GenerateQueries {
            queries: queries.clone(),
        },
    );
    set.validate()?;
    let positionals = selector.into_iter().map(str::to_string).collect();
    let options = into_slice
        .map(|slice| BTreeMap::from([("into-slice".to_string(), vec![slice.to_string()])]))
        .unwrap_or_default();
    commit_set(
        run,
        set,
        &reads,
        &Asked::new(
            CanonicalMutationRequest::SqlGenerate { queries },
            &["sql", "generate"],
            positionals,
            options,
            BTreeSet::new(),
        ),
    )
}

fn packages(base: &Package) -> Result<NamedQueryPackages> {
    Ok(NamedQueryPackages {
        application_query: base.join(&Package::parse("application.query")?),
        jdbc_adapter: base.join(&Package::parse("adapter.jdbc")?),
        fake_adapter: base.join(&Package::parse("adapter.query")?),
    })
}
