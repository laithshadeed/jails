//! Which model node a legacy line names.
//!
//! Split out because it is a different question from the translation beside
//! it. Translating says how a pre-v1 line is spelled in v1; this says which
//! declaration in the already-linked legacy model that line *is*, which is
//! what lets the rewriter write the effective stable ID out explicitly instead
//! of letting v1 derive a new one.
//!
//! Every lookup is by label, and every label comes from `super::label` -- the
//! same function the pre-v1 parser uses to build the model being searched. A
//! second spelling rule here would drift, so there is not one.

use super::{Cursor, label, refuse};
use crate::{Diagnostics, StableId};

impl Cursor<'_> {
    pub(super) fn entity(&self, number: usize, label: &str) -> Result<&crate::Entity, Diagnostics> {
        self.legacy
            .entities
            .values()
            .find(|entity| entity.label == label)
            .ok_or_else(|| missing(number, "entity", label))
    }

    pub(super) fn entity_id(&self, number: usize, label: &str) -> Result<String, Diagnostics> {
        Ok(self.entity(number, label)?.id.as_str().to_string())
    }

    pub(super) fn field_node(
        &self,
        number: usize,
        name: &str,
    ) -> Result<&crate::Field, Diagnostics> {
        let entity = self.entity(number, &self.entity)?;
        let label = label(name);
        entity
            .fields
            .iter()
            .find(|field| field.label == label)
            .ok_or_else(|| missing(number, "field", &label))
    }

    pub(super) fn index_node(
        &self,
        number: usize,
        columns: &str,
    ) -> Result<&crate::Index, Diagnostics> {
        let entity = self.entity(number, &self.entity)?;
        // Matched on the column list rather than a label: pre-v1 labels an
        // index after its ID and v1 after its columns, so the columns are the
        // one thing both spellings agree on.
        let wanted = columns
            .split(',')
            .map(|column| label(column.split_whitespace().next().unwrap_or_default()))
            .collect::<Vec<_>>();
        entity
            .indexes
            .values()
            .find(|index| {
                index.columns.len() == wanted.len()
                    && index.columns.iter().zip(&wanted).all(|(column, name)| {
                        entity
                            .field(&column.field)
                            .is_some_and(|field| &field.label == name)
                    })
            })
            .ok_or_else(|| missing(number, "index", columns))
    }

    pub(super) fn operation_id(&self, number: usize, label: &str) -> Result<String, Diagnostics> {
        self.legacy
            .operations
            .values()
            .find(|operation| operation.label == label)
            .map(|operation| operation.id.as_str().to_string())
            .ok_or_else(|| missing(number, "operation", label))
    }

    pub(super) fn capability_id(&self, number: usize, label: &str) -> Result<String, Diagnostics> {
        self.legacy
            .capabilities
            .values()
            .find(|capability| capability.label == label)
            .map(|capability| capability.id.as_str().to_string())
            .ok_or_else(|| missing(number, "capability", label))
    }

    pub(super) fn setting_id(
        &self,
        number: usize,
        key: &str,
        target: &str,
    ) -> Result<String, Diagnostics> {
        self.legacy
            .settings
            .values()
            .find(|setting| setting.key == key && setting.target.label() == target)
            .map(|setting| setting.id.as_str().to_string())
            .ok_or_else(|| missing(number, "setting", key))
    }
}

fn missing(number: usize, kind: &str, label: &str) -> Diagnostics {
    refuse(
        number,
        format!("the legacy model has no {kind} `{label}`"),
        "this is an upgrade defect, not a source one: the rewriter and the legacy parser \
         disagree about this declaration's label. Report it rather than editing the source.",
    )
}
