//! The closed source shape the linker reads, and only that.
//!
//! **A wire format, deliberately separate from `AppModel`.** Everything here
//! uses plain `String` keys and carries no validated identity: the
//! authoritative model has validated IDs, resolved references and linked
//! semantics, and this has none of that, because a parser that validated as
//! it decoded could only report the first problem it met.
//!
//! Nothing outside this module may hold one of these values. The JDL v1
//! parser builds a `Document` from `.jails/model.jdl`, and `Linker` turns it
//! into an `AppModel`, running every stable-ID constructor and every reference
//! check and reporting all of them at once as `Diagnostics`.
//!
//! `.jails/model.jdl` is the one authoring boundary, so a new declaration
//! starts in the JDL v1 parser and lands here as the shape the linker needs;
//! this module is never a second way to state one.

use crate::model::{DependencyScope, Facet, SettingTarget};
use crate::{ComponentKind, EndpointMethod, RequestFormat, UnitKind};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct Document {
    pub(crate) schema: String,
    pub(crate) project: Project,
    pub(crate) capabilities: BTreeMap<String, Capability>,
    pub(crate) dependencies: BTreeMap<String, Dependency>,
    pub(crate) settings: BTreeMap<String, Setting>,
    pub(crate) ejections: BTreeMap<String, Ejection>,
    pub(crate) units: BTreeMap<String, Unit>,
    pub(crate) components: BTreeMap<String, Component>,
    pub(crate) entities: BTreeMap<String, Entity>,
    pub(crate) operations: BTreeMap<String, Operation>,
    pub(crate) projection_rules: Vec<ProjectionRule>,
}

pub(crate) struct Project {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) base_package: String,
    pub(crate) java_release: u16,
    pub(crate) dialect: String,
    pub(crate) platform: String,
    pub(crate) build: String,
}

pub(crate) struct Capability {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) name: Option<String>,
    pub(crate) package: Option<String>,
}

pub(crate) struct Dependency {
    pub(crate) id: String,
    pub(crate) group: String,
    pub(crate) artifact: String,
    pub(crate) version: Option<String>,
    pub(crate) scope: DependencyScope,
}

pub(crate) struct Setting {
    pub(crate) id: String,
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) target: SettingTarget,
}

pub(crate) struct Ejection {
    pub(crate) id: String,
    pub(crate) target: String,
}

pub(crate) struct Unit {
    pub(crate) id: String,
    pub(crate) kind: UnitKind,
    pub(crate) java_name: Option<String>,
    pub(crate) package: Option<String>,
    pub(crate) variants: Vec<String>,
    pub(crate) on: Option<String>,
    pub(crate) yields: Option<String>,
    pub(crate) method: Option<EndpointMethod>,
    pub(crate) path: Option<String>,
    pub(crate) consumes: Option<RequestFormat>,
}

pub(crate) struct Component {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) kind: ComponentKind,
    pub(crate) parameters: Vec<ComponentParameter>,
    pub(crate) on: Option<String>,
    pub(crate) yields: Option<String>,
    pub(crate) route: Option<OperationRoute>,
    pub(crate) bindings: Vec<ParameterBinding>,
    pub(crate) variants: Vec<ComponentVariant>,
    pub(crate) source: Option<String>,
}

pub struct ComponentParameter {
    pub(crate) name: String,
    pub(crate) type_name: String,
    pub(crate) required: bool,
    pub(crate) constraints: ParameterConstraints,
}

pub struct ComponentVariant {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) parameters: Vec<ComponentParameter>,
}

