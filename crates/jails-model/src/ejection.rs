//! Linking for explicit generated-to-reader ownership transfers.

use crate::id::EjectionId;
use crate::linker::Linker;
use crate::model::Ejection;
use crate::source;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn link(
    declarations: BTreeMap<String, source::Ejection>,
    known_targets: &BTreeSet<&str>,
    linker: &mut Linker,
) -> BTreeMap<EjectionId, Ejection> {
    let mut linked = BTreeMap::new();
    let mut targets = BTreeMap::<String, String>::new();
    for (label, declaration) in declarations {
        let path = format!("$.ejections.{label}");
        linker.label(&label, &path);
        linker.register_id(&declaration.id, &format!("{path}.id"));
        let id = linker.ejection_id(&declaration.id, &format!("{path}.id"));
        if !declaration.target.starts_with("art_")
            && !known_targets.contains(declaration.target.as_str())
        {
            linker.problem(
                "model-ejection-target",
                format!("{path}.target"),
                format!(
                    "ejection target `{}` is neither a generated artifact nor a semantic implementation boundary",
                    declaration.target
                ),
                "use an ejectable artifact or implementation-boundary id reported by generated provenance",
            );
        }
        if let Some(first) = targets.insert(declaration.target.clone(), path.clone()) {
            linker.problem(
                "model-ejection-collision",
                format!("{path}.target"),
                format!(
                    "semantic target `{}` is already ejected at {first}",
                    declaration.target
                ),
                "keep one ejection declaration per semantic target",
            );
        }
        if let Some(id) = id {
            linked.insert(
                id.clone(),
                Ejection {
                    id,
                    label,
                    target: declaration.target,
                },
            );
        }
    }
    linked
}
