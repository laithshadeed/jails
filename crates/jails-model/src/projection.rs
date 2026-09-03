//! Closed projection vocabulary and post-collection selector expansion.

use crate::id::{EntityId, FieldId, ProjectionId, StableId};
use crate::linker::Linker;
use crate::model::{BuiltinType, Entity, Facet, TypeRef};
use crate::source;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Projection {
    pub id: ProjectionId,
    pub entity: EntityId,
    pub kind: ProjectionKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProjectionKind {
    Value,
    Repository,
    Service,
    Http { path: Option<String> },
    Dto,
    Factory,
    Search { fields: Vec<FieldId> },
    Seed,
}

impl ProjectionKind {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Value => "value",
            Self::Repository => "repo",
            Self::Service => "service",
            Self::Http { .. } => "http",
            Self::Dto => "dto",
            Self::Factory => "factory",
            Self::Search { .. } => "search",
            Self::Seed => "seed",
        }
    }
}

pub(crate) fn link(
    local: BTreeMap<EntityId, Vec<source::Projection>>,
    global: Vec<source::ProjectionRule>,
    entities: &mut BTreeMap<EntityId, Entity>,
    labels: &BTreeMap<String, EntityId>,
    platform: &str,
    dialect: &str,
    linker: &mut Linker,
) -> BTreeMap<ProjectionId, Projection> {
    let selectable = labels
        .iter()
        .filter(|(_, id)| {
            entities
                .get(*id)
                .is_some_and(|entity| !entity.facets.contains(&Facet::Enum))
        })
        .map(|(label, id)| (label.clone(), id.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut selected = BTreeMap::<(EntityId, String), (source::Projection, String)>::new();

    for (entity, projections) in local {
        for (position, projection) in projections.into_iter().enumerate() {
            select(
                &mut selected,
                entity.clone(),
                projection,
                format!("$.entities.{entity}.projections[{position}]"),
                linker,
            );
        }
    }

    for (rule_position, rule) in global.into_iter().enumerate() {
        let path = format!("$.projection_rules[{rule_position}]");
        let mut targets = match rule.selector {
            source::ProjectionSelector::All => selectable.values().cloned().collect::<Vec<_>>(),
            source::ProjectionSelector::Named(names) => {
                resolve_names(&names, &format!("{path}.selector"), &selectable, linker)
            }
        };
        let excluded = resolve_names(&rule.except, &format!("{path}.except"), &selectable, linker)
            .into_iter()
            .collect::<BTreeSet<_>>();
        targets.retain(|entity| !excluded.contains(entity));
        for entity in targets {
            for (projection_position, projection) in rule.projections.iter().cloned().enumerate() {
                select(
                    &mut selected,
                    entity.clone(),
                    projection,
                    format!("{path}.projections[{projection_position}]"),
                    linker,
                );
            }
        }
    }

    let mut projections = BTreeMap::new();
    for ((entity_id, kind), (source, path)) in selected {
        let Some(entity) = entities.get(&entity_id) else {
            continue;
        };
        let linked_kind = lower(&source, entity, &path, linker);
        let Some(linked_kind) = linked_kind else {
            continue;
        };
        let raw_id = format!("prj_{}_{}", entity_id.as_str(), kind);
        linker.register_id(&raw_id, &format!("{path}.id"));
        let Some(id) = linker.projection_id(&raw_id, &format!("{path}.id")) else {
            continue;
        };
        projections.insert(
            id.clone(),
            Projection {
                id,
                entity: entity_id,
                kind: linked_kind,
            },
        );
    }

    for projection in projections.values() {
        if let Some(entity) = entities.get_mut(&projection.entity) {
            entity.facets.insert(compatibility_facet(&projection.kind));
        }
    }
    validate_prerequisites(&projections, entities, platform, dialect, linker);
    projections
}

fn resolve_names(
    names: &[String],
    path: &str,
    labels: &BTreeMap<String, EntityId>,
    linker: &mut Linker,
) -> Vec<EntityId> {
    let mut seen = BTreeSet::new();
    names
        .iter()
        .filter_map(|name| {
            let id = labels.get(name).cloned().or_else(|| {
                linker.problem(
                    "model-projection-selector-reference",
                    path,
                    format!("`{name}` does not name an entity"),
                    "name an entity declared anywhere in this document",
                );
                None
            })?;
            if !seen.insert(id.clone()) {
                linker.problem(
                    "model-projection-selector-duplicate",
                    path,
                    format!("entity `{name}` is selected more than once"),
                    "keep each selector name once",
                );
                return None;
            }
            Some(id)
        })
        .collect()
}

fn select(
    selected: &mut BTreeMap<(EntityId, String), (source::Projection, String)>,
    entity: EntityId,
    projection: source::Projection,
    path: String,
    linker: &mut Linker,
) {
    for expanded in expand(projection) {
        let key = (entity.clone(), expanded.kind.clone());
        if let Some((first, first_path)) = selected.get(&key) {
            if first != &expanded {
                linker.problem(
                    "model-projection-configuration-conflict",
                    &path,
                    format!(
                        "projection `{}` is configured differently at {first_path}",
                        expanded.kind
                    ),
                    "make duplicate projection arguments identical",
                );
            }
        } else {
            selected.insert(key, (expanded, path.clone()));
        }
    }
}

/// The profiles, resolved to the projections they stand for.
///
/// **A scaffold carries `dto`, and that is the row worth explaining.**
/// Without the request records the controller binds the domain row, so a
/// caller can set the audit columns and the optimistic-lock version -- values
/// the server is the authority on. A scaffold is the profile that means "the
/// whole resource, served", so the boundary belongs in it rather than in
/// something an author has to remember to add beside it. A bare `use http`
/// still binds the row, which is the shape somebody asking for one controller
/// and nothing else chose.
fn expand(projection: source::Projection) -> Vec<source::Projection> {
    fn plain(kind: &str) -> source::Projection {
        source::Projection {
            kind: kind.to_string(),
            fields: Vec::new(),
            path: None,
        }
    }
    if projection.kind != "scaffold" {
        return vec![projection];
    }
    vec![
        plain("repo"),
        plain("service"),
        plain("dto"),
        // **`seed` is deliberately not here**, and the reason is a limit of
        // this function rather than a judgement about fixtures: a profile
        // expands before the app block is read, and `seed` needs `storage
        // postgres`, so putting it in would refuse every scaffold on a
        // project with no database.
        source::Projection {
            kind: "http".to_string(),
            fields: Vec::new(),
            path: projection.path,
        },
    ]
}

fn lower(
    source: &source::Projection,
    entity: &Entity,
    path: &str,
    linker: &mut Linker,
) -> Option<ProjectionKind> {
    let no_arguments = || source.path.is_none() && source.fields.is_empty();
    let kind = match source.kind.as_str() {
        "value" if no_arguments() => ProjectionKind::Value,
        "repo" if no_arguments() => ProjectionKind::Repository,
        "service" if no_arguments() => ProjectionKind::Service,
        "http" if source.fields.is_empty() => ProjectionKind::Http {
            path: source.path.clone(),
        },
        "dto" if no_arguments() => ProjectionKind::Dto,
        "factory" if no_arguments() => ProjectionKind::Factory,
        "seed" if no_arguments() => ProjectionKind::Seed,
        "search" if source.path.is_none() && !source.fields.is_empty() => {
            let fields = resolve_fields(&source.fields, entity, path, linker);
            ProjectionKind::Search { fields }
        }
        "search" => {
            refuse_arguments(
                linker,
                path,
                "projection `search` requires a non-empty `fields` argument",
                "write `search(fields: [title])`",
            );
            return None;
        }
        known @ ("value" | "repo" | "service" | "dto" | "factory" | "seed") => {
            refuse_arguments(
                linker,
                path,
                format!("projection `{known}` accepts no arguments"),
                "remove its argument list",
            );
            return None;
        }
        other => {
            linker.problem(
                "model-projection-kind",
                path,
                format!("unknown projection `{other}`"),
                "use value, repo, service, http, dto, factory, search, seed, or scaffold",
            );
            return None;
        }
    };
    Some(kind)
}

fn resolve_fields(
    labels: &[String],
    entity: &Entity,
    path: &str,
    linker: &mut Linker,
) -> Vec<FieldId> {
    let mut seen = BTreeSet::new();
    labels
        .iter()
        .filter_map(|label| {
            let field = entity
                .fields
                .iter()
                .find(|field| &field.label == label)
                .map(|field| field.id.clone())
                .or_else(|| {
                    linker.problem(
                        "model-projection-field-reference",
                        path,
                        format!("`{label}` is not a field on `{}`", entity.label),
                        "name a field on the selected entity",
                    );
                    None
                })?;
            if !seen.insert(field.clone()) {
                linker.problem(
                    "model-projection-field-duplicate",
                    path,
                    format!("search field `{label}` is repeated"),
                    "keep each search field once",
                );
                return None;
            }
            Some(field)
        })
        .collect()
}

pub(crate) fn compatibility_facet(kind: &ProjectionKind) -> Facet {
    match kind {
        ProjectionKind::Value => Facet::Record,
        ProjectionKind::Repository => Facet::Repository,
        ProjectionKind::Service => Facet::Service,
        ProjectionKind::Http { .. } => Facet::Http,
        ProjectionKind::Dto => Facet::Dto,
        ProjectionKind::Factory => Facet::Factory,
        ProjectionKind::Seed => Facet::Seed,
        ProjectionKind::Search { .. } => Facet::Search,
    }
}

fn validate_prerequisites(
    projections: &BTreeMap<ProjectionId, Projection>,
    entities: &BTreeMap<EntityId, Entity>,
    platform: &str,
    dialect: &str,
    linker: &mut Linker,
) {
    let selected = projections
        .values()
        .map(|projection| {
            (
                (projection.entity.clone(), projection.kind.label()),
                projection,
            )
        })
        .collect::<BTreeMap<_, _>>();
    for projection in projections.values() {
        let entity = &entities[&projection.entity];
        let has = |kind| selected.contains_key(&(projection.entity.clone(), kind));
        let problem = |linker: &mut Linker, requirement: &str| {
            linker.problem(
                "model-projection-prerequisite",
                format!("$.projections.{}", projection.id),
                format!(
                    "projection `{}` on `{}` requires {requirement}",
                    projection.kind.label(),
                    entity.label
                ),
                "add the prerequisite or remove this projection",
            );
        };
        match &projection.kind {
            ProjectionKind::Repository if !has_primary_key(entity) => {
                problem(linker, "a primary key")
            }
            ProjectionKind::Service if !has("repo") => problem(linker, "`repo`"),
            ProjectionKind::Http { .. }
                if !has("repo") || !has("service") || platform != "spring" =>
            {
                problem(linker, "`repo`, `service`, and platform spring")
            }
            ProjectionKind::Dto if platform != "spring" => problem(linker, "platform spring"),
            ProjectionKind::Search { fields }
                if dialect != "postgresql"
                    || fields.iter().any(|id| {
                        entity
                            .field(id)
                            .is_none_or(|field| field.ty != TypeRef::Builtin(BuiltinType::String))
                    }) =>
            {
                problem(linker, "storage postgres and only string fields")
            }
            ProjectionKind::Seed
                if !has("repo") || dialect != "postgresql" || platform != "spring" =>
            {
                problem(linker, "`repo`, storage postgres, and platform spring")
            }
            _ => {}
        }
    }
}

fn has_primary_key(entity: &Entity) -> bool {
    entity.fields.iter().any(|field| field.primary_key)
        || entity
            .constraints
            .values()
            .any(|constraint| constraint.kind == crate::ConstraintKind::PrimaryKey)
}

/// A projection whose argument list is not what its kind takes.
///
/// One code, so one constructor: `search` wants a `fields` list and the other
/// six want none, and which way round it went wrong is the sentence's job.
fn refuse_arguments(
    linker: &mut Linker,
    path: &str,
    message: impl Into<String>,
    fix: &'static str,
) {
    linker.problem("model-projection-arguments", path, message, fix);
}
