use crate::model::{LengthRange, TypeRef};
use crate::{EndpointMethod, EntityId, FieldId, OperationId, RequestFormat};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Operation {
    pub id: OperationId,
    pub label: String,
    pub names: OperationNames,
    pub kind: OperationKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationNames {
    pub java_type: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum OperationKind {
    Command(Command),
    Query(Query),
    Transition(Transition),
    Event(Event),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Command {
    pub on: EntityId,
    pub fields: Vec<FieldId>,
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "CommandSemantics::is_empty")]
    pub semantics: CommandSemantics,
}

/// A linked query. Its ordering and row ceiling live in [`QuerySemantics`]
/// and nowhere else.
///
/// This carried `order_by: Vec<FieldId>` beside `semantics.order:
/// Vec<Ordering>`. A `FieldId` cannot hold a direction, the emitters read the
/// flat list, and so `order by [createdAt desc, id]` was parsed, linked, and
/// rendered as `order by created_at, id` -- a query declared newest-first
/// returning oldest-first, with nothing to say so.
///
/// `filters` stays: it is the entity fields a predicate is built from, which
/// is a different projection from `semantics.parameters`, since a parameter
/// may name a join alias that has no column on this table.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Query {
    pub on: EntityId,
    pub filters: Vec<FieldId>,
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "QuerySemantics::is_empty")]
    pub semantics: QuerySemantics,
}

