use crate::SourceUnit;
use crate::id::{
    CapabilityId, DependencyId, EntityId, FieldId, IndexId, OperationId, SettingId, UnitId,
};
use crate::model::{Capability, Dependency, Ejection, Entity, Facet, Field, Index, Setting};
use crate::operation::Operation;

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