pub(crate) struct Entity {
    pub(crate) id: String,
    pub(crate) active: bool,
    pub(crate) java_name: Option<String>,
    pub(crate) table: Option<String>,
    /// Where this entity's Java goes, instead of the layer packages.
    ///
    /// Relative to the application's base package, exactly as a capability's
    /// is, and empty means the base itself. A slice that wants its record,
    /// repository, service and controller together says so once here rather
    /// than in each generator's call site.
    pub(crate) package: Option<String>,
    pub(crate) facets: BTreeSet<Facet>,
    pub(crate) values: Vec<String>,
    pub(crate) fields: BTreeMap<String, Field>,
    /// Declaration order, when the frontend has one to give.
    ///
    /// A Java record's component order is ABI, so JDL v1 §7.3 makes this
    /// semantic -- but a TOML table is unordered by spec and `toml` 1.1 has no
    /// `preserve_order`, so a `BTreeMap` is the only shape the compatibility
    /// input can deserialize into. JDL walks a CST and does know, so it fills
    /// this and the linker follows it; an empty list means "no order was
    /// stated", and label order is then the only answer available.
    pub(crate) field_order: Vec<String>,
    pub(crate) indexes: BTreeMap<String, Index>,
    pub(crate) constraints: Vec<EntityConstraint>,
    pub(crate) relations: BTreeMap<String, Relation>,
    pub(crate) projections: Vec<Projection>,
}

pub(crate) struct Field {
    pub(crate) id: String,
    pub(crate) java_name: Option<String>,
    pub(crate) column: Option<String>,
    pub(crate) type_name: String,
    pub(crate) required: bool,
    pub(crate) non_blank: bool,
    pub(crate) primary_key: bool,
    pub(crate) unique: bool,
    pub(crate) indexed: bool,
    pub(crate) min_length: Option<u32>,
    pub(crate) max_length: Option<u32>,
    pub(crate) semantics: FieldSemantics,
}

#[derive(Default)]
pub struct FieldSemantics {
    pub(crate) positive: bool,
    pub(crate) nonnegative: bool,
    pub(crate) scope: Option<FieldScope>,
    pub(crate) version: bool,
    pub(crate) default: Option<Value>,
    pub(crate) updated: bool,
}

pub struct FieldScope {
    pub(crate) claim: Option<String>,
}