/// A linked transition. Its changed fields and emitted events live in
/// [`TransitionSemantics`] and nowhere else.
///
/// This carried `sets: Vec<FieldId>` and `yields: Option<OperationId>` beside
/// `semantics.update` and `semantics.emits`, and the two disagreed. The flat
/// pair was a compatibility projection the JDL v1 frontend synthesised --
/// `sets` was *every* parameter whenever `update` was omitted, without
/// subtracting the row selector or the version -- while the rich pair was
/// linked correctly and read by nobody. Emitters read the flat pair, so
/// `select [id]` was applied as an update and `jdl-sol.md` §4 could not link;
/// `yields` held one event, so the second `emit` on a transition vanished.
///
/// The source shape still accepts both because `.jails/model.toml` spells
/// only the flat one. Folding that in belongs at the linker boundary, which
/// is where every other wire-to-semantic conversion happens, so the linked
/// model has one home per fact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Transition {
    pub on: EntityId,
    pub fields: Vec<FieldId>,
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "TransitionSemantics::is_empty")]
    pub semantics: TransitionSemantics,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Event {
    pub on: Option<EntityId>,
    pub fields: Vec<FieldId>,
    #[serde(default, skip_serializing_if = "EventSemantics::is_empty")]
    pub semantics: EventSemantics,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandSemantics {
    pub parameters: Vec<OperationParameter>,
    pub assignments: Vec<Assignment>,
    pub resolutions: Vec<Resolution>,
    pub conflict_key: Vec<FieldId>,
    pub emits: Vec<OperationId>,
    pub bindings: Vec<ParameterBinding>,
    pub route: Option<OperationRoute>,
    pub internal: bool,
}

impl CommandSemantics {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuerySemantics {
    pub parameters: Vec<OperationParameter>,
    pub joins: Vec<Join>,
    pub order: Vec<Ordering>,
    pub limit: Option<u32>,
    pub bindings: Vec<ParameterBinding>,
    pub route: Option<OperationRoute>,
    pub internal: bool,
}

impl QuerySemantics {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransitionSemantics {
    pub parameters: Vec<OperationParameter>,
    pub select: Vec<FieldId>,
    pub update: Vec<FieldId>,
    pub assignments: Vec<Assignment>,
    pub precondition: Option<Precondition>,
    pub emits: Vec<OperationId>,
    pub bindings: Vec<ParameterBinding>,
    pub route: Option<OperationRoute>,
    pub internal: bool,
}

impl TransitionSemantics {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventSemantics {
    pub parameters: Vec<OperationParameter>,
    pub partition_by: Option<String>,
}

impl EventSemantics {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationParameter {
    pub name: String,
    pub source: ParameterSource,
    pub required: bool,
    pub optional_filter: bool,
    pub constraints: ParameterConstraints,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ParameterSource {
    Field(VisibleField),
    Typed(TypeRef),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VisibleField {
    pub entity: EntityId,
    pub field: FieldId,
    pub qualifier: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ParameterConstraints {
    pub default: Option<Value>,
    pub non_blank: bool,
    pub length: Option<LengthRange>,
    pub positive: bool,
    pub nonnegative: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Value {
    String(String),
    Integer(String),
    Decimal(String),
    Boolean(bool),
    EnumConstant(String),
    Function { name: String, arguments: Vec<Value> },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Assignment {
    pub field: FieldId,
    pub value: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Resolution {
    pub target: FieldId,
    pub remote_entity: EntityId,
    pub remote_value: FieldId,
    pub remote_lookup: FieldId,
    pub parameter: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Join {
    pub entity: EntityId,
    pub alias: String,
    pub mappings: Vec<FieldMapping>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldMapping {
    pub local: FieldId,
    pub remote: FieldId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Ordering {
    pub field: VisibleField,
    pub direction: SortDirection,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Precondition {
    Required,
    Optional,
    None,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationRoute {
    pub method: EndpointMethod,
    pub path: String,
    pub consumes: Option<RequestFormat>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ParameterBinding {
    pub parameter: String,
    pub source: BindingSource,
    pub wire_name: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingSource {
    Path,
    Query,
    Header,
    Claim,
    Form,
}

pub(crate) fn entity(kind: &OperationKind) -> Option<&EntityId> {
    match kind {
        OperationKind::Command(command) => Some(&command.on),
        OperationKind::Query(query) => Some(&query.on),
        OperationKind::Transition(transition) => Some(&transition.on),
        OperationKind::Event(event) => event.on.as_ref(),
    }
}

pub(crate) fn references_entity(kind: &OperationKind, target: &EntityId) -> bool {
    entity(kind) == Some(target)
        || match kind {
            OperationKind::Command(command) => command.semantics.parameters.iter().any(|parameter| {
                matches!(&parameter.source, ParameterSource::Field(field) if &field.entity == target)
            }) || command.semantics.resolutions.iter().any(|resolution| {
                &resolution.remote_entity == target
            }),
            OperationKind::Query(query) => query.semantics.parameters.iter().any(|parameter| {
                matches!(&parameter.source, ParameterSource::Field(field) if &field.entity == target)
            }) || query
                .semantics
                .joins
                .iter()
                .any(|join| &join.entity == target)
                || query
                    .semantics
                    .order
                    .iter()
                    .any(|ordering| &ordering.field.entity == target),
            OperationKind::Transition(transition) => transition
                .semantics
                .parameters
                .iter()
                .any(|parameter| {
                    matches!(&parameter.source, ParameterSource::Field(field) if &field.entity == target)
                }),
            OperationKind::Event(event) => event.semantics.parameters.iter().any(|parameter| {
                matches!(&parameter.source, ParameterSource::Field(field) if &field.entity == target)
            }),
        }
}

pub(crate) fn fields(kind: &OperationKind) -> Vec<&FieldId> {
    match kind {
        OperationKind::Command(command) => command
            .fields
            .iter()
            .chain(parameter_fields(&command.semantics.parameters))
            .chain(
                command
                    .semantics
                    .assignments
                    .iter()
                    .map(|assignment| &assignment.field),
            )
            .chain(command.semantics.resolutions.iter().flat_map(|resolution| {
                [
                    &resolution.target,
                    &resolution.remote_value,
                    &resolution.remote_lookup,
                ]
            }))
            .chain(command.semantics.conflict_key.iter())
            .collect(),
        OperationKind::Query(query) => query
            .filters
            .iter()
            .chain(parameter_fields(&query.semantics.parameters))
            .chain(query.semantics.joins.iter().flat_map(|join| {
                join.mappings
                    .iter()
                    .flat_map(|mapping| [&mapping.local, &mapping.remote])
            }))
            .chain(
                query
                    .semantics
                    .order
                    .iter()
                    .map(|ordering| &ordering.field.field),
            )
            .collect(),
        OperationKind::Transition(transition) => transition
            .fields
            .iter()
            .chain(parameter_fields(&transition.semantics.parameters))
            .chain(transition.semantics.select.iter())
            .chain(transition.semantics.update.iter())
            .chain(
                transition
                    .semantics
                    .assignments
                    .iter()
                    .map(|assignment| &assignment.field),
            )
            .collect(),
        OperationKind::Event(event) => event
            .fields
            .iter()
            .chain(parameter_fields(&event.semantics.parameters))
            .collect(),
    }
}

fn parameter_fields(parameters: &[OperationParameter]) -> impl Iterator<Item = &FieldId> {
    parameters
        .iter()
        .filter_map(|parameter| match &parameter.source {
            ParameterSource::Field(field) => Some(&field.field),
            ParameterSource::Typed(_) => None,
        })
}

pub(crate) fn emits(kind: &OperationKind) -> Vec<&OperationId> {
    match kind {
        OperationKind::Command(command) => command.semantics.emits.iter().collect(),
        OperationKind::Query(_) | OperationKind::Event(_) => Vec::new(),
        OperationKind::Transition(transition) => transition.semantics.emits.iter().collect(),
    }
}
