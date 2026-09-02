//! Linking rules for optional application capabilities.

use crate::id::CapabilityId;
use crate::linker::Linker;
use crate::model::Capability;
use crate::source;
use std::collections::BTreeMap;

pub(crate) fn link(
    declarations: BTreeMap<String, source::Capability>,
    base_package: &str,
    linker: &mut Linker,
) -> BTreeMap<CapabilityId, Capability> {
    let mut capabilities = BTreeMap::new();
    let mut kinds = BTreeMap::<String, String>::new();
    for (label, capability) in declarations {
        let path = format!("$.capabilities.{label}");
        linker.label(&label, &path);
        linker.label(&capability.kind, &format!("{path}.kind"));
        linker.register_id(&capability.id, &format!("{path}.id"));
        let Some(id) = linker.capability_id(&capability.id, &format!("{path}.id")) else {
            continue;
        };
        if let Some(first) = kinds.insert(capability.kind.clone(), path.clone()) {
            linker.problem(
                "model-capability-collision",
                format!("{path}.kind"),
                format!(
                    "capability kind `{}` is already declared at {first}",
                    capability.kind
                ),
                "keep one declaration for each capability kind",
            );
        }
        if let Some(name) = &capability.name {
            linker.java_type(name, &format!("{path}.name"));
        }
        let java_package = capability.package.map(|package| {
            let resolved = if package.is_empty() {
                base_package.to_string()
            } else {
                format!("{base_package}.{package}")
            };
            linker.java_package(&resolved, &format!("{path}.package"));
            resolved
        });
        capabilities.insert(
            id.clone(),
            Capability {
                id,
                label,
                kind: capability.kind,
                name: capability.name,
                java_package,
            },
        );
    }
    capabilities
}
