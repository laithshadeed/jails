//! Transaction-owned projection of a portable HTTP contract.

use super::*;

pub fn contract_emit(
    run: &Run,
    target: ProjectPath,
    body: Vec<u8>,
    json_schema: bool,
) -> Result<Outcome> {
    let project = run.project();
    let mut reads = capture::capability_reads()?.file(target.clone());
    let mut prefix = Vec::new();
    let components = target.as_str().split('/').collect::<Vec<_>>();
    for component in &components[..components.len().saturating_sub(1)] {
        prefix.push(*component);
        if prefix.as_slice() != [".jails"] {
            reads = reads.directory(ProjectPath::parse(&prefix.join("/"))?);
        }
    }
    for source in jails_java::java::source_files(&project.root().join("src/main/java")) {
        reads = reads.file(relative_path(project, &source)?);
    }
    let mut change = DesiredChange::maintenance(MaintenanceAttribution::ContractProjection);
    change.files.push(DesiredFile {
        path: target.clone(),
        body: DesiredBody::Bytes(body.into()),
        mode: None,
        resource: None,
        renderer: None,
    });
    let observed = observed(project)?;
    let set = resource_change_set(
        observed.generation(),
        vec![change],
        Vec::new(),
        PlannedSubject::ContractProjection {
            target: target.clone(),
            json_schema,
        },
    );
    set.validate()?;
    commit_set(
        run,
        set,
        &reads,
        &Asked::new(
            CanonicalMutationRequest::ContractEmit {
                target: target.clone(),
                json_schema,
            },
            &["contract", "emit"],
            Vec::new(),
            BTreeMap::from([
                ("out".to_string(), vec![target.as_str().to_string()]),
                (
                    "format".to_string(),
                    vec![
                        if json_schema {
                            "json-schema"
                        } else {
                            "openapi"
                        }
                        .to_string(),
                    ],
                ),
            ]),
            BTreeSet::new(),
        ),
    )
}
