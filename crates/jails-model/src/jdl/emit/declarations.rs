//! The declaration families: the app header, the project-level declarations,
//! enums, and an entity with everything nested inside it.
//!
//! Split from [`super`] because it reopened a closed ratchet at 878 lines --
//! and the seam the board asks for is by *subject*, so this file is what a
//! reader looks at to answer "how is an entity written back", without the
//! operation and component renderers in the way.

use super::operations::{owner, write_operation};
use super::{field_label, json, refuse, type_token, value};
use crate::ConstraintKind;
use crate::id::StableId;
use crate::model::AppModel;
use crate::model::FieldDefault;
use crate::projection::ProjectionKind;
use crate::relation::ReferentialAction;
use crate::{Diagnostic, EntityId};
use std::collections::BTreeMap;
use std::fmt::Write as _;

pub(super) fn write_app(model: &AppModel, out: &mut String, refusals: &mut Vec<Diagnostic>) {
    let project = &model.project;
    if project.layout != crate::Layout::default() {
        refuse(
            refusals,
            "$.project.layout",
            "a per-project layer layout",
            "the layout reaches the model from `jails.toml`; keep that file and re-adopt after the upgrade",
        );
    }
    let storage = match project.dialect.as_str() {
        "postgresql" => "postgres",
        "h2" => "h2",
        "sqlite" => "sqlite",
        "none" | "" => "none",
        other => {
            refuse(
                refusals,
                "$.project.dialect",
                &format!("the SQL dialect `{other}`"),
                "use postgresql, h2, sqlite, or none",
            );
            "none"
        }
    };
    let _ = writeln!(out, "app {} @id({}) {{", project.name, project.id.as_str());
    let _ = writeln!(out, "  pkg {}", project.base_package);
    let _ = writeln!(out, "  java {}", project.java_release);
    let _ = writeln!(out, "  platform {}", project.platform);
    let _ = writeln!(out, "  build {}", project.build);
    let _ = writeln!(out, "  storage {storage}");
    out.push_str("}\n\n");
}

/// The storage axis materialises its own capability, so re-declaring it would
/// be a second spelling of one fact -- and the linker would then see two.
fn derived_capability(dialect: &str) -> Option<&'static str> {
    match dialect {
        "postgresql" => Some("db"),
        "h2" => Some("h2"),
        "sqlite" => Some("sqlite"),
        _ => None,
    }
}

pub(super) fn write_project_declarations(
    model: &AppModel,
    out: &mut String,
    refusals: &mut Vec<Diagnostic>,
) {
    let implied = derived_capability(&model.project.dialect);
    for capability in model.capabilities.values() {
        if let Some(package) = &capability.java_package {
            refuse(
                refusals,
                format!("$.capabilities.{}", capability.label),
                &format!("a capability package (`{package}`)"),
                "v1 has no per-declaration package; plan a canonical placement move or eject the unit",
            );
        }
        if implied == Some(capability.kind.as_str()) {
            continue;
        }
        let instance = capability
            .name
            .as_deref()
            .map(|name| format!(" {name}"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "cap {}{instance} @id({})",
            capability.kind,
            capability.id.as_str()
        );
    }
    if !model.capabilities.is_empty() {
        out.push('\n');
    }
    for dependency in model.dependencies.values() {
        let version = dependency
            .version
            .as_deref()
            .map(|version| format!(" @version({})", json(version)))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "dep {}:{} @id({}){version} @scope({})",
            dependency.group,
            dependency.artifact,
            dependency.id.as_str(),
            scope(dependency.scope)
        );
    }
    if !model.dependencies.is_empty() {
        out.push('\n');
    }
    for setting in model.settings.values() {
        let _ = writeln!(
            out,
            "prop {} = {} @id({}) @target({})",
            setting.key,
            json(&setting.value),
            setting.id.as_str(),
            setting.target.label()
        );
    }
    if !model.settings.is_empty() {
        out.push('\n');
    }
}

