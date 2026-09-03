//! Values exchanged across the compiler/workspace boundary.
//!
//! These types deliberately contain bytes and observations, never filesystem
//! handles, project roots, parsers, renderers, or executor implementations.

pub mod bytes_field;
mod draft;
pub mod lock_bytes;
mod path;
mod plan;
mod snapshot;
mod templates;

pub use draft::{
    BuildDependency, BuildFeature, CompilerDiagnostic, DiagnosticSeverity, DocumentIntent,
    EffectIntent, FileKind, FileMode, PlanDraft, PropertyEntry, Provenance, ReaderFacetKind,
    RenderedFile, RenderedMigration, RenderedReaderFacet, RenderedTree, SemanticPlan, SourceRoot,
};
/// The build language, from the crate that owns every closed vocabulary.
pub use jails_model::BuildSystem;
pub use jails_model::{Head, Layer, Layout, Package};
pub use path::ProjectPath;
pub use plan::{
    FileImageRef, ModelFileUpdate, Plan, PlanBundle, PlanInput, PlannedOperation, TreeEntry,
    TreeManifest,
};
pub use snapshot::{
    CapturedFile, ContentDigest, DirectoryPrecondition, ExternalType, ExternalTypeIndex,
    FilePrecondition, MigrationHistory, MigrationRecord, OwnedPatchState, ProjectFacts, Reactor,
    ReactorModule, SnapshotPreconditions, VersionedModel, WorkspaceSnapshot,
};
pub use templates::{TemplateOverride, TemplateOverrides};
