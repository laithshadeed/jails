//! The concise, single-source authoring frontend for [`crate::AppModel`].
//!
//! JDL lowers to the existing closed semantic source and then uses the same
//! linker as every other frontend. It never becomes a second model.

use crate::{AppModel, Diagnostics, EndpointMethod, RequestFormat};

mod declaration;
mod operation;
mod render;
pub mod upgrade;
pub mod v1;
use declaration::{DependencyDraft, EjectionDraft, SettingDraft};
use operation::OperationDraft;

#[derive(Default)]
struct ProjectDraft {
    name: Option<String>,
    id: Option<String>,
    package: Option<String>,
    java: Option<u16>,
    dialect: Option<String>,
}

struct EntityDraft {
    name: String,
    id: String,
    label: String,
    active: bool,
    facets: Vec<String>,
    table: Option<String>,
    fields: Vec<FieldDraft>,
    indexes: Vec<IndexDraft>,
    operations: Vec<OperationDraft>,
}

struct FieldDraft {
    name: String,
    id: String,
    label: String,
    type_name: String,
    column: Option<String>,
    required: bool,
    non_blank: bool,
    primary_key: bool,
    unique: bool,
    indexed: bool,
    min_length: Option<u32>,
    max_length: Option<u32>,
}

struct IndexDraft {
    label: String,
    id: String,
    name: Option<String>,
    columns: Vec<String>,
}

struct EnumDraft {
    name: String,
    id: String,
    label: String,
    values: Vec<String>,
}

struct CapabilityDraft {
    label: String,
    id: String,
    kind: String,
    name: Option<String>,
    package: Option<String>,
}

struct UnitDraft {
    name: String,
    id: String,
    label: String,
    kind: &'static str,
    package: Option<String>,
    variants: Vec<String>,
    on: Option<String>,
    yields: Option<String>,
    method: Option<EndpointMethod>,
    path: Option<String>,
    consumes: Option<RequestFormat>,
}

struct DocumentDraft {
    project: ProjectDraft,
    entities: Vec<EntityDraft>,
    enums: Vec<EnumDraft>,
    units: Vec<UnitDraft>,
    capabilities: Vec<CapabilityDraft>,
    dependencies: Vec<DependencyDraft>,
    settings: Vec<SettingDraft>,
    ejections: Vec<EjectionDraft>,
}

enum Block {
    Root,
    Entity(EntityDraft),
    Enum(EnumDraft),
    Operation(EntityDraft, Box<OperationDraft>),
}

