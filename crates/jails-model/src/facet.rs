//! The facet set's defaults.
//!
//! **A facet and a projection are one fact recorded twice**, and the
//! projection is the authority: `projection::link` derives `entity.facets`
//! from the projections it built, and the facet set is the compatibility view
//! the emitters read. Both come from the source on every link, so nothing
//! maintains one without the other.

pub(crate) const fn active_entity() -> bool {
    true
}
