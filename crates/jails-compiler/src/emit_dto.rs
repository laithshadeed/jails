//! Lower one entity DTO facet into its request, response, and contract test.
//!
//! The three files are one semantic projection but three merge histories.
//! Their artifact ids therefore differ even though field evolution recompiles
//! them in one plan. DTOs are managed ABI: reader edits merge, but ownership
//! cannot be transferred away from the compiler.

use crate::emit_companion_test::JAVA_TEST_ROOT;
use crate::{CompileError, emit_java};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile};
use jails_model::{AppModel, Entity, Field, Package, StableId};
use std::collections::BTreeSet;

pub(crate) fn lower(
    model: &AppModel,
    entity: &Entity,
    spring_boot: Option<&str>,
) -> Vec<Result<emit_java::Unit, CompileError>> {
    vec![
        request(model, entity, spring_boot),
        response(model, entity),
        contract_test(model, entity),
    ]
}

/// Whether a caller supplies this field, or the server assigns it.
///
/// **This is the whole reason a request record exists.** Binding the domain
/// record at the HTTP boundary lets any caller set the audit columns, the
/// optimistic-lock version and the tenancy scope -- the three things the
/// server is the authority on -- so `POST` with a `createdAt` of last year
/// silently succeeds. The request declares what the caller is *asked* for and
/// nothing else, which makes the omission structural rather than a check
/// somebody has to remember.
///
/// A literal default (`@default(QUEUED)`) stays: it is a value the caller may
/// supply and the schema fills in when they do not. Only the four functions
/// the model closes over -- `now`, `today`, `uuid7`, `identity` -- name a
/// value the server computes.
pub(crate) fn caller_supplied(field: &Field) -> bool {
    // **A scope field stays declared, and that is a limit rather than a
    // choice.** Proving it belongs to the caller needs the claim, and the
    // resource controller has no `ExecutionContext` to read one from -- that
    // machinery is the operation boundary's, where `@scope` is already proved
    // against a `ScopeAuthorizer`. Dropping the component here would leave
    // `toDomain` with no value for it and nowhere honest to get one. See
    // `plan.md` for the resource-boundary half.
    if field.semantics.updated || field.semantics.version {
        return false;
    }
    !matches!(
        field.semantics.default.as_ref().map(|default| &default.value),
        Some(jails_model::Value::Function { name, arguments })
            if arguments.is_empty()
                && matches!(name.as_str(), "now" | "today" | "uuid7" | "identity")
    )
}

/// The value `toDomain` supplies for a field the caller was not asked for.
///
/// Minted here rather than in the controller so the assignment sits where the
/// model declares it. `identity()` is the database's to assign, and the insert
/// omits the column -- so the honest Java placeholder is `null`, not a value
/// invented on the way past.
fn assigned_value(
    model: &AppModel,
    field: &Field,
    imports: &mut BTreeSet<String>,
) -> Result<String, CompileError> {
    if let Some(jails_model::Value::Function { name, .. }) = field
        .semantics
        .default
        .as_ref()
        .map(|default| &default.value)
    {
        match name.as_str() {
            "uuid7" => {
                // `TimeOrderedUuid` is the project's, not this entity's, so
                // it stays in the domain layer however a slice is packaged.
                imports.insert(format!(
                    "{}.TimeOrderedUuid",
                    model.project.package_for(Package::Domain)
                ));
                return Ok("TimeOrderedUuid.next()".to_string());
            }
            // The database assigns it and the insert omits the column, so
            // the Java side needs a placeholder rather than a value. A
            // primitive key has no null to write, and zero is what an
            // unassigned identity reads as everywhere else in Java.
            "identity" => {
                let java = emit_java::java_type(field, imports);
                return Ok(match java.as_str() {
                    "long" => "0L".to_string(),
                    "int" | "short" | "byte" => "0".to_string(),
                    _ => "null".to_string(),
                });
            }
            _ => {}
        }
    }
    let java = emit_java::java_type(field, imports);
    match java.as_str() {
        "Instant" => Ok("Instant.now()".to_string()),
        "LocalDate" => Ok("LocalDate.now()".to_string()),
        "LocalDateTime" => Ok("LocalDateTime.now()".to_string()),
        "OffsetDateTime" => Ok("OffsetDateTime.now()".to_string()),
        "long" | "Long" if field.semantics.version => Ok("0L".to_string()),
        "int" | "Integer" if field.semantics.version => Ok("0".to_string()),
        _ => Err(CompileError::new(format!(
            "field `{}` is server-assigned but `{java}` is not a type jails can mint\n       fix: declare it a caller input, or eject the DTO and assign it yourself",
            field.label
        ))),
    }
}

/// Every value this row assigns more than once, in the order it first appears.
fn duplicated(values: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut twice = BTreeSet::new();
    let mut order = Vec::new();
    for value in values {
        if !seen.insert(value.clone()) && twice.insert(value.clone()) {
            order.push(value.clone());
        }
    }
    order
}

