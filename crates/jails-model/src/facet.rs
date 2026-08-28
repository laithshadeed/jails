//! Entity-facet mutation policy.

use crate::{AppModel, EntityId, Facet, StableId};

pub(crate) const fn active_entity() -> bool {
    true
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
    Ok(())
}
