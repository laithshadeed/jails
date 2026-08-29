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