pub fn parse(input: &str) -> Result<AppModel, Diagnostics> {
    if v1::is_v1(input) {
        return v1::parse(input);
    }
    let mut project = ProjectDraft::default();
    let mut entities = Vec::new();
    let mut enums = Vec::new();
    let mut capabilities = Vec::new();
    let mut units = Vec::new();
    let mut dependencies = Vec::new();
    let mut settings = Vec::new();
    let mut ejections = Vec::new();
    let mut block = Block::Root;

    for (offset, raw) in input.lines().enumerate() {
        let line_number = offset + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        match &mut block {
            Block::Root => {
                if let Some(rest) = line.strip_prefix("application ") {
                    if project.name.is_some() {
                        return Err(problem(
                            line_number,
                            "the application header is declared more than once",
                            "keep one `application Name` line",
                        ));
                    }
                    let name = first_word(rest);
                    if name.is_empty() {
                        return Err(problem(
                            line_number,
                            "the application header has no name",
                            "write `application MyApp`",
                        ));
                    }
                    project.name = Some(name.to_string());
                    project.id = annotation(rest, "id")
                        .map(str::to_string)
                        .or_else(|| Some(format!("project_{}", label(name))));
                } else if let Some(rest) = line.strip_prefix("package ") {
                    project.package = Some(rest.trim().to_string());
                } else if let Some(rest) = line.strip_prefix("java ") {
                    project.java = Some(rest.trim().parse().map_err(|_| {
                        problem(
                            line_number,
                            format!("`{}` is not a Java release", rest.trim()),
                            "write a number such as `java 21`",
                        )
                    })?);
                } else if let Some(rest) = line.strip_prefix("dialect ") {
                    project.dialect = Some(rest.trim().to_string());
                } else if line.starts_with("entity ") {
                    block = Block::Entity(entity_header(line_number, line)?);
                } else if line.starts_with("enum ") {
                    block = Block::Enum(enum_header(line_number, line)?);
                } else if line.starts_with("capability ") {
                    capabilities.push(capability(line_number, line)?);
                } else if [
                    "class ",
                    "interface ",
                    "service ",
                    "test ",
                    "integration-test ",
                    "sealed ",
                    "strategy ",
                    "controller ",
                ]
                .iter()
                .any(|prefix| line.starts_with(prefix))
                {
                    units.push(unit(line_number, line)?);
                } else if line.starts_with("dependency ") {
                    dependencies.push(declaration::dependency(line_number, line)?);
                } else if line.starts_with("setting ") {
                    settings.push(declaration::setting(line_number, line)?);
                } else if line.starts_with("eject ") {
                    ejections.push(declaration::ejection(line_number, line)?);
                } else {
                    return Err(problem(
                        line_number,
                        format!("`{line}` is not a top-level JDL declaration"),
                        "use `application`, `package`, `java`, `dialect`, `entity`, `enum`, `class`, `interface`, `service`, `test`, `integration-test`, `sealed`, `capability`, `dependency`, `setting`, or `eject`",
                    ));
                }
            }
            Block::Entity(entity) => {
                if line == "}" {
                    let Block::Entity(entity) = std::mem::replace(&mut block, Block::Root) else {
                        unreachable!()
                    };
                    entities.push(entity);
                } else if operation::is_header(line) {
                    let Block::Entity(entity) = std::mem::replace(&mut block, Block::Root) else {
                        unreachable!()
                    };
                    let operation = operation::header(line_number, line, &entity.label)?;
                    block = Block::Operation(entity, Box::new(operation));
                } else if line.starts_with("index ") {
                    entity.indexes.push(index(line_number, line)?);
                } else if line.ends_with('{') {
                    return Err(problem(
                        line_number,
                        "this nested JDL declaration is not recognized",
                        "use `command`, `query`, `transition`, or `event` inside an entity",
                    ));
                } else {
                    entity.fields.push(field(line_number, &entity.label, line)?);
                }
            }
            Block::Enum(enumeration) => {
                if line == "}" {
                    let Block::Enum(enumeration) = std::mem::replace(&mut block, Block::Root)
                    else {
                        unreachable!()
                    };
                    enums.push(enumeration);
                } else {
                    enumeration.values.push(enum_value(line_number, line)?);
                }
            }
            Block::Operation(_, operation) if line == "}" => {
                let Block::Operation(mut entity, operation) =
                    std::mem::replace(&mut block, Block::Root)
                else {
                    unreachable!()
                };
                entity.operations.push(*operation);
                block = Block::Entity(entity);
            }
            Block::Operation(_, operation) => operation.property(line_number, line)?,
        }
    }
    if !matches!(block, Block::Root) {
        return Err(problem(
            input.lines().count().max(1),
            "a JDL declaration is missing its closing `}`",
            "close the entity or enum block",
        ));
    }
    crate::parse_toml(&render::render(DocumentDraft {
        project,
        entities,
        enums,
        units,
        capabilities,
        dependencies,
        settings,
        ejections,
    })?)
}

