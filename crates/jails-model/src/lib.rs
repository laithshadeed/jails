//! The semantic source of a Jails application.
//!
//! This crate is deliberately below the compiler, workspace, protocol and CLI.
//! It parses one closed model language and links every human label to an
//! explicit stable identity. It does not read files, render Java, allocate a
//! migration version or know how a plan is persisted.

mod app;
mod builtin;
mod capability;
mod component;
mod constraint;
mod dependency;
mod derived;
mod diagnostic;
mod ejection;
mod enum_constant;
mod evolution;
mod facet;
mod guard;
mod id;
mod index;
mod jdl;
pub mod layout;
mod linker;
mod model;
mod naming;
/// Exported so an integration test in `tests/` can compare this crate's
/// JDL v1 §9.7 pluralization with `jails-protocol`'s copy, which this crate
/// cannot depend on.
pub use naming::{lower_camel_case, plural_snake_case};
mod operation;
mod projection;
mod relation;
mod setting;
mod source;
mod unit;

pub use app::ProjectIntent;
pub use builtin::{BuiltinSemantics, LiteralKind};
pub use component::{
    Component, ComponentKind, ComponentParameter, ComponentReference, ComponentVariant,
};
pub use constraint::{ConstraintKind, EntityConstraint};
pub use derived::{DerivedRole, DerivedRoleKey, DerivedValue};
pub use diagnostic::{Diagnostic, Diagnostics, Severity};
pub use enum_constant::EnumConstant;
pub use evolution::{
    ColumnRenamePolicy, Evolution, EvolutionStep, FieldAddPolicy, FieldEvolutionPolicy,
    StorageRetirementPolicy, TypeChangeStrategy,
};
pub use id::{
    CapabilityId, ComponentId, ComponentVariantId, ConstraintId, DependencyId, EjectionId,
    EntityId, FieldId, IndexId, OperationId, ProjectId, ProjectionId, RelationId, SettingId,
    StableId, UnitId,
};
pub use jdl::parse as parse_jdl;
pub use jdl::v1::{
    DeclarationCst, DocumentCst, MemberCst, Span as JdlSpan, Token as JdlToken,
    TokenKind as JdlTokenKind, append_jdl_declaration, format as format_jdl_v1,
    insert_jdl_entity_member, insert_jdl_enum_constant, parse_cst as parse_jdl_cst,
    remove_jdl_declaration, remove_jdl_entity_member, rename_jdl_declaration,
    replace_jdl_entity_member, set_jdl_app_property, set_jdl_entity_attribute,
    set_jdl_projection_path,
};
pub use layout::{Head, Layer, Layout, Package};
pub use linker::JAVA_RELEASE_FLOOR;
pub use model::{
    AppModel, BuiltinType, Capability, Dependency, DependencyScope, Ejection, Entity, EntityNames,
    Facet, Field, FieldDefault, FieldNames, FieldScope, FieldSemantics, Index, IndexColumn,
    IndexDirection, LengthRange, Setting, SettingTarget, TypeRef,
};
pub use operation::{
    Assignment, BindingSource, Command, CommandSemantics, Delivery, Event, EventSemantics,
    FieldMapping, Join, Operation, OperationKind, OperationNames, OperationParameter,
    OperationRoute, Ordering, ParameterBinding, ParameterConstraints, ParameterSource,
    Precondition, Query, QuerySemantics, Resolution, SortDirection, Transition,
    TransitionSemantics, Value, VisibleField,
};
pub use projection::{Projection, ProjectionKind};
pub use relation::{ReferentialAction, Relation, RelationCardinality, RelationMapping};
pub use unit::{EndpointMethod, HttpEndpoint, RequestFormat, SourceUnit, UnitKind};
