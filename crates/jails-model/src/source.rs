use crate::model::{DependencyScope, Facet, SettingTarget};
use crate::{ComponentKind, EndpointMethod, RequestFormat, UnitKind};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Document {
    pub(crate) schema: String,
    pub(crate) project: Project,
    #[serde(default)]
    pub(crate) capabilities: BTreeMap<String, Capability>,
    #[serde(default)]
    pub(crate) dependencies: BTreeMap<String, Dependency>,
    #[serde(default)]
    pub(crate) settings: BTreeMap<String, Setting>,
    #[serde(default)]
    pub(crate) ejections: BTreeMap<String, Ejection>,
    #[serde(default)]
    pub(crate) units: BTreeMap<String, Unit>,
    #[serde(default)]
    pub(crate) components: BTreeMap<String, Component>,
    #[serde(default)]
    pub(crate) entities: BTreeMap<String, Entity>,
    #[serde(default)]
    pub(crate) operations: BTreeMap<String, Operation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Project {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) base_package: String,
    pub(crate) java_release: u16,
    pub(crate) dialect: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Capability {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) name: Option<String>,
    pub(crate) package: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Dependency {
    pub(crate) id: String,
    pub(crate) group: String,
    pub(crate) artifact: String,
    pub(crate) version: Option<String>,
    #[serde(default = "compile_scope")]
    pub(crate) scope: DependencyScope,
}

const fn compile_scope() -> DependencyScope {
    DependencyScope::Compile
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Setting {
    pub(crate) id: String,
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) target: SettingTarget,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ejection {
    pub(crate) id: String,
    pub(crate) target: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Unit {
    pub(crate) id: String,
    pub(crate) kind: UnitKind,
    pub(crate) java_name: Option<String>,
    pub(crate) package: Option<String>,
    #[serde(default)]
    pub(crate) variants: Vec<String>,
    #[serde(default)]
    pub(crate) on: Option<String>,
    #[serde(default)]
    pub(crate) yields: Option<String>,
    #[serde(default)]
    pub(crate) method: Option<EndpointMethod>,
    pub(crate) path: Option<String>,
    #[serde(default)]
    pub(crate) consumes: Option<RequestFormat>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Component {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) kind: ComponentKind,
    #[serde(default)]
    pub(crate) parameters: Vec<ComponentParameter>,
    pub(crate) on: Option<String>,
    pub(crate) yields: Option<String>,
    pub(crate) route: Option<OperationRoute>,
    #[serde(default)]
    pub(crate) bindings: Vec<ParameterBinding>,
    #[serde(default)]
    pub(crate) variants: Vec<ComponentVariant>,
    pub(crate) source: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ComponentParameter {
    pub(crate) name: String,
    #[serde(rename = "type")]
    pub(crate) type_name: String,
    #[serde(default = "required")]
    pub(crate) required: bool,
    #[serde(default)]
    pub(crate) constraints: ParameterConstraints,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ComponentVariant {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) parameters: Vec<ComponentParameter>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entity {
    pub(crate) id: String,
    #[serde(default = "active")]
    pub(crate) active: bool,
    pub(crate) java_name: Option<String>,
    pub(crate) table: Option<String>,
    #[serde(default)]
    pub(crate) facets: BTreeSet<Facet>,
    #[serde(default)]
    pub(crate) values: Vec<String>,
    #[serde(default)]
    pub(crate) fields: BTreeMap<String, Field>,
    #[serde(default)]
    pub(crate) indexes: BTreeMap<String, Index>,
}

const fn active() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Field {
    pub(crate) id: String,
    pub(crate) java_name: Option<String>,
    pub(crate) column: Option<String>,
    #[serde(rename = "type")]
    pub(crate) type_name: String,
    #[serde(default = "required")]
    pub(crate) required: bool,
    #[serde(default)]
    pub(crate) non_blank: bool,
    #[serde(default)]
    pub(crate) primary_key: bool,
    #[serde(default)]
    pub(crate) unique: bool,
    #[serde(default)]
    pub(crate) indexed: bool,
    pub(crate) min_length: Option<u32>,
    pub(crate) max_length: Option<u32>,
}

const fn required() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Index {
    pub(crate) id: String,
    pub(crate) name: Option<String>,
    pub(crate) columns: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum Operation {
    Command {
        id: String,
        java_name: Option<String>,
        on: String,
        #[serde(default)]
        fields: Vec<String>,
        route: Option<String>,
        #[serde(default)]
        semantics: CommandSemantics,
    },
    Query {
        id: String,
        java_name: Option<String>,
        on: String,
        #[serde(default)]
        filters: Vec<String>,
        #[serde(default)]
        order_by: Vec<String>,
        limit: Option<u32>,
        route: Option<String>,
        #[serde(default)]
        semantics: QuerySemantics,
    },
    Transition {
        id: String,
        java_name: Option<String>,
        on: String,
        #[serde(default)]
        fields: Vec<String>,
        #[serde(default)]
        sets: Vec<String>,
        yields: Option<String>,
        route: Option<String>,
        #[serde(default)]
        semantics: TransitionSemantics,
    },
    Event {
        id: String,
        java_name: Option<String>,
        on: Option<String>,
        #[serde(default)]
        fields: Vec<String>,
        #[serde(default)]
        semantics: EventSemantics,
    },
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommandSemantics {
    #[serde(default)]
    pub(crate) parameters: Vec<OperationParameter>,
    #[serde(default)]
    pub(crate) assignments: Vec<Assignment>,
    #[serde(default)]
    pub(crate) resolutions: Vec<Resolution>,
    #[serde(default)]
    pub(crate) conflict_key: Vec<String>,
    #[serde(default)]
    pub(crate) emits: Vec<String>,
    #[serde(default)]
    pub(crate) bindings: Vec<ParameterBinding>,
    pub(crate) route: Option<OperationRoute>,
    #[serde(default)]
    pub(crate) internal: bool,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct QuerySemantics {
    #[serde(default)]
    pub(crate) parameters: Vec<OperationParameter>,
    #[serde(default)]
    pub(crate) joins: Vec<Join>,
    #[serde(default)]
    pub(crate) order: Vec<Ordering>,
    pub(crate) limit: Option<u32>,
    #[serde(default)]
    pub(crate) bindings: Vec<ParameterBinding>,
    pub(crate) route: Option<OperationRoute>,
    #[serde(default)]
    pub(crate) internal: bool,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TransitionSemantics {
    #[serde(default)]
    pub(crate) parameters: Vec<OperationParameter>,
    #[serde(default)]
    pub(crate) select: Vec<String>,
    #[serde(default)]
    pub(crate) update: Vec<String>,
    #[serde(default)]
    pub(crate) assignments: Vec<Assignment>,
    pub(crate) precondition: Option<Precondition>,
    #[serde(default)]
    pub(crate) emits: Vec<String>,
    #[serde(default)]
    pub(crate) bindings: Vec<ParameterBinding>,
    pub(crate) route: Option<OperationRoute>,
    #[serde(default)]
    pub(crate) internal: bool,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventSemantics {
    #[serde(default)]
    pub(crate) parameters: Vec<OperationParameter>,
    pub(crate) partition_by: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OperationParameter {
    pub(crate) name: String,
    pub(crate) source: ParameterSource,
    #[serde(default = "required")]
    pub(crate) required: bool,
    #[serde(default)]
    pub(crate) optional_filter: bool,
    #[serde(default)]
    pub(crate) constraints: ParameterConstraints,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum ParameterSource {
    Field {
        path: String,
    },
    Typed {
        #[serde(rename = "type")]
        type_name: String,
    },
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ParameterConstraints {
    pub(crate) default: Option<Value>,
    #[serde(default)]
    pub(crate) non_blank: bool,
    pub(crate) min_length: Option<u32>,
    pub(crate) max_length: Option<u32>,
    #[serde(default)]
    pub(crate) positive: bool,
    #[serde(default)]
    pub(crate) nonnegative: bool,
}

#[derive(Clone, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub(crate) enum Value {
    String(String),
    Integer(String),
    Decimal(String),
    Boolean(bool),
    EnumConstant(String),
    Function(FunctionCall),
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FunctionCall {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) arguments: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Assignment {
    pub(crate) field: String,
    pub(crate) value: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Resolution {
    pub(crate) target: String,
    pub(crate) remote_value: String,
    pub(crate) remote_lookup: String,
    pub(crate) parameter: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Join {
    pub(crate) entity: String,
    pub(crate) alias: String,
    pub(crate) mappings: Vec<FieldMapping>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FieldMapping {
    pub(crate) local: String,
    pub(crate) remote: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ordering {
    pub(crate) field: String,
    #[serde(default)]
    pub(crate) direction: SortDirection,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SortDirection {
    #[default]
    Asc,
    Desc,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Precondition {
    Required,
    Optional,
    None,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OperationRoute {
    pub(crate) method: EndpointMethod,
    pub(crate) path: String,
    pub(crate) consumes: Option<RequestFormat>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ParameterBinding {
    pub(crate) parameter: String,
    pub(crate) source: BindingSource,
    pub(crate) wire_name: Option<String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BindingSource {
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