fn entity_header(line_number: usize, line: &str) -> Result<EntityDraft, Diagnostics> {
    if !line.ends_with('{') {
        return Err(problem(
            line_number,
            "an entity header must end with `{`",
            "write `entity Task {`",
        ));
    }
    let rest = line
        .strip_prefix("entity ")
        .expect("caller recognized entity")
        .trim_end_matches('{')
        .trim();
    let name = first_word(rest);
    if name.is_empty() {
        return Err(problem(
            line_number,
            "the entity has no name",
            "write `entity Task {`",
        ));
    }
    let label = annotation(rest, "as")
        .map(str::to_string)
        .unwrap_or_else(|| label(name));
    let factory = rest.split_whitespace().any(|word| word == "@factory");
    let dto = rest.split_whitespace().any(|word| word == "@dto");
    let repository = rest.split_whitespace().any(|word| word == "@repository");
    let mut facets = if rest.split_whitespace().any(|word| word == "@scaffold") {
        vec!["record", "repository", "service", "http"]
    } else if let Some(values) = annotation(rest, "facets") {
        values
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect()
    } else {
        vec!["record"]
    };
    if factory {
        facets.push("factory");
    }
    if dto {
        facets.push("dto");
    }
    if repository && !facets.contains(&"repository") {
        facets.push("repository");
    }
    Ok(EntityDraft {
        name: name.to_string(),
        id: annotation(rest, "id")
            .map(str::to_string)
            .unwrap_or_else(|| format!("ent_{label}")),
        label,
        active: !marker(rest, "inactive"),
        facets: facets.into_iter().map(str::to_string).collect(),
        table: annotation(rest, "table").map(str::to_string),
        fields: Vec::new(),
        indexes: Vec::new(),
        operations: Vec::new(),
    })
}

fn enum_header(line_number: usize, line: &str) -> Result<EnumDraft, Diagnostics> {
    if !line.ends_with('{') {
        return Err(problem(
            line_number,
            "an enum header must end with `{`",
            "write `enum Status {`",
        ));
    }
    let rest = line
        .strip_prefix("enum ")
        .expect("caller recognized enum")
        .trim_end_matches('{')
        .trim();
    let name = first_word(rest);
    if name.is_empty() {
        return Err(problem(
            line_number,
            "the enum has no name",
            "write `enum Status {`",
        ));
    }
    let label = annotation(rest, "as")
        .map(str::to_string)
        .unwrap_or_else(|| label(name));
    Ok(EnumDraft {
        name: name.to_string(),
        id: annotation(rest, "id")
            .map(str::to_string)
            .unwrap_or_else(|| format!("ent_{label}")),
        label,
        values: Vec::new(),
    })
}

fn field(line_number: usize, entity: &str, line: &str) -> Result<FieldDraft, Diagnostics> {
    let line = line.trim_end_matches([',', ';']).trim();
    let (name, rest) = line.split_once(':').ok_or_else(|| {
        problem(
            line_number,
            format!("`{line}` is not a compact field"),
            "write `name: type`, optionally followed by `!`, `?`, `@pk`, `@unique`, or `@index`",
        )
    })?;
    let name = name.trim();
    let type_token = rest.split_whitespace().next().unwrap_or_default();
    if name.is_empty() || type_token.is_empty() {
        return Err(problem(
            line_number,
            "a field name or type is empty",
            "write a complete field such as `title: string!`",
        ));
    }
    let (type_shape, min_length, max_length) = parse_length(line_number, type_token)?;
    let (type_name, required, non_blank) = if let Some(value) = type_shape.strip_suffix('!') {
        (value, true, true)
    } else if let Some(value) = type_shape.strip_suffix('?') {
        (value, false, false)
    } else {
        (type_shape, true, false)
    };
    let field_label = annotation(rest, "as")
        .map(str::to_string)
        .unwrap_or_else(|| label(name));
    Ok(FieldDraft {
        name: name.to_string(),
        id: annotation(rest, "id")
            .map(str::to_string)
            .unwrap_or_else(|| format!("fld_{entity}_{field_label}")),
        label: field_label,
        type_name: normalize_type(type_name).to_string(),
        column: annotation(rest, "column").map(str::to_string),
        required,
        non_blank,
        primary_key: marker(rest, "pk"),
        unique: marker(rest, "unique"),
        indexed: marker(rest, "index"),
        min_length,
        max_length,
    })
}

