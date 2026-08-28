//! Typed database relations: ordered field mappings and key validation.

use crate::ConstraintKind;
use crate::id::{EntityId, FieldId, RelationId};
use crate::linker::Linker;
use crate::model::Entity;
use crate::source;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Relation {
    pub id: RelationId,
    pub label: String,
    pub child: EntityId,
    pub parent: EntityId,
    pub sql_name: String,
    pub mappings: Vec<RelationMapping>,
    pub on_delete: ReferentialAction,
    pub on_update: ReferentialAction,
    pub cardinality: RelationCardinality,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RelationMapping {
    pub local: FieldId,
    pub remote: FieldId,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReferentialAction {
    Restrict,
    Cascade,
    SetNull,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RelationCardinality {
    ManyToOne,
    OneToOne,
}

pub(crate) fn link(
    declarations: BTreeMap<EntityId, BTreeMap<String, source::Relation>>,
    entities: &BTreeMap<EntityId, Entity>,
    labels: &BTreeMap<String, EntityId>,
    linker: &mut Linker,
) -> BTreeMap<RelationId, Relation> {
    let mut relations = BTreeMap::new();
    let mut sql_names = BTreeMap::<String, String>::new();
    for (child_id, declarations) in declarations {
        let Some(child) = entities.get(&child_id) else {
            continue;
        };
        for (label, declaration) in declarations {
            let path = format!("$.entities.{}.relations.{label}", child.label);
            linker.register_id(&declaration.id, &format!("{path}.id"));
            let id = linker.relation_id(&declaration.id, &format!("{path}.id"));
            if !crate::naming::valid_java_member(&declaration.name) {
                linker.problem(
                    "model-relation-name",
                    &path,
                    format!("`{}` is not a lower-camel relation name", declaration.name),
                    "use a lowerCamel Java identifier",
                );
            }
            let Some(parent_id) = labels.get(&declaration.target).cloned() else {
                linker.problem(
                    "model-relation-target-reference",
                    format!("{path}.target"),
                    format!("`{}` does not name an entity", declaration.target),
                    "name a declared entity",
                );
                continue;
            };
            let Some(parent) = entities.get(&parent_id) else {
                continue;
            };
            if parent.facets.contains(&crate::Facet::Enum) {
                linker.problem(
                    "model-relation-target-reference",
                    format!("{path}.target"),
                    format!("`{}` names an enum, not an entity", declaration.target),
                    "target a stored entity",
                );
                continue;
            }
            let mappings = resolve_mappings(&declaration, child, parent, &path, linker);
            validate_mapping(&mappings, child, parent, &declaration, &path, linker);
            let sql_name = declaration
                .sql_name
                .unwrap_or_else(|| format!("fk_{}_{}", child.names.sql_table, label));
            linker.sql_identifier(&sql_name, &format!("{path}.sql_name"));
            if let Some(first) = sql_names.insert(sql_name.clone(), path.clone()) {
                linker.problem(
                    "model-sql-relation-collision",
                    &path,
                    format!("SQL relation `{sql_name}` is already declared at {first}"),
                    "give each relation a unique physical name",
                );
            }
            let cardinality = if tuple_is_unique(
                child,
                &mappings
                    .iter()
                    .map(|mapping| mapping.local.clone())
                    .collect::<Vec<_>>(),
            ) {
                RelationCardinality::OneToOne
            } else {
                RelationCardinality::ManyToOne
            };
            if let Some(id) = id {
                relations.insert(
                    id.clone(),
                    Relation {
                        id,
                        label,
                        child: child_id.clone(),
                        parent: parent_id,
                        sql_name,
                        mappings,
                        on_delete: action(declaration.on_delete),
                        on_update: action(declaration.on_update),
                        cardinality,
                    },
                );
            }
        }
    }
    validate_cascade_cycles(&relations, entities, linker);
    relations
}

fn resolve_mappings(
    declaration: &source::Relation,
    child: &Entity,
    parent: &Entity,
    path: &str,
    linker: &mut Linker,
) -> Vec<RelationMapping> {
    let mut mappings = Vec::new();
    let mut local_seen = BTreeSet::new();
    let mut remote_seen = BTreeSet::new();
    for (position, mapping) in declaration.mappings.iter().enumerate() {
        let mapping_path = format!("{path}.mappings[{position}]");
        let local_label = normalized_field(&mapping.local, &child.label);
        let remote_label = normalized_field(&mapping.remote, &parent.label);
        let local = field(child, &local_label).or_else(|| {
            linker.problem(
                "model-relation-local-field",
                &mapping_path,
                format!("`{}` is not a field on `{}`", mapping.local, child.label),
                "map a field on the child entity",
            );
            None
        });
        let remote = field(parent, &remote_label).or_else(|| {
            linker.problem(
                "model-relation-remote-field",
                &mapping_path,
                format!("`{}` is not a field on `{}`", mapping.remote, parent.label),
                "map a field on the parent entity",
            );
            None
        });
        let (Some(local), Some(remote)) = (local, remote) else {
            continue;
        };
        if !local_seen.insert(local.clone()) {
            linker.problem(
                "model-relation-local-duplicate",
                &mapping_path,
                format!("local field `{local_label}` is mapped more than once"),
                "map each local field once",
            );
            continue;
        }
        if !remote_seen.insert(remote.clone()) {
            linker.problem(
                "model-relation-remote-duplicate",
                &mapping_path,
                format!("remote field `{remote_label}` is mapped more than once"),
                "map each remote field once",
            );
            continue;
        }
        mappings.push(RelationMapping { local, remote });
    }
    mappings
}

fn validate_mapping(
    mappings: &[RelationMapping],
    child: &Entity,
    parent: &Entity,
    declaration: &source::Relation,
    path: &str,
    linker: &mut Linker,
) {
    if mappings.is_empty() {
        linker.problem(
            "model-relation-empty",
            format!("{path}.mappings"),
            "a relation needs at least one valid mapping",
            "add `map local -> remote`",
        );
        return;
    }
    for (position, mapping) in mappings.iter().enumerate() {
        let local = &child.fields[&mapping.local];
        let remote = &parent.fields[&mapping.remote];
        if local.ty != remote.ty {
            linker.problem(
                "model-relation-type-mismatch",
                format!("{path}.mappings[{position}]"),
                format!(
                    "local field `{}` and remote field `{}` have different logical types",
                    local.label, remote.label
                ),
                "map fields with the same logical type",
            );
        }
        if !remote.required {
            linker.problem(
                "model-relation-remote-required",
                format!("{path}.mappings[{position}]"),
                format!("remote key field `{}` is nullable", remote.label),
                "target required key fields",
            );
        }
    }
    let required = mappings
        .iter()
        .filter(|mapping| child.fields[&mapping.local].required)
        .count();
    if required != 0 && required != mappings.len() {
        linker.problem(
            "model-relation-partial-nullability",
            format!("{path}.mappings"),
            "a composite foreign-key tuple mixes required and nullable fields",
            "make the entire local tuple required or nullable",
        );
    }
    if (declaration.on_delete == source::ReferentialAction::SetNull
        || declaration.on_update == source::ReferentialAction::SetNull)
        && required != 0
    {
        linker.problem(
            "model-relation-set-null-required",
            path,
            "`set-null` is used with required local fields",
            "make every local field nullable or use restrict/cascade",
        );
    }
    let remote = mappings
        .iter()
        .map(|mapping| mapping.remote.clone())
        .collect::<Vec<_>>();
    if !tuple_is_unique(parent, &remote) {
        linker.problem(
            "model-relation-target-key",
            format!("{path}.mappings"),
            "the remote tuple does not exactly match a primary or unique key",
            "map every field of one parent primary/unique constraint in declared order",
        );
    }
}

fn tuple_is_unique(entity: &Entity, tuple: &[FieldId]) -> bool {
    (tuple.len() == 1
        && entity
            .fields
            .get(&tuple[0])
            .is_some_and(|field| field.primary_key || field.unique))
        || entity.constraints.values().any(|constraint| {
            matches!(
                constraint.kind,
                ConstraintKind::PrimaryKey | ConstraintKind::Unique
            ) && constraint.fields == tuple
        })
}

fn field(entity: &Entity, label: &str) -> Option<FieldId> {
    entity
        .fields
        .values()
        .find(|field| field.label == label)
        .map(|field| field.id.clone())
}

fn normalized_field(path: &str, entity: &str) -> String {
    let field = path.split_once('.').map_or(path, |(qualifier, field)| {
        if semantic_fragment(qualifier) == entity {
            field
        } else {
            path
        }
    });
    semantic_fragment(field)
}

fn semantic_fragment(value: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for (position, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if position > 0 && !separator {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
            separator = false;
        } else if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            separator = false;
        } else if !separator && !output.is_empty() {
            output.push('_');
            separator = true;
        }
    }
    output.trim_matches('_').to_string()
}

const fn action(action: source::ReferentialAction) -> ReferentialAction {
    match action {
        source::ReferentialAction::Restrict => ReferentialAction::Restrict,
        source::ReferentialAction::Cascade => ReferentialAction::Cascade,
        source::ReferentialAction::SetNull => ReferentialAction::SetNull,
    }
}

fn validate_cascade_cycles(
    relations: &BTreeMap<RelationId, Relation>,
    entities: &BTreeMap<EntityId, Entity>,
    linker: &mut Linker,
) {
    for kind in ["delete", "update"] {
        let mut graph = BTreeMap::<EntityId, Vec<EntityId>>::new();
        for relation in relations.values() {
            let required = relation
                .mappings
                .iter()
                .all(|mapping| entities[&relation.child].fields[&mapping.local].required);
            let action = if kind == "delete" {
                relation.on_delete
            } else {
                relation.on_update
            };
            if required && action == ReferentialAction::Cascade {
                graph
                    .entry(relation.child.clone())
                    .or_default()
                    .push(relation.parent.clone());
            }
        }
        if has_cycle(&graph) {
            linker.problem(
                "model-relation-cascade-cycle",
                "$.relations",
                format!("required `{kind} cascade` relations form a cycle"),
                "break the cycle with restrict or a nullable relation",
            );
        }
    }
}

fn has_cycle(graph: &BTreeMap<EntityId, Vec<EntityId>>) -> bool {
    fn visit(
        node: &EntityId,
        graph: &BTreeMap<EntityId, Vec<EntityId>>,
        visiting: &mut BTreeSet<EntityId>,
        visited: &mut BTreeSet<EntityId>,
    ) -> bool {
        if visiting.contains(node) {
            return true;
        }
        if !visited.insert(node.clone()) {
            return false;
        }
        visiting.insert(node.clone());
        let cyclic = graph.get(node).is_some_and(|next| {
            next.iter()
                .any(|next| visit(next, graph, visiting, visited))
        });
        visiting.remove(node);
        cyclic
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    graph
        .keys()
        .any(|node| visit(node, graph, &mut visiting, &mut visited))
}
