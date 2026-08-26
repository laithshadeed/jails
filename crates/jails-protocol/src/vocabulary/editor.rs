//! Versioned values at the CLI/editor boundary.
//!
//! These are data-only projections. Editors never infer command semantics
//! from them and never execute a diagnostic message as a shell command.

use crate::application::JavaRelease;
use crate::database::EvidenceLevel;
use crate::identity::{ObjectId, ProjectPath};
use crate::request::CanonicalRequestSyntaxV1;
use crate::snapshot::InputPrecondition;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EditorBuildSystem {
    Maven,
    Gradle,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EditorCapability {
    CompletionV1,
    SymbolsV1,
    DiagnosticsV1,
    PreparedPlansV1,
    TestWatchEventsV1,
    TestdV2,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EditorSourceKind {
    MainJava,
    TestJava,
    MainResources,
    TestResources,
    Generated,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorSourceRoot {
    pub path: ProjectPath,
    pub kind: EditorSourceKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorProjectV1 {
    pub identity: ObjectId,
    pub root_digest: ObjectId,
    pub build_systems: BTreeSet<EditorBuildSystem>,
    pub java_release: JavaRelease,
    pub new_project_default_java_release: JavaRelease,
    pub source_roots: Vec<EditorSourceRoot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorHandshakeV1 {
    pub editor_protocol: u16,
    pub cli_version: String,
    pub command_result_schema: String,
    pub event_schema: String,
    pub project: EditorProjectV1,
    pub capabilities: BTreeSet<EditorCapability>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditorCursor {
    pub argument_index: u32,
    pub byte_offset: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditorReplacement {
    pub argument_index: u32,
    pub start_byte: u32,
    pub end_byte: u32,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EditorCompletionKind {
    Command,
    Option,
    Value,
    Path,
    Type,
    Test,
    Symbol,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorCompletionCandidate {
    pub value: String,
    pub display: String,
    pub kind: EditorCompletionKind,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorCompletionV1 {
    pub input: EditorCursor,
    pub replace: EditorReplacement,
    pub candidates: Vec<EditorCompletionCandidate>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EditorSymbolKind {
    Routes,
    Beans,
    Queries,
    Tests,
    Types,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditorPosition {
    pub line: u32,
    pub byte_column: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditorRange {
    pub start: EditorPosition,
    pub end: EditorPosition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorLocation {
    pub path: ProjectPath,
    pub range: EditorRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorSymbol {
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    pub location: Option<EditorLocation>,
    pub evidence: EvidenceLevel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorSymbolsV1 {
    pub root_digest: ObjectId,
    pub epoch: u64,
    pub kind: EditorSymbolKind,
    pub symbols: Vec<EditorSymbol>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Severity {
    Note,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceLabel {
    pub path: ProjectPath,
    pub range: EditorRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedFix {
    pub title: String,
    pub request: CanonicalRequestSyntaxV1,
    pub preconditions: Vec<InputPrecondition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub subject: Option<String>,
    pub primary: Option<SourceLabel>,
    pub related: Vec<SourceLabel>,
    pub evidence: Vec<EvidenceLevel>,
    pub fixes: Vec<TypedFix>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditorDiagnosticScope {
    Buffer(ProjectPath),
    Project,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorDiagnosticsV1 {
    pub root_digest: ObjectId,
    pub epoch: u64,
    pub scope: EditorDiagnosticScope,
    pub diagnostics: Vec<Diagnostic>,
}
