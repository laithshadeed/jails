//! Read-only consistency result for one durable resource identity.

use crate::entity::EntityId;
use crate::identity::SqlName;
use crate::request::CanonicalRequestSyntaxV1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceConsistency {
    Consistent,
    SourceDiverged,
    MigrationEditedAfterSeal,
    MigrationMissingAfterSeal,
    RuntimeSchemaBehind,
    RetiredStoragePresent,
    DropPending,
    DropObservedApplied,
    RenamePending,
    Ambiguous,
}

impl ResourceConsistency {
    pub fn label(self) -> &'static str {
        match self {
            Self::Consistent => "consistent",
            Self::SourceDiverged => "source-diverged",
            Self::MigrationEditedAfterSeal => "migration-edited-after-seal",
            Self::MigrationMissingAfterSeal => "migration-missing-after-seal",
            Self::RuntimeSchemaBehind => "runtime-schema-behind",
            Self::RetiredStoragePresent => "retired-storage-present",
            Self::DropPending => "drop-pending",
            Self::DropObservedApplied => "drop-observed-applied",
            Self::RenamePending => "rename-pending",
            Self::Ambiguous => "ambiguous",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorityStatus {
    Present,
    Absent,
    Diverged,
    Unknown,
}

impl AuthorityStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::Absent => "absent",
            Self::Diverged => "diverged",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceFindingV1 {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceStatusV1 {
    pub entity: Option<EntityId>,
    pub state: ResourceConsistency,
    pub declaration: AuthorityStatus,
    pub generated: AuthorityStatus,
    pub migration_history: AuthorityStatus,
    pub live: Option<AuthorityStatus>,
    pub table: Option<SqlName>,
    pub findings: Vec<ResourceFindingV1>,
    pub next_requests: Vec<CanonicalRequestSyntaxV1>,
}