pub(super) fn write_enums(model: &AppModel, out: &mut String) {
    for entity in model
        .entities
        .values()
        .filter(|entity| entity.facets.contains(&crate::Facet::Enum))
    {
        let _ = writeln!(
            out,
            "enum {} @id({}) {{",
            entity.names.java_type,
            entity.id.as_str()
        );
        for constant in &entity.enum_constants {
            if constant.wire_value() == constant.java_name {
                let _ = writeln!(out, "  {}", constant.java_name);
            } else {
                let _ = writeln!(
                    out,
                    "  {} = {}",
                    constant.java_name,
                    json(constant.wire_value())
                );
            }
        }
        out.push_str("}\n\n");
    }
}

pub(super) fn write_entities(model: &AppModel, out: &mut String, refusals: &mut Vec<Diagnostic>) {
    let projections = projections_by_entity(model);
    for entity in model
        .entities
        .values()
        .filter(|entity| !entity.facets.contains(&crate::Facet::Enum))
    {
        if let Some(package) = &entity.java_package {
            refuse(
                refusals,
                format!("$.entities.{}", entity.label),
                &format!("an entity package (`{package}`)"),
                "v1 has no per-declaration package; plan a canonical placement move",
            );
        }
        let retired = if entity.active { "" } else { " @retired" };
        let _ = writeln!(
            out,
            "entity {} @id({}){retired} {{",
            entity.names.java_type,
            entity.id.as_str()
        );
        if let Some(kinds) = projections.get(&entity.id) {
            for kind in kinds {
                let _ = writeln!(out, "  use {kind}");
            }
        }
        let _ = writeln!(out, "  table {}", json(&entity.names.sql_table));
        for field in &entity.fields {
            let _ = writeln!(out, "  {}", field_line(field, refusals, &entity.label));
        }
        for constraint in entity.constraints.values() {
            let keyword = match constraint.kind {
                ConstraintKind::PrimaryKey => "pk",
                ConstraintKind::Unique => "unique",
            };
            let columns = constraint
                .fields
                .iter()
                .map(|field| field_label(model, &entity.id, field).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                out,
                "  {keyword} [{columns}] @id({}) @map({})",
                constraint.id.as_str(),
                json(&constraint.sql_name)
            );
        }
        for index in entity.indexes.values() {
            let columns = index
                .columns
                .iter()
                .map(|column| {
                    let label = field_label(model, &entity.id, &column.field);
                    match column.direction {
                        crate::IndexDirection::Desc => format!("{label} desc"),
                        crate::IndexDirection::Asc => label.to_string(),
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                out,
                "  index [{columns}] @id({}) @map({})",
                index.id.as_str(),
                json(&index.sql_name)
            );
        }
        for relation in model
            .relations
            .values()
            .filter(|relation| relation.child == entity.id)
        {
            write_relation(model, relation, out, refusals);
        }
        for operation in model.operations.values() {
            if owner(&operation.kind) == Some(&entity.id) {
                write_operation(model, operation, out, "  ");
            }
        }
        out.push_str("}\n\n");
    }
}

fn projections_by_entity(model: &AppModel) -> BTreeMap<EntityId, Vec<String>> {
    let mut grouped: BTreeMap<EntityId, Vec<String>> = BTreeMap::new();
    for projection in model.projections.values() {
        let rendered = match &projection.kind {
            ProjectionKind::Value => "value".to_string(),
            ProjectionKind::Repository => "repo".to_string(),
            ProjectionKind::Service => "service".to_string(),
            ProjectionKind::Dto => "dto".to_string(),
            ProjectionKind::Factory => "factory".to_string(),
            ProjectionKind::Seed => "seed".to_string(),
            ProjectionKind::Http { path } => path
                .as_deref()
                .map(|path| format!("http(path: {})", json(path)))
                .unwrap_or_else(|| "http".to_string()),
            ProjectionKind::Search { fields } => {
                let labels = fields
                    .iter()
                    .map(|field| field_label(model, &projection.entity, field).to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("search(fields: [{labels}])")
            }
        };
        grouped
            .entry(projection.entity.clone())
            .or_default()
            .push(rendered);
    }
    grouped
}

fn field_line(field: &crate::Field, refusals: &mut Vec<Diagnostic>, entity: &str) -> String {
    let mut line = format!(
        "{}: {}{}",
        field.names.java_member,
        type_token(&field.ty),
        if field.required { "" } else { "?" }
    );
    let _ = write!(line, " @id({})", field.id.as_str());
    if let Some(default) = &field.semantics.default
        && !default.derived
    {
        let _ = write!(line, " @default({})", value(&default.value));
    }
    if field.primary_key {
        line.push_str(" @pk");
    }
    if field.semantics.version {
        line.push_str(" @version");
    }
    if let Some(length) = &field.length {
        let min = length.min.map(|min| min.to_string()).unwrap_or_default();
        let max = length.max.map(|max| max.to_string()).unwrap_or_default();
        let _ = write!(line, " @length({min}..{max})");
    }
    if field.semantics.nonnegative {
        line.push_str(" @nonnegative");
    }
    if field.non_blank {
        line.push_str(" @notBlank");
    }
    if field.semantics.positive {
        line.push_str(" @positive");
    }
    if field.indexed {
        line.push_str(" @index");
    }
    if let Some(scope) = &field.semantics.scope {
        if scope.pinned {
            let _ = write!(line, " @scope(claim: {})", json(&scope.claim));
        } else {
            line.push_str(" @scope");
        }
    }
    if field.unique {
        line.push_str(" @unique");
    }
    if field.semantics.updated {
        line.push_str(" @updated");
    }
    let _ = write!(line, " @map({})", json(&field.names.sql_column));
    if matches!(&field.semantics.default, Some(FieldDefault { derived, .. }) if *derived)
        && field.semantics.default.is_some()
        && !field.primary_key
        && !field.semantics.version
        && !field.semantics.updated
    {
        refuse(
            refusals,
            format!("$.entities.{entity}.fields.{}", field.label),
            "a compiler-derived default that is not a key, a version or an updated stamp",
            "state the default explicitly before upgrading",
        );
    }
    line
}

fn write_relation(
    model: &AppModel,
    relation: &crate::Relation,
    out: &mut String,
    refusals: &mut Vec<Diagnostic>,
) {
    let Some(parent) = model.entities.get(&relation.parent) else {
        refuse(
            refusals,
            format!("$.relations.{}", relation.label),
            "a relation whose parent is not in this model",
            "remove the relation or restore its parent entity",
        );
        return;
    };
    let _ = writeln!(
        out,
        "  relation {} to {} @id({}) @map({}) {{",
        crate::lower_camel_case(&relation.label),
        parent.names.java_type,
        relation.id.as_str(),
        json(&relation.sql_name)
    );
    for mapping in &relation.mappings {
        let _ = writeln!(
            out,
            "    map {} -> {}",
            field_label(model, &relation.child, &mapping.local),
            field_label(model, &relation.parent, &mapping.remote)
        );
    }
    // Both always, rather than only when they differ from the default: the
    // parser's default lives on the source type and this one is the linked
    // type, so "same as the default" would be a second opinion about a value
    // the round trip is about to check anyway.
    let _ = writeln!(out, "    on delete {}", action(relation.on_delete));
    let _ = writeln!(out, "    on update {}", action(relation.on_update));
    out.push_str("  }\n");
}

pub(super) fn scope(scope: crate::DependencyScope) -> &'static str {
    match scope {
        crate::DependencyScope::Compile => "compile",
        crate::DependencyScope::Runtime => "runtime",
        crate::DependencyScope::Test => "test",
    }
}

pub(super) fn action(action: ReferentialAction) -> &'static str {
    match action {
        ReferentialAction::Restrict => "restrict",
        ReferentialAction::Cascade => "cascade",
        ReferentialAction::SetNull => "set-null",
    }
}