fn parse_length(line: usize, token: &str) -> Result<(&str, Option<u32>, Option<u32>), Diagnostics> {
    let Some(open) = token.find('(') else {
        return Ok((token, None, None));
    };
    if !token.ends_with(')') {
        return Err(problem(
            line,
            "the field length range is not closed",
            "write a range such as `string!(1..200)`",
        ));
    }
    let bounds = &token[open + 1..token.len() - 1];
    let (min, max) = bounds.split_once("..").ok_or_else(|| {
        problem(
            line,
            format!("`{bounds}` is not a length range"),
            "use `min..max`, `min..`, or `..max`",
        )
    })?;
    let parse_bound = |value: &str| {
        if value.is_empty() {
            Ok(None)
        } else {
            value.parse::<u32>().map(Some).map_err(|_| {
                problem(
                    line,
                    format!("`{value}` is not a non-negative length bound"),
                    "use an unsigned integer length",
                )
            })
        }
    };
    let min = parse_bound(min.trim())?;
    let max = parse_bound(max.trim())?;
    if min.is_none() && max.is_none() {
        return Err(problem(
            line,
            "the field length range has no bounds",
            "provide at least one bound inside `(min..max)`",
        ));
    }
    Ok((&token[..open], min, max))
}

fn index(line_number: usize, line: &str) -> Result<IndexDraft, Diagnostics> {
    let rest = line
        .strip_prefix("index ")
        .expect("caller recognized index")
        .trim();
    let open = rest.find('(').ok_or_else(|| {
        problem(
            line_number,
            "the index has no column list",
            "write `index (title, createdAt desc) @id(idx_task_recent)`",
        )
    })?;
    let close = rest[open + 1..]
        .find(')')
        .map(|close| open + 1 + close)
        .ok_or_else(|| {
            problem(
                line_number,
                "the index column list is not closed",
                "close the index columns with `)`",
            )
        })?;
    let columns = rest[open + 1..close]
        .split(',')
        .map(str::trim)
        .filter(|column| !column.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if columns.is_empty() {
        return Err(problem(
            line_number,
            "the index has no columns",
            "name at least one field inside `index (...)`",
        ));
    }
    let annotations = rest[close + 1..].trim();
    let id = annotation(annotations, "id")
        .map(str::to_string)
        .unwrap_or_else(|| format!("idx_{}", columns.join("_").replace(' ', "_")));
    Ok(IndexDraft {
        label: annotation(annotations, "as").unwrap_or(&id).to_string(),
        id,
        name: annotation(annotations, "name").map(str::to_string),
        columns,
    })
}

fn enum_value(line_number: usize, line: &str) -> Result<String, Diagnostics> {
    let line = line.trim_end_matches([',', ';']).trim();
    if line.is_empty() {
        return Err(problem(
            line_number,
            "the enum contains an empty value",
            "remove the empty line or name a constant",
        ));
    }
    let Some((name, wire)) = line.split_once('=') else {
        return Ok(line.to_string());
    };
    let wire = wire.trim().trim_matches('"');
    Ok(format!("{}={wire}", name.trim()))
}

fn capability(line_number: usize, line: &str) -> Result<CapabilityDraft, Diagnostics> {
    let rest = line
        .strip_prefix("capability ")
        .expect("caller recognized capability")
        .trim();
    let kind = first_word(rest);
    if kind.is_empty() {
        return Err(problem(
            line_number,
            "the capability has no kind",
            "write `capability api`",
        ));
    }
    let label = annotation(rest, "as").unwrap_or(kind).replace('-', "_");
    Ok(CapabilityDraft {
        id: annotation(rest, "id")
            .map(str::to_string)
            .unwrap_or_else(|| format!("cap_{label}")),
        label,
        kind: kind.to_string(),
        name: annotation(rest, "name").map(str::to_string),
        package: annotation(rest, "package").map(str::to_string),
    })
}

fn unit(line_number: usize, line: &str) -> Result<UnitDraft, Diagnostics> {
    let (kind, rest) = [
        "class",
        "interface",
        "service",
        "test",
        "integration-test",
        "sealed",
        "strategy",
        "controller",
    ]
    .into_iter()
    .find_map(|kind| {
        line.strip_prefix(&format!("{kind} "))
            .map(|rest| (kind, rest.trim()))
    })
    .expect("caller recognized a source unit");
    let name = first_word(rest);
    if name.is_empty() {
        return Err(problem(
            line_number,
            format!("the {kind} has no name"),
            format!("write `{kind} Example`"),
        ));
    }
    let label = annotation(rest, "as")
        .map(str::to_string)
        .unwrap_or_else(|| label(name));
    let variants = rest
        .split_whitespace()
        .skip(1)
        .take_while(|word| !word.starts_with('@'))
        .map(str::to_string)
        .collect::<Vec<_>>();
    if matches!(kind, "sealed" | "strategy") && variants.is_empty() {
        return Err(problem(
            line_number,
            format!("the {kind} has no variants"),
            format!("write `{kind} {name} Accepted Rejected`"),
        ));
    }
    if !matches!(kind, "sealed" | "strategy") && !variants.is_empty() {
        return Err(problem(
            line_number,
            format!("the {kind} declaration has unexpected positional arguments"),
            format!("write `{kind} {name}` and remove the extra names"),
        ));
    }
    let on = annotation(rest, "on").map(str::to_string);
    if kind == "strategy" && on.is_none() {
        return Err(problem(
            line_number,
            "the strategy does not name the type it examines",
            format!("add `@on(Subject)` to the `{name}` strategy"),
        ));
    }
    let yields = annotation(rest, "yields").map(str::to_string);
    if !matches!(kind, "strategy" | "controller") && (on.is_some() || yields.is_some()) {
        return Err(problem(
            line_number,
            format!("the {kind} declaration has strategy type annotations"),
            "remove `@on(...)` and `@yields(...)`",
        ));
    }
    let method = annotation(rest, "method")
        .map(EndpointMethod::parse)
        .transpose()
        .map_err(|message| problem(line_number, message, "use get, post, put, patch, or delete"))?;
    let consumes = annotation(rest, "consumes")
        .map(RequestFormat::parse)
        .transpose()
        .map_err(|message| problem(line_number, message, "use json or form"))?;
    let path = annotation(rest, "path").map(str::to_string);
    if kind != "controller" && (method.is_some() || path.is_some() || consumes.is_some()) {
        return Err(problem(
            line_number,
            format!("the {kind} declaration has HTTP endpoint annotations"),
            "remove `@method(...)`, `@path(...)`, and `@consumes(...)`",
        ));
    }
    Ok(UnitDraft {
        name: name.to_string(),
        id: annotation(rest, "id")
            .map(str::to_string)
            .unwrap_or_else(|| format!("unit_{}_{label}", kind.replace('-', "_"))),
        label,
        kind,
        package: annotation(rest, "package").map(str::to_string),
        variants,
        on,
        yields,
        method,
        path,
        consumes,
    })
}

fn annotation<'a>(input: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("@{name}(");
    let start = input.find(&prefix)? + prefix.len();
    let rest = &input[start..];
    let end = rest.find(')')?;
    Some(rest[..end].trim())
}

