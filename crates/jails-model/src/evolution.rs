//! `Evolution` -- what a mutation means beyond what the source says.
//!
//! The source is desired state: it says what the model *is*. It cannot say
//! how to get there from the model the project has accepted, because that is
//! a one-shot instruction and not a fact about the application -- rename the
//! column or cut over to a new one, backfill the rows or leave the column
//! nullable, keep the table or drop it. Writing any of those into the file
//! would make the next reader carry out a data move somebody meant once.
//!
//! So a mutation is two things: an edit to the source, and an [`Evolution`]
//! passed to the compiler beside the model the edited source links to. The
//! plan records the evolution as its input, so two mutations that edit the
//! source identically but mean different things -- a rename that preserves
//! the column and one that cuts over -- have different digests.
//!
//! **The list is closed and every step names a stable ID**, so the compiler
//! can match a step against the accepted model by identity rather than by
//! label, and a step naming nothing the accepted model has is a refusal
//! rather than a no-op.

use crate::id::{EntityId, FieldId, IndexId, RelationId};

/// The one-shot instructions a mutation carries beside its edited source.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct Evolution {
    pub steps: Vec<EvolutionStep>,
}

impl Evolution {
    /// No instruction: the accepted model reaches the next one by the
    /// compiler's own rules alone, which is every additive change.
    pub fn none() -> Self {
        Self::default()
    }

    pub fn one(step: EvolutionStep) -> Self {
        Self { steps: vec![step] }
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvolutionStep {
    /// A column added to an accepted table, and what to do about its rows.
    AddField {
        field: FieldId,
        policy: FieldAddPolicy,
    },
    /// An accepted column renamed, retyped or made required or nullable.
    ReplaceField {
        field: FieldId,
        policy: FieldEvolutionPolicy,
    },
    /// An accepted column dropped, confirmed by its SQL name.
    RemoveField {
        field: FieldId,
        confirmed_column: String,
    },
    /// An accepted index dropped, confirmed by its SQL name.
    RemoveIndex {
        index: IndexId,
        confirmed_name: String,
    },
    /// One accepted foreign key, retired by naming it.
    ///
    /// Dropping a constraint is a forward migration somebody has to mean,
    /// and inferring one from a deleted declaration is how a production
    /// invariant disappears in a routine edit -- so the compiler refuses a
    /// relation that merely stopped being declared, and this step is how
    /// the reader says they meant it. `confirmed_name` is the SQL constraint
    /// name as accepted, so a rename between accepting and retiring refuses
    /// rather than emitting `drop constraint` against a name the database
    /// does not have.
    RemoveRelation {
        relation: RelationId,
        confirmed_name: String,
    },
    /// An accepted, stored entity leaving the model, and what its table does.
    RetireEntity {
        entity: EntityId,
        policy: StorageRetirementPolicy,
    },
    /// A preserved table taken back into use by an entity, confirmed by name.
    ReviveEntity {
        entity: EntityId,
        confirmed_table: String,
    },
    /// The SQL name a single-cutover rename moves an entity's table to.
    ///
    /// A rename with no policy is refused, because the accepted table would
    /// simply be left behind under its old name with nothing saying so. This
    /// is the reader stating the move, and the derived migration is one
    /// `alter table ... rename to`, which keeps the rows, the indexes and the
    /// constraints -- the whole reason a cutover is one statement.
    RenameTable { entity: EntityId, table: String },
}

/// Explicit data policy for adding a column to an already accepted table.
///
/// Nullable columns need no data rewrite. Required columns carry the typed
/// literal used to backfill existing rows before `not null` is enforced.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FieldAddPolicy {
    Nullable,
    BackfillLiteral(String),
    ReaderOwnedSql(Vec<u8>),
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FieldEvolutionPolicy {
    Rename { column: ColumnRenamePolicy },
    ChangeType { strategy: TypeChangeStrategy },
    SetNullability { backfill_sql: Option<Vec<u8>> },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColumnRenamePolicy {
    Preserve,
    SingleCutover,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TypeChangeStrategy {
    Safe,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StorageRetirementPolicy {
    Preserve,
    Drop { confirmed_table: String },
}