/// What to call the local a repeated assignment is hoisted into.
///
/// **Only the clocks**, because they are the only assignments whose repetition
/// changes the row: two `TimeOrderedUuid.next()` calls are two different
/// columns that were always meant to differ. The name is the one a reader
/// would write, and it steps aside for a component that already has it rather
/// than shadowing the caller's own value.
fn hoisted_name(value: &str, taken: &BTreeSet<&str>) -> Option<String> {
    let name = match value {
        "Instant.now()" => "now",
        "LocalDate.now()" => "today",
        "LocalDateTime.now()" => "localNow",
        "OffsetDateTime.now()" => "offsetNow",
        _ => return None,
    };
    Some(if taken.contains(name) {
        format!("assigned{}{}", name[..1].to_uppercase(), &name[1..])
    } else {
        name.to_string()
    })
}

fn request(
    model: &AppModel,
    entity: &Entity,
    spring_boot: Option<&str>,
) -> Result<emit_java::Unit, CompileError> {
    let package = crate::emit_java::entity_package(model, entity, Package::Web);
    let type_name = format!("{}Request", entity.names.java_type);
    let artifact_id = format!("art_{}_dto_request", entity.id.as_str());
    let mut imports = BTreeSet::from([emit_java::domain_import(model, entity)]);
    let asked = entity
        .fields
        .iter()
        .filter(|field| caller_supplied(field))
        .collect::<Vec<_>>();
    let components = declared(
        model,
        &asked,
        &mut imports,
        Some(crate::emit_capability::validation_package(
            crate::emit_capability::boot_major(spring_boot),
        )),
    );
    if asked.iter().any(|field| !field.required) {
        imports.insert("java.util.Optional".to_string());
    }
    let mut arguments = Vec::new();
    let mut assigned = Vec::new();
    for field in &entity.fields {
        let name = &field.names.java_member;
        arguments.push(if !caller_supplied(field) {
            let value = assigned_value(model, field, &mut imports)?;
            assigned.push(value.clone());
            format!("                {value}")
        } else if field.required {
            format!("                {name}")
        } else {
            format!("                Optional.ofNullable({name})")
        });
    }
    // **One clock reading for the whole row.** `--timestamps` assigns
    // `createdAt` and `updatedAt` from the same source, and two calls to
    // `Instant.now()` are two readings: the row is born with an `updatedAt`
    // later than its `createdAt`, so "has this ever been edited" answers yes
    // for every row ever written. Hoisting is only for a value used more than
    // once -- a single `TimeOrderedUuid.next()` reads better inline.
    let mut locals = Vec::new();
    let taken: BTreeSet<&str> = asked
        .iter()
        .map(|field| field.names.java_member.as_str())
        .collect();
    for value in duplicated(&assigned) {
        let Some(local) = hoisted_name(&value, &taken) else {
            continue;
        };
        let ty = value
            .split_once('.')
            .map_or(value.as_str(), |(ty, _)| ty)
            .to_string();
        locals.push(format!("        {ty} {local} = {value};"));
        for argument in &mut arguments {
            if argument.trim_end_matches(',') == format!("                {value}") {
                *argument = format!("                {local}");
            }
        }
    }
    let arguments = arguments.join(",\n");
    let preamble = if locals.is_empty() {
        String::new()
    } else {
        format!("{}\n", locals.join("\n"))
    };
    let record = &entity.names.java_type;
    let body = format!(
        "public record {type_name}(\n{components}\n) {{\n\n    /**\n     * The domain row this request describes, with every server-assigned\n     * value supplied here rather than taken from the caller.\n     */\n    public {record} toDomain() {{\n{preamble}        return new {record}(\n{arguments});\n    }}\n}}"
    );
    unit(
        package,
        type_name,
        artifact_id,
        imports,
        body,
        FileKind::JavaMain,
        "dto-request",
        entity,
    )
}