fn marker(input: &str, name: &str) -> bool {
    input
        .split_whitespace()
        .any(|word| word == format!("@{name}"))
}

fn first_word(input: &str) -> &str {
    input.split_whitespace().next().unwrap_or_default()
}

/// The stable label a pre-v1 name gets.
///
/// **It is `naming::stable_fragment`, and it used to be a second copy.** The
/// copy differed in one way that looks cosmetic and is not: it left every
/// character that is neither a letter nor `-` alone, so a dot survived into
/// the label. A label is the key of the intermediate TOML table *and* a model
/// label, so `dependency org.apache.commons:commons-csv` -- any real Maven
/// group -- and `setting server.port` -- the most ordinary setting there is --
/// each produced a label the linker then refused. Neither parsed at all.
///
/// `stable_fragment` agrees with the old function on every name that was
/// already accepted: both lowercase, both split camelCase on an underscore,
/// both map `-` to `_`. It differs only where the old one produced something
/// invalid.
fn label(value: &str) -> String {
    crate::naming::stable_fragment(value)
}

fn normalize_type(value: &str) -> &str {
    crate::model::BuiltinType::canonicalize(value)
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    let bytes = line.as_bytes();
    let mut offset = 0;
    while offset + 1 < bytes.len() {
        if bytes[offset] == b'"' && (offset == 0 || bytes[offset - 1] != b'\\') {
            quoted = !quoted;
        }
        if !quoted && bytes[offset] == b'/' && bytes[offset + 1] == b'/' {
            return &line[..offset];
        }
        offset += 1;
    }
    line
}

