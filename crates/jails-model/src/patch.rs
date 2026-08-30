//! `ModelPatch` — every way the model may change, as one closed enum.
//!
//! **Desired state is edited by applying one of these, never by rewriting the
//! model in place.** A patch is what a CLI mutation lowers to, what `apply`
//! validates against the current model, and what the exact plan records as its
//! input, so a mutation that has no variant here is a mutation that cannot
//! happen — which is what keeps `.jails/model.jdl` and the CLI from becoming
//! two editable sources of truth.
//!
//! The variants are deliberately fine-grained: `ReplaceField` carries exactly
//! one typed policy, `RemoveEntity` carries a storage decision, an index is a
//! stable child rather than an entity rewrite. Coarser variants would be
//! easier to build and would lose the evolution *intent* — retire versus drop,
//! preserve-column versus single-cutover, which backfill file a required
//! column is proved against — and that intent is the only thing that tells the
//! compiler whether a migration is owed.
//!
//! `ReplaceModel` is the documented exception and says on itself why it exists
//! and why nothing else may use it.

use crate::id::{
    CapabilityId, ComponentId, DependencyId, EntityId, FieldId, IndexId, OperationId, SettingId,
    UnitId,
};
use crate::model::{Capability, Dependency, Ejection, Entity, Facet, Field, Index, Setting};
use crate::operation::Operation;
use crate::{Component, SourceUnit};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelPatch {
    Batch(Vec<ModelPatch>),
    /// Replace the whole model, for a source-language upgrade.
    ///
    /// **The one patch that is not an edit**, and it exists for exactly one
    /// caller: `jails model upgrade --to 1` rewrites `.jails/model.jdl` from
    /// the pre-v1 draft into JDL v1, and the two dialects do not link to the
    /// same model -- v1 materializes projections, links operation parameters
    /// and reads `storage` as a capability, none of which any sequence of
    /// field- and entity-level patches describes.
    ///
    /// It is deliberately not general. Every other mutation carries evolution
    /// *intent* the new model cannot state -- retire versus drop, a backfill
    /// policy, which column a rename preserves -- so routing an ordinary edit
    /// through here would lose the one thing the patch is for. The upgrade has
    /// no such intent: the model is the same application, re-read by a
    /// stricter parser, and `jails_model::upgrade` proves every stable ID and
    /// physical name survived before this variant is ever built.
    ReplaceModel(Box<crate::AppModel>),
    AddCapability(Capability),
    RemoveCapability(CapabilityId),
    AddDependency(Dependency),
    RemoveDependency(DependencyId),
    SetSetting(Setting),
    RemoveSetting(SettingId),
    AddEjection(Ejection),
    AddUnit(SourceUnit),
    ReplaceUnit(SourceUnit),
    RemoveUnit(UnitId),
    AddComponent(Component),
    ReplaceComponent(Component),
    RemoveComponent(ComponentId),
    AddEntity(Entity),
    AddFacet {
        entity: EntityId,
        facet: Facet,
    },
    RemoveFacet {
        entity: EntityId,
        facet: Facet,
    },
    RemoveEntity(EntityId),
    RetireEntity {
        entity: EntityId,
        policy: StorageRetirementPolicy,
    },
    ReviveEntity {
        entity: EntityId,
        confirmed_table: String,
    },
    AddField {
        entity: EntityId,
        field: Field,
        policy: FieldAddPolicy,
        placement: FieldPlacement,
    },
    AddIndex {
        entity: EntityId,
        index: Index,
    },
    /// One declared relation, which the compiler lowers to a foreign key.
    ///
    /// Whole rather than `{ child, parent, mappings }` for the same reason
    /// [`Self::AddIndex`] carries an [`Index`]: the linker has already
    /// resolved both sides to stable field IDs and assigned the constraint its
    /// SQL name, and re-deriving any of that here would be a second answer to
    /// a question the model already answered.
    AddRelation(crate::Relation),
    RemoveIndex {
        entity: EntityId,
        index: IndexId,
        confirmed_name: String,
    },
    ReplaceField {
        entity: EntityId,
        field: FieldId,
        replacement: Field,
        policy: FieldEvolutionPolicy,
    },
    RemoveField {
        entity: EntityId,
        field: FieldId,
        confirmed_column: String,
    },
    AddOperation(Operation),
    RemoveOperation(OperationId),
    RenameEntityProjection {
        entity: EntityId,
        label: Option<String>,
        java: Option<String>,
        table: Option<String>,
    },
}

/// Explicit data policy for adding a column to an already accepted table.
///
/// Nullable columns need no data rewrite. Required columns carry the typed
/// literal used to backfill existing rows before `not null` is enforced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldAddPolicy {
    Nullable,
    BackfillLiteral(String),
    ReaderOwnedSql(Vec<u8>),
}

/// Where a new field lands among an entity's existing ones.
///
/// A record's component order is ABI, so this is not presentation. The
/// frontend that edits the source is the only thing that knows: it writes the
/// bytes, and the patched model has to equal what re-parsing those bytes
/// yields or `model check --frozen` fails on the very next command.
///
/// This replaced a heuristic that read the *existing* order and guessed --
/// "already sorted by label" was taken to mean "the source states no order",
/// which is true right up until a JDL entity happens to be declared
/// alphabetically. Then appending `delta` to `alpha, beta, gamma` put it
/// third in the model and fourth in the file. Only the frontend can answer
/// this, so it is asked.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldPlacement {
    /// The source states field order and the edit appended: JDL v1, whose
    /// parser records the declaration order its CST walked.
    Last,
    /// The source states no order, so re-parsing sorts by label: a
    /// `.jails/model.toml` table, and the pre-v1 JDL draft, which reaches the
    /// linker by rendering that same TOML.
    ByLabel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldEvolutionPolicy {
    Rename { column: ColumnRenamePolicy },
    ChangeType { strategy: TypeChangeStrategy },
    SetNullability { backfill_sql: Option<Vec<u8>> },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnRenamePolicy {
    Preserve,
    SingleCutover,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeChangeStrategy {
    Safe,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageRetirementPolicy {
    Preserve,
    Drop { confirmed_table: String },
}