pub(crate) struct Index {
    pub(crate) id: String,
    pub(crate) name: Option<String>,
    pub(crate) columns: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Projection {
    pub(crate) kind: String,
    pub(crate) fields: Vec<String>,
    pub(crate) path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectionRule {
    pub(crate) projections: Vec<Projection>,
    pub(crate) selector: ProjectionSelector,
    pub(crate) except: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProjectionSelector {
    All,
    Named(Vec<String>),
}

pub struct EntityConstraint {
    pub(crate) id: String,
    pub(crate) kind: ConstraintKind,
    pub(crate) name: Option<String>,
    pub(crate) fields: Vec<String>,
}

#[derive(Clone, Copy)]
pub(crate) enum ConstraintKind {
    PrimaryKey,
    Unique,
}

pub(crate) struct Relation {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) target: String,
    pub(crate) sql_name: Option<String>,
    pub(crate) mappings: Vec<RelationMapping>,
    pub(crate) on_delete: ReferentialAction,
    pub(crate) on_update: ReferentialAction,
}

pub struct RelationMapping {
    pub(crate) local: String,
    pub(crate) remote: String,
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub(crate) enum ReferentialAction {
    #[default]
    Restrict,
    Cascade,
    SetNull,
}

pub(crate) enum Operation {
    Command {
        id: String,
        java_name: Option<String>,
        on: String,
        fields: Vec<String>,
        route: Option<String>,
        semantics: CommandSemantics,
    },
    Query {
        id: String,
        java_name: Option<String>,
        on: String,
        filters: Vec<String>,
        order_by: Vec<String>,
        limit: Option<u32>,
        route: Option<String>,
        semantics: QuerySemantics,
    },
    Transition {
        id: String,
        java_name: Option<String>,
        on: String,
        fields: Vec<String>,
        sets: Vec<String>,
        yields: Option<String>,
        route: Option<String>,
        semantics: TransitionSemantics,
    },
    Event {
        id: String,
        java_name: Option<String>,
        on: Option<String>,
        fields: Vec<String>,
        semantics: EventSemantics,
    },
}

#[derive(Default)]
pub struct CommandSemantics {
    pub(crate) parameters: Vec<OperationParameter>,
    pub(crate) assignments: Vec<Assignment>,
    pub(crate) resolutions: Vec<Resolution>,
    pub(crate) conflict_key: Vec<String>,
    pub(crate) emits: Vec<String>,
    /// `direct` or `outbox`; anything else is a diagnostic at link time.
    pub(crate) delivery: Option<String>,
    pub(crate) bindings: Vec<ParameterBinding>,
    pub(crate) route: Option<OperationRoute>,
    pub(crate) internal: bool,
}

#[derive(Default)]
pub struct QuerySemantics {
    pub(crate) parameters: Vec<OperationParameter>,
    pub(crate) joins: Vec<Join>,
    pub(crate) order: Vec<Ordering>,
    pub(crate) limit: Option<u32>,
    pub(crate) bindings: Vec<ParameterBinding>,
    pub(crate) route: Option<OperationRoute>,
    pub(crate) internal: bool,
}

#[derive(Default)]
pub(crate) struct TransitionSemantics {
    pub(crate) parameters: Vec<OperationParameter>,
    pub(crate) select: Vec<String>,
    pub(crate) update: Vec<String>,
    pub(crate) assignments: Vec<Assignment>,
    pub(crate) precondition: Option<Precondition>,
    pub(crate) emits: Vec<String>,
    pub(crate) bindings: Vec<ParameterBinding>,
    pub(crate) route: Option<OperationRoute>,
    pub(crate) internal: bool,
}

#[derive(Default)]
pub struct EventSemantics {
    pub(crate) parameters: Vec<OperationParameter>,
    pub(crate) partition_by: Option<String>,
}

pub(crate) struct OperationParameter {
    pub(crate) name: String,
    pub(crate) source: ParameterSource,
    pub(crate) required: bool,
    pub(crate) optional_filter: bool,
    pub(crate) constraints: ParameterConstraints,
}

pub(crate) enum ParameterSource {
    Field { path: String },
    Typed { type_name: String },
}

#[derive(Default)]
pub struct ParameterConstraints {
    pub(crate) default: Option<Value>,
    pub(crate) non_blank: bool,
    pub(crate) min_length: Option<u32>,
    pub(crate) max_length: Option<u32>,
    pub(crate) positive: bool,
    pub(crate) nonnegative: bool,
}

#[derive(Clone)]
pub(crate) enum Value {
    String(String),
    Integer(String),
    Decimal(String),
    Boolean(bool),
    EnumConstant(String),
    Function(FunctionCall),
}

#[derive(Clone)]
pub(crate) struct FunctionCall {
    pub(crate) name: String,
    pub(crate) arguments: Vec<Value>,
}

pub struct Assignment {
    pub(crate) field: String,
    pub(crate) value: Value,
}

pub(crate) struct Resolution {
    pub(crate) target: String,
    pub(crate) remote_value: String,
    pub(crate) remote_lookup: String,
    pub(crate) parameter: String,
}

pub(crate) struct Join {
    pub(crate) entity: String,
    pub(crate) alias: String,
    pub(crate) mappings: Vec<FieldMapping>,
}

pub(crate) struct FieldMapping {
    pub(crate) local: String,
    pub(crate) remote: String,
}

pub(crate) struct Ordering {
    pub(crate) field: String,
    pub(crate) direction: SortDirection,
}

#[derive(Clone, Copy, Default)]
pub(crate) enum SortDirection {
    #[default]
    Asc,
    Desc,
}

#[derive(Clone, Copy)]
pub(crate) enum Precondition {
    Required,
    Optional,
    None,
}

pub(crate) struct OperationRoute {
    pub(crate) method: EndpointMethod,
    pub(crate) path: String,
    pub(crate) consumes: Option<RequestFormat>,
}

pub(crate) struct ParameterBinding {
    pub(crate) parameter: String,
    pub(crate) source: BindingSource,
    pub(crate) wire_name: Option<String>,
}

#[derive(Clone, Copy)]
pub enum BindingSource {
    Path,
    Query,
    Header,
    Claim,
    Form,
}

impl Operation {
    pub(crate) fn id(&self) -> &str {
        match self {
            Self::Command { id, .. }
            | Self::Query { id, .. }
            | Self::Transition { id, .. }
            | Self::Event { id, .. } => id,
        }
    }

    pub(crate) fn java_name(&self) -> Option<&str> {
        match self {
            Self::Command { java_name, .. }
            | Self::Query { java_name, .. }
            | Self::Transition { java_name, .. }
            | Self::Event { java_name, .. } => java_name.as_deref(),
        }
    }
}