fn response(model: &AppModel, entity: &Entity) -> Result<emit_java::Unit, CompileError> {
    let package = crate::emit_java::entity_package(model, entity, Package::Web);
    let type_name = format!("{}Response", entity.names.java_type);
    let artifact_id = format!("art_{}_dto_response", entity.id.as_str());
    let mut imports = BTreeSet::from([emit_java::domain_import(model, entity)]);
    let components = components(model, entity, &mut imports, None);
    let variable = lower_first(&entity.names.java_type);
    let arguments = entity
        .fields
        .iter()
        .map(|field| {
            let accessor = format!("{variable}.{}()", field.names.java_member);
            if field.required {
                format!("                {accessor}")
            } else {
                format!("                {accessor}.orElse(null)")
            }
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let record = &entity.names.java_type;
    let body = format!(
        "public record {type_name}(\n{components}\n) {{\n\n    public static {type_name} fromDomain({record} {variable}) {{\n        return new {type_name}(\n{arguments});\n    }}\n}}"
    );
    unit(
        package,
        type_name,
        artifact_id,
        imports,
        body,
        FileKind::JavaMain,
        "dto-response",
        entity,
    )
}

fn contract_test(model: &AppModel, entity: &Entity) -> Result<emit_java::Unit, CompileError> {
    let package = crate::emit_java::entity_package(model, entity, Package::Web);
    let record = &entity.names.java_type;
    let type_name = format!("{record}DtoTest");
    let artifact_id = format!("art_{}_dto_test", entity.id.as_str());
    let imports = BTreeSet::from([
        "java.util.Arrays".to_string(),
        "java.util.List".to_string(),
        "org.junit.jupiter.api.Test".to_string(),
        "static org.junit.jupiter.api.Assertions.assertEquals".to_string(),
    ]);
    let quoted = |fields: Vec<&Field>| {
        fields
            .iter()
            .map(|field| format!("\"{}\"", field.names.java_member))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let every = quoted(entity.fields.iter().collect());
    let asked = quoted(
        entity
            .fields
            .iter()
            .filter(|field| caller_supplied(field))
            .collect(),
    );
    // **The two records answer different questions, so the test asks two.**
    // A response projects the whole row; a request declares only what a caller
    // supplies, and asserting they match is asserting the boundary does not
    // exist. Naming the omitted components is what makes a field that quietly
    // becomes caller-settable fail here rather than in production.
    let body = format!(
        "final class {type_name} {{\n\n    @Test\n    void theResponseProjectsEveryComponent() {{\n        assertEquals(List.of({every}), componentNames({record}Response.class));\n    }}\n\n    /**\n     * The request declares what a caller supplies and nothing else: an\n     * audit column or an optimistic-lock version accepted from the body is\n     * a value the server is supposed to be the authority on.\n     */\n    @Test\n    void theRequestAsksOnlyForCallerSuppliedComponents() {{\n        assertEquals(List.of({asked}), componentNames({record}Request.class));\n    }}\n\n    private static List<String> componentNames(Class<?> type) {{\n        return Arrays.stream(type.getRecordComponents())\n                .map(component -> component.getName())\n                .toList();\n    }}\n}}"
    );
    unit(
        package,
        type_name,
        artifact_id,
        imports,
        body,
        FileKind::JavaTest,
        "dto-contract-test",
        entity,
    )
}

fn components(
    model: &AppModel,
    entity: &Entity,
    imports: &mut BTreeSet<String>,
    validation: Option<&str>,
) -> String {
    declared(
        model,
        &entity.fields.iter().collect::<Vec<_>>(),
        imports,
        validation,
    )
}

fn declared(
    model: &AppModel,
    fields: &[&Field],
    imports: &mut BTreeSet<String>,
    validation: Option<&str>,
) -> String {
    // **A DTO lives in `web` and a declared type lives in `domain`**, so an
    // enum or record component that needs no import inside the entity's own
    // package needs one here. Left out, every project whose entity carries a
    // declared type produced a request and a response that did not compile.
    for field in fields {
        emit_java::import_declared_type(model, &field.ty, imports);
    }
    fields
        .iter()
        .map(|field| {
            let annotation =
                validation.and_then(|package| validation_annotation(field, imports, package));
            let java = emit_java::java_type(field, imports);
            let name = &field.names.java_member;
            match annotation {
                Some(annotation) => format!("    {annotation} {java} {name}"),
                None => format!("    {java} {name}"),
            }
        })
        .collect::<Vec<_>>()
        .join(",\n")
}

fn validation_annotation(
    field: &Field,
    imports: &mut BTreeSet<String>,
    package: &str,
) -> Option<&'static str> {
    if !field.required || primitive(field) {
        return None;
    }
    if field.non_blank {
        imports.insert(format!("{package}.validation.constraints.NotBlank"));
        Some("@NotBlank")
    } else {
        imports.insert(format!("{package}.validation.constraints.NotNull"));
        Some("@NotNull")
    }
}

fn primitive(field: &Field) -> bool {
    field.required
        && matches!(
            field.ty,
            jails_model::TypeRef::Builtin(builtin) if builtin.semantics().java_primitive.is_some()
        )
}

#[allow(clippy::too_many_arguments)]
fn unit(
    package: String,
    type_name: String,
    artifact_id: String,
    imports: BTreeSet<String>,
    body: String,
    kind: FileKind,
    compiler_pass: &str,
    entity: &Entity,
) -> Result<emit_java::Unit, CompileError> {
    let rendered = emit_java::render(&package, &imports, &body, &artifact_id);
    let root = match kind {
        FileKind::JavaMain => emit_java::JAVA_ROOT,
        FileKind::JavaTest => JAVA_TEST_ROOT,
        _ => unreachable!("DTOs lower only Java source"),
    };
    let path = ProjectPath::parse(format!(
        "{root}/{}/{}.java",
        package.replace('.', "/"),
        type_name
    ))
    .map_err(CompileError::new)?;
    Ok(emit_java::Unit {
        path,
        file: RenderedFile {
            kind,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: false,
                semantic_ids: BTreeSet::from([entity.id.as_str().to_string()]),
                compiler_pass: compiler_pass.to_string(),
            },
        },
    })
}

fn lower_first(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_ascii_lowercase().to_string() + characters.as_str()
    })
}
