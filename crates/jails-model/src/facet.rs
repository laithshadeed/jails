//! Entity-facet mutation policy.
//!
//! **A facet and a projection are one fact recorded twice**, and linking says
//! which way round: `projection::link` derives `entity.facets` from the
//! projections it built, so the projection is the authority and the facet set
//! is the compatibility view the emitters read. A patch that maintained only
//! the facet therefore left the two disagreeing until the next `jails sync`
//! reparsed the whole source and rebuilt both -- so `g repo Note` produced a
//! repository the *emitters* could see and `resource status`, prerequisite
//! validation and `emit_resource_http`'s route lookup could not, and the
//! disagreement healed itself on an unrelated later command. Both are
//! maintained here, together, for that reason.

use crate::projection::{Projection, ProjectionKind};
use crate::{AppModel, EntityId, Facet, StableId};

pub(crate) const fn active_entity() -> bool {
    true
}

/// The argument-free projection a bare facet stands for.
///
/// `None` for the two kinds that carry arguments: a search projection names
/// fields and an HTTP one may pin a route, and inventing either from a facet
/// would be a guess recorded as a declaration. Those arrive as
/// [`crate::ModelPatch::AddProjection`], which carries the whole value.
fn bare_projection(facet: Facet) -> Option<ProjectionKind> {
    match facet {
        Facet::Record => Some(ProjectionKind::Value),
        Facet::Repository => Some(ProjectionKind::Repository),
        Facet::Service => Some(ProjectionKind::Service),
        Facet::Dto => Some(ProjectionKind::Dto),
        Facet::Factory => Some(ProjectionKind::Factory),
        Facet::Seed => Some(ProjectionKind::Seed),
        Facet::Http | Facet::Search | Facet::Enum | Facet::Events => None,
    }
}

/// The id `projection::link` would have minted for this pairing.
///
/// Derived the same way rather than allocated afresh, so a facet added by a
/// patch and the same facet arrived at by reparsing the source are the same
/// node with the same identity -- which is what keeps a plan digest stable
/// across the two routes to it.
fn projection_id(entity: &EntityId, kind: &ProjectionKind) -> String {
    format!("prj_{}_{}", entity.as_str(), kind.label())
}

pub(crate) fn add(model: &mut AppModel, entity: EntityId, facet: Facet) -> Result<(), String> {
    let target = model
        .entities
        .get_mut(&entity)
        .ok_or_else(|| format!("entity id `{entity}` does not exist"))?;
    crate::model::refuse_retired_entity(target)?;
    if !target.facets.insert(facet) {
        return Err(format!("facet `{facet:?}` already exists on `{entity}`"));
    }
    if let Some(kind) = bare_projection(facet) {
        let raw = projection_id(&entity, &kind);
        if let Ok(id) = crate::id::ProjectionId::parse(raw) {
            model
                .projections
                .entry(id.clone())
                .or_insert(Projection { id, entity, kind });
        }
    }
    Ok(())
}

pub(crate) fn remove(model: &mut AppModel, entity: EntityId, facet: Facet) -> Result<(), String> {
    crate::model::refuse_ejected_target(model, entity.as_str())?;
    let target = model
        .entities
        .get_mut(&entity)
        .ok_or_else(|| format!("entity id `{entity}` does not exist"))?;
    if !target.facets.remove(&facet) {
        return Err(format!("facet `{facet:?}` does not exist on `{entity}`"));
    }
    // By what the projection *is*, not by the id it would have had: an HTTP or
    // search projection carries arguments, so its id is the only thing a bare
    // facet could reconstruct and the value behind it is not.
    model.projections.retain(|_, projection| {
        projection.entity != entity
            || crate::projection::compatibility_facet(&projection.kind) != facet
    });
    Ok(())
}