fn problem(line: usize, message: impl Into<String>, fix: impl Into<String>) -> Diagnostics {
    Diagnostics::jdl_syntax(line, message, fix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Facet, OperationKind, StableId};

    #[test]
    fn compact_jdl_lowers_to_the_same_typed_model() {
        let model = parse(
            r#"
application Notes @id(project_notes)
package com.example.notes
java 26
dialect postgresql

capability api
dependency org.example:widget @id(dep_widget) @scope(test) = "1.2.3"
setting server.port @id(set_server_port) @target(main) = "8080"

entity Task @id(ent_task) @scaffold @factory @dto {
  id: uuid @id(fld_task_id) @pk
  title: string!(1..200) @index
  done: boolean?
}

enum Status {
  OPEN
  IN_PROGRESS = "in_progress"
}
"#,
        )
        .unwrap();
        let task = model
            .entities
            .values()
            .find(|entity| entity.label == "task")
            .unwrap();
        assert_eq!(task.id.as_str(), "ent_task");
        assert!(task.facets.contains(&Facet::Repository));
        assert!(task.facets.contains(&Facet::Factory));
        assert!(task.facets.contains(&Facet::Dto));
        assert_eq!(task.fields.len(), 3);
        let title = task
            .fields
            .iter()
            .find(|field| field.label == "title")
            .unwrap();
        assert_eq!(title.length.as_ref().unwrap().min, Some(1));
        assert_eq!(title.length.as_ref().unwrap().max, Some(200));
        let status = model
            .entities
            .values()
            .find(|entity| entity.label == "status")
            .unwrap();
        assert_eq!(status.enum_constants[1].wire_value(), "in_progress");
        assert_eq!(model.capabilities.len(), 1);
        let dependency = model.dependencies.values().next().unwrap();
        assert_eq!(dependency.id.as_str(), "dep_widget");
        assert_eq!(dependency.version.as_deref(), Some("1.2.3"));
        assert_eq!(dependency.scope, crate::DependencyScope::Test);
        let setting = model.settings.values().next().unwrap();
        assert_eq!(setting.id.as_str(), "set_server_port");
        assert_eq!(setting.value, "8080");
        assert_eq!(setting.target, crate::SettingTarget::Main);
    }

    #[test]
    fn malformed_length_constraints_are_refused_not_discarded() {
        let error = parse(
            "application Notes\npackage com.example.notes\njava 26\ndialect postgresql\nentity Note {\n title: string!(one..200)\n}\n",
        )
        .unwrap_err();
        assert!(error.to_string().contains("length bound"));
    }

    #[test]
    fn capability_projection_options_are_typed_and_package_relative() {
        let model = parse(
            "application Demo\npackage com.example.demo\njava 26\ndialect postgresql\ncapability csv @id(cap_csv) @name(Dataset) @package(imports)\n",
        )
        .unwrap();
        let capability = model.capabilities.values().next().unwrap();
        assert_eq!(capability.id.as_str(), "cap_csv");
        assert_eq!(capability.name.as_deref(), Some("Dataset"));
        assert_eq!(
            capability.java_package.as_deref(),
            Some("com.example.demo.imports")
        );
    }

    #[test]
    fn source_units_lower_to_one_typed_vocabulary_with_resolved_packages() {
        let model = parse(
            "application Demo\npackage com.example.demo\njava 26\ndialect postgresql\n\nclass Clock\ninterface Port @package(ports)\nservice BillingService\ntest Parser\nintegration-test Checkout\nsealed Outcome Accepted Rejected\nstrategy PostRule Featured Standard @on(Post) @yields(Tag)\ncontroller Verify @method(post) @path(/v1/verify) @on(Request) @yields(Response) @consumes(json)\n",
        )
        .unwrap();
        assert_eq!(model.units.len(), 8);
        let clock = model
            .units
            .values()
            .find(|unit| unit.label == "clock")
            .unwrap();
        assert_eq!(clock.kind, crate::UnitKind::Class);
        assert_eq!(clock.java_package, "com.example.demo");
        let port = model
            .units
            .values()
            .find(|unit| unit.label == "port")
            .unwrap();
        assert_eq!(port.kind, crate::UnitKind::Interface);
        assert_eq!(port.java_package, "com.example.demo.ports");
        let service = model
            .units
            .values()
            .find(|unit| unit.label == "billing_service")
            .unwrap();
        assert_eq!(service.kind, crate::UnitKind::Service);
        assert_eq!(service.java_type, "BillingService");
        assert_eq!(service.java_package, "com.example.demo.service");
        let test = model
            .units
            .values()
            .find(|unit| unit.label == "parser")
            .unwrap();
        assert_eq!(test.kind, crate::UnitKind::Test);
        assert_eq!(test.java_type, "ParserTest");
        assert_eq!(test.java_package, "com.example.demo");
        let integration_test = model
            .units
            .values()
            .find(|unit| unit.label == "checkout")
            .unwrap();
        assert_eq!(integration_test.kind, crate::UnitKind::IntegrationTest);
        assert_eq!(integration_test.java_type, "CheckoutIT");
        assert_eq!(integration_test.java_package, "com.example.demo");
        let sealed = model
            .units
            .values()
            .find(|unit| unit.label == "outcome")
            .unwrap();
        assert_eq!(sealed.kind, crate::UnitKind::Sealed);
        assert_eq!(sealed.java_type, "Outcome");
        assert_eq!(sealed.java_package, "com.example.demo.domain");
        assert_eq!(sealed.variants, ["Accepted", "Rejected"]);
        let strategy = model
            .units
            .values()
            .find(|unit| unit.label == "post_rule")
            .unwrap();
        assert_eq!(strategy.kind, crate::UnitKind::Strategy);
        assert_eq!(strategy.variants, ["Featured", "Standard"]);
        assert_eq!(strategy.on.as_deref(), Some("Post"));
        assert_eq!(strategy.yields.as_deref(), Some("Tag"));
        let controller = model
            .units
            .values()
            .find(|unit| unit.label == "verify")
            .unwrap();
        assert_eq!(controller.kind, crate::UnitKind::Controller);
        assert_eq!(controller.java_type, "VerifyController");
        assert_eq!(controller.java_package, "com.example.demo.web");
        let endpoint = controller.endpoint.as_ref().unwrap();
        assert_eq!(endpoint.method, crate::EndpointMethod::Post);
        assert_eq!(endpoint.path, "/v1/verify");
        assert_eq!(endpoint.accepts.as_deref(), Some("Request"));
        assert_eq!(endpoint.returns.as_deref(), Some("Response"));
        assert_eq!(endpoint.consumes, crate::RequestFormat::Json);
    }

    #[test]
    fn controller_refuses_a_request_body_on_a_bodyless_verb() {
        let error = parse(
            "application Demo\npackage com.example.demo\njava 26\ndialect postgresql\ncontroller Verify @method(get) @on(Request)\n",
        )
        .unwrap_err();
        assert!(error.to_string().contains("does not carry"));
    }

    #[test]
    fn nested_operations_lower_to_typed_operation_nodes() {
        let model = parse(
            r#"
application Notes
package com.example.notes
java 26
dialect postgresql

entity Task {
  id: uuid @pk
  title: string!
  done: boolean

  event TaskDone(id) @id(op_task_done) {
  }

  command CreateTask(title) @id(op_create_task) {
    route: POST /tasks
  }

  query OpenTasks(done) @id(op_open_tasks) {
    orderBy: title
    limit: 100
    route: GET /tasks
  }

  transition CompleteTask(done) @id(op_complete_task) {
    sets: done
    yields: TaskDone
    route: PATCH /tasks/{id}
  }
}
"#,
        )
        .unwrap();
        assert_eq!(model.operations.len(), 4);
        assert!(
            model
                .operations
                .values()
                .any(|operation| matches!(operation.kind, OperationKind::Command(_)))
        );
        assert!(
            model
                .operations
                .values()
                .any(|operation| matches!(operation.kind, OperationKind::Query(_)))
        );
        assert!(
            model
                .operations
                .values()
                .any(|operation| matches!(operation.kind, OperationKind::Transition(_)))
        );
    }
}
