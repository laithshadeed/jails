//! Entity-facet mutation policy.

use crate::{AppModel, EntityId, Facet, StableId};

pub(crate) const fn active_entity() -> bool {
    true
}

pub(crate) fn add(
    model: &mut AppModel,
    entity: EntityId,
    facet: Facet,
    projection: Option<crate::Projection>,
) -> Result<(), String> {
    let target = model
        .entities
        .get_mut(&entity)
        .ok_or_else(|| format!("entity id `{entity}` does not exist"))?;
    crate::model::refuse_retired_entity(target)?;
    if !target.facets.insert(facet) {
        return Err(format!("facet `{facet:?}` already exists on `{entity}`"));
    }
    if let Some(projection) = projection {
        if projection.entity != entity {
            return Err(format!(
                "projection `{}` does not belong to entity `{entity}`",
                projection.id
            ));
        }
        model.projections.insert(projection.id.clone(), projection);
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
    // The `use` declaration and the facet are one thing in the source, so
    // removing the facet removes the projection it carried.
    let marker = match facet {
        Facet::Record | Facet::Enum | Facet::Events => None,
        Facet::Factory => Some("factory"),
        Facet::Dto => Some("dto"),
        Facet::Repository => Some("repo"),
        Facet::Service => Some("service"),
        Facet::Http => Some("http"),
        Facet::Search => Some("search"),
        Facet::Seed => Some("seed"),
    };
    if let Some(marker) = marker {
        model.projections.retain(|_, projection| {
            projection.entity != entity || projection.kind.label() != marker
        });
    }
    Ok(())
}
