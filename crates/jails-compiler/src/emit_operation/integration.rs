//! The integration test that ships beside every operation's JDBC adapter.
//!
//! **An operation adapter is SQL, and SQL is the one thing a unit test cannot
//! check.** A column list that drifted from the row mapper, an `on conflict`
//! naming a column with no unique index, a `where` clause binding a type the
//! driver will not take -- every one of them compiles. The canonical backend
//! emitted a JDBC adapter per command, query and transition and an integration
//! test for none of them.
//!
//! What it asserts is that the statement runs against a real PostgreSQL and
//! answers. It deliberately does *not* assert which rows come back: a filter
//! sampled from the field it reads matches the stored row only when the
//! database did not assign that column, and `created_at`, an identity key and
//! any `@default(now())` are all assigned. Pinning containment would make the
//! test fail on the shape of the schema rather than on the correctness of the
//! statement, which is the opposite of what it is for.

use crate::CompileError;
use crate::emit_java::{JAVA_TEST_ROOT, domain_import, with_suffix};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile};
use jails_model::{AppModel, Operation, OperationKind, Package, ParameterSource, StableId};
use std::collections::BTreeSet;

pub(crate) fn lower_and_emit(
    model: &AppModel,
    output: &mut jails_contracts::RenderedTree,
) -> Result<(), CompileError> {
    let Some(capability) = model
        .capabilities
        .values()
        .find(|capability| capability.kind == "db")
    else {
        return Ok(());
    };
    for operation in model.operations.values() {
        if let Some((path, file)) = lower(model, capability.id.as_str(), operation)? {
            output.insert(path, file).map_err(CompileError::new)?;
        }
    }
    Ok(())
}

fn lower(
    model: &AppModel,
    capability_id: &str,
    operation: &Operation,
) -> Result<Option<(ProjectPath, RenderedFile)>, CompileError> {
    let (target, port_package, port_suffix, keyed, returns_list) = match &operation.kind {
        OperationKind::Command(spec) => (
            &spec.on,
            Package::ApplicationCommands,
            "Command",
            false,
            false,
        ),
        OperationKind::Query(spec) => (&spec.on, Package::ApplicationQueries, "Query", false, true),
        OperationKind::Transition(spec) => (
            &spec.on,
            Package::ApplicationTransitions,
            "Transition",
            true,
            false,
        ),
        OperationKind::Event(_) => return Ok(None),
    };
    let Some(entity) = model.entities.get(target) else {
        return Ok(None);
    };
    // A scoped operation takes an `ExecutionContext` this test cannot mint.
    if entity
        .fields
        .iter()
        .any(|field| field.semantics.scope.is_some())
    {
        return Ok(None);
    }

    let port_type = with_suffix(&operation.names.java_type, port_suffix);
    let type_name = format!("Jdbc{port_type}IT");
    let package = model.project.package_for(Package::AdaptersJdbc);
    let mut imports = crate::emit_fixture::integration_imports(model);
    imports.insert(format!(
        "{}.{port_type}",
        model.project.package_for(port_package)
    ));
    imports.insert(domain_import(model, entity));

    let Some(parents) = crate::emit_fixture::parents(model, entity, &mut imports) else {
        return Ok(None);
    };
    let Some(arguments) = input_arguments(model, operation, &parents, &mut imports) else {
        return Ok(None);
    };

    // A query and a transition both need a row already in the table: one to
    // read, one to change. A command writes its own.
    let stored = if returns_list || keyed {
        let Some(row) = crate::emit_companion_test::constructor_call_with(
            model,
            entity,
            &mut imports,
            &parents.overrides,
        ) else {
            return Ok(None);
        };
        let record = &entity.names.java_type;
        imports.insert(format!(
            "{}.{record}Repository",
            model.project.package_for(Package::Repository)
        ));
        Some((record.clone(), row))
    } else {
        None
    };

    let mut autowired = format!(
        "\n    @Autowired\n    private {port_type} operation;\n{}",
        parents.autowired
    );
    let mut fixtures = parents.fixtures.clone();
    if let Some((record, row)) = &stored {
        let field = lower_first(record);
        autowired.push_str(&format!(
            "\n    @Autowired\n    private {record}Repository {field}Repository;\n"
        ));
        fixtures.push_str(&format!(
            "        {record} stored = {field}Repository.save({row});\n"
        ));
    }

    let record = &entity.names.java_type;
    let key = crate::emit_java::primary_key(entity)?;
    let invocation = if keyed {
        format!(
            "operation.execute(stored.{}(), new {port_type}.Input({arguments}))",
            key.names.java_member
        )
    } else {
        format!("operation.execute(new {port_type}.Input({arguments}))")
    };
    let (declaration, assertion) = if returns_list {
        imports.insert("java.util.List".to_string());
        (
            format!("List<{record}> answered = {invocation};"),
            "        assertThat(answered).isNotNull();".to_string(),
        )
    } else {
        (
            format!("{record} answered = {invocation};"),
            // A command's insert and a transition's update both name
            // `returning`, so a null answer means the statement matched no row
            // -- which is the failure worth catching here.
            "        assertThat(answered).isNotNull();".to_string(),
        )
    };
    let annotations = crate::emit_fixture::ANNOTATIONS;
    let body = format!(
        "{annotations}class {type_name} {{\n{autowired}\n    @Test\n    void runsAgainstTheRealDatabase() {{\n{fixtures}        {declaration}\n\n{assertion}\n    }}\n\n    // Reader-owned tests belong below this stable boundary.\n}}"
    );
    let artifact_id = format!("art_{}_adapter_test", operation.id.as_str());
    let rendered = crate::emit_java::render(&package, &imports, &body, &artifact_id);
    let package_path = package.replace('.', "/");
    let path = ProjectPath::parse(format!("{JAVA_TEST_ROOT}/{package_path}/{type_name}.java"))
        .map_err(CompileError::new)?;
    Ok(Some((
        path,
        RenderedFile {
            kind: FileKind::JavaTest,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: true,
                semantic_ids: BTreeSet::from([
                    capability_id.to_string(),
                    operation.id.as_str().to_string(),
                ]),
                compiler_pass: "capability-db-operation-test".to_string(),
            },
        },
    )))
}

/// The operation's `Input` arguments, in the order its record declares them.
///
/// A component reading a foreign key takes the parent's assigned value rather
/// than a sample, for the same reason the row does: a sampled key names a row
/// that is not there.
fn input_arguments(
    model: &AppModel,
    operation: &Operation,
    parents: &crate::emit_fixture::Parents,
    imports: &mut BTreeSet<String>,
) -> Option<String> {
    let arguments = match crate::emit_java::operation_input(model, operation).ok()? {
        crate::emit_java::InputSource::Fields(fields) => fields
            .into_iter()
            .map(|field| match parents.overrides.get(&field.id) {
                Some(supplied) => Some(supplied.clone()),
                None => crate::emit_companion_test::sample_of(model, field, imports),
            })
            .collect::<Option<Vec<_>>>()?,
        crate::emit_java::InputSource::Parameters(parameters) => parameters
            .iter()
            .map(|parameter| {
                let ParameterSource::Field(visible) = &parameter.source else {
                    return None;
                };
                if let Some(supplied) = parents.overrides.get(&visible.field) {
                    return Some(supplied.clone());
                }
                let owner = model.entities.get(&visible.entity)?;
                let field = owner.field(&visible.field)?;
                crate::emit_companion_test::sample_of(model, field, imports)
            })
            .collect::<Option<Vec<_>>>()?,
    };
    Some(arguments.join(", "))
}

fn lower_first(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_ascii_lowercase().to_string() + characters.as_str()
    })
}
