//! The semantic source of a Jails application.
//!
//! This crate is deliberately below the compiler, workspace, protocol and CLI.
//! It parses one closed model language and links every human label to an
//! explicit stable identity. It does not read files, render Java, allocate a
//! migration version or know how a plan is persisted.

mod capability;
mod component;
mod dependency;
mod diagnostic;
mod ejection;
mod enum_constant;
mod facet;
mod id;
mod index;
mod jdl;
mod linker;
mod model;
mod naming;
mod operation;
mod patch;
mod setting;
mod source;
mod syntax_edit;
mod unit;

pub use component::{
    Component, ComponentKind, ComponentParameter, ComponentReference, ComponentVariant,
};
pub use diagnostic::{Diagnostic, Diagnostics};
pub use enum_constant::EnumConstant;
pub use id::{
    CapabilityId, ComponentId, ComponentVariantId, DependencyId, EjectionId, EntityId, FieldId,
    IndexId, OperationId, ProjectId, SettingId, StableId, UnitId,
};
pub use jdl::parse as parse_jdl;
pub use jdl::v1::{
    DeclarationCst, DocumentCst, Span as JdlSpan, Token as JdlToken, TokenKind as JdlTokenKind,
    format as format_jdl_v1, parse_cst as parse_jdl_cst,
};
pub use model::{
    AppModel, BuiltinType, Capability, Dependency, DependencyScope, Ejection, Entity, EntityNames,
    Facet, Field, FieldNames, Index, IndexColumn, IndexDirection, LengthRange, ProjectIntent,
    Setting, SettingTarget, TypeRef,
};
pub use operation::{
    Assignment, BindingSource, Command, CommandSemantics, Event, EventSemantics, FieldMapping,
    Join, Operation, OperationKind, OperationNames, OperationParameter, OperationRoute, Ordering,
    ParameterBinding, ParameterConstraints, ParameterSource, Precondition, Query, QuerySemantics,
    Resolution, SortDirection, Transition, TransitionSemantics, Value, VisibleField,
};
pub use patch::{
    ColumnRenamePolicy, FieldAddPolicy, FieldEvolutionPolicy, ModelPatch, StorageRetirementPolicy,
    TypeChangeStrategy,
};
pub use syntax_edit::{
    remove_capability_declaration, remove_dependency_declaration, remove_entity_declaration,
    remove_field_declaration, remove_index_declaration, remove_operation_declaration,
    remove_setting_declaration, remove_unit_declaration, set_entity_active, set_entity_java_name,
    set_field_column, set_field_java_name, set_field_required, set_field_type,
};
pub use unit::{EndpointMethod, HttpEndpoint, RequestFormat, SourceUnit, UnitKind};

/// Parse and link a canonical TOML model.
///
/// Syntax and semantic failures share one diagnostic type so the CLI and a
/// future language server cannot disagree about what makes a model valid.
pub fn parse_toml(input: &str) -> Result<AppModel, Diagnostics> {
    let source = toml::from_str(input).map_err(Diagnostics::syntax)?;
    linker::link(source)
}
