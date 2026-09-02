//! Linking for explicit generated-to-reader ownership transfers.
//!
//! An ejection names its boundary one of three ways: a readable path
//! (`Task.repo.fake`), which the boundary registry resolves to the artifact
//! id the compiler emits; an artifact id from generated provenance (`art_*`);
//! or a node id, for a capability whose whole pack moves. Whichever the
//! author wrote, the linked ejection stores the resolved id (JDL v1 §16.4),
//! so everything downstream -- the compiler excluding the artifact from the
//! managed tree, `jails model eject` refusing a second transfer -- compares
//! one spelling.

use crate::boundary::{self, Unresolved};
use crate::id::EjectionId;
use crate::linker::Linker;
use crate::model::Ejection;
use crate::source;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn link(
    declarations: BTreeMap<String, source::Ejection>,
    known_targets: &BTreeSet<&str>,
    resolve: impl Fn(&str) -> Result<String, Unresolved>,
    linker: &mut Linker,
) -> BTreeMap<EjectionId, Ejection> {
    let mut linked = BTreeMap::new();
    let mut targets = BTreeMap::<String, String>::new();
    for (label, declaration) in declarations {
        let path = format!("$.ejections.{label}");
        linker.label(&label, &path);
        linker.register_id(&declaration.id, &format!("{path}.id"));
        let id = linker.ejection_id(&declaration.id, &format!("{path}.id"));
        let target = if declaration.target.starts_with("art_")
            || known_targets.contains(declaration.target.as_str())
        {
            declaration.target
        } else {
            match resolve(&declaration.target) {
                Ok(target) => target,
                Err(Unresolved { message, fix }) => {
                    linker.problem(
                        "model-ejection-target",
                        format!("{path}.target"),
                        format!(
                            "ejection target `{}` is neither a boundary path, a generated artifact nor a semantic implementation boundary: {message}",
                            declaration.target
                        ),
                        fix,
                    );
                    declaration.target
                }
            }
        };
        if let Some(first) = targets.insert(target.clone(), path.clone()) {
            linker.problem(
                "model-ejection-collision",
                format!("{path}.target"),
                format!("semantic target `{target}` is already ejected at {first}"),
                "keep one ejection declaration per semantic target",
            );
        }
        if let Some(id) = id {
            linked.insert(id.clone(), Ejection { id, label, target });
        }
    }
    linked
}

/// The resolver the linker hands [`link`]: what a path's first segment names
/// in this model, and which capability is its primary storage.
pub(crate) fn resolver<'a>(
    entities: &'a BTreeMap<crate::id::EntityId, crate::model::Entity>,
    components: &'a BTreeMap<crate::id::ComponentId, crate::component::Component>,
    capabilities: &'a BTreeMap<crate::id::CapabilityId, crate::model::Capability>,
) -> impl Fn(&str) -> Result<String, Unresolved> + 'a {
    use crate::id::StableId;
    move |path| {
        let storage = capabilities
            .values()
            .find(|capability| capability.kind == "db")
            .map(|capability| capability.id.as_str());
        boundary::resolve(
            path,
            |name| {
                let entity = entities
                    .values()
                    .find(|entity| entity.names.java_type == name)
                    .map(|entity| boundary::Resolved {
                        owner: boundary::Owner::Entity,
                        id: entity.id.as_str().to_string(),
                    });
                let component = components
                    .values()
                    .find(|component| component.name == name)
                    .map(|component| boundary::Resolved {
                        owner: boundary::Owner::Component(component.kind),
                        id: component.id.as_str().to_string(),
                    });
                match (entity, component) {
                    (Some(_), Some(_)) => Err(format!(
                        "`{name}` names both an entity and a component, so the path is ambiguous"
                    )),
                    (entity, component) => Ok(entity.or(component)),
                }
            },
            storage,
        )
    }
}
