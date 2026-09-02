//! `command`, `query`, `transition` and `event` — the four verbs.
//!
//! **Operations are compiler nodes, not manifest metadata.** A linked
//! operation emits typed managed Java ABI and, with the `db` capability, an
//! executable JDBC adapter; the HTTP surface is the `api` capability's
//! rendering of the same node. That is the whole reason the kinds are closed:
//! four verbs the compiler can reason about beat an open set it can only pass
//! through.
//!
//! The split between the declaration and its `*Semantics` is deliberate and
//! easy to lose. The declaration is what the author wrote — which entity, which
//! fields, which route. The semantics are what the linker *resolved* — the
//! parameter list with sources and constraints, the assignments, the emitted
//! events, the ordering and limit. An emitter reads semantics; a formatter
//! reads the declaration. Reading the wrong one is how a renderer and its test
//! came to disagree about where a request's values come from.
//!
//! `Event` is the odd verb: it declares a payload rather than an action, and
//! whether emitting it publishes in-process or stages a row in an outbox is
//! not stated here at all — that follows from the model as a whole, and the
//! compiler decides it.

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

impl Operation {
    /// The route this operation answers on, declared or derived.
    ///
    /// **Read this rather than the flat `route: Option<String>` beside it.**
    /// The flat field is the source's flat spelling; it is a rendering
    /// of this one and carries no method or request format of its own. An
    /// emitter reading the flat field sees nothing for an operation whose
    /// route the convention derived, and a missing route is how an operation
    /// says it has no HTTP surface, so the mistake is silent.
    pub fn route(&self) -> Option<&OperationRoute> {
        routes(&self.kind).1
    }

    /// The wire names this operation's author stated for its components.
    ///
    /// Empty for a kind that binds no request. `--bind` is refused without
    /// `consumes form`, so a non-empty list only ever reaches a form.
    pub fn bindings(&self) -> &[ParameterBinding] {
        match &self.kind {
            OperationKind::Command(command) => &command.semantics.bindings,
            OperationKind::Query(query) => &query.semantics.bindings,
            OperationKind::Transition(transition) => &transition.semantics.bindings,
            OperationKind::Event(_) => &[],
        }
    }
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
/// A flat `order_by: Vec<FieldId>` beside `semantics.order` cannot hold a
/// direction, so an emitter reading it renders `order by [createdAt desc,
/// id]` as `order by created_at, id` -- a query declared newest-first
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
/// A flat `sets`/`yields` pair beside `semantics.update` and
/// `semantics.emits` is a second answer that drifts: a `sets` synthesised as
/// *every* parameter subtracts neither the row selector nor the version, so
/// `select [id]` is applied as an update, and a single `yields` drops the
/// second `emit` on a transition.
///
/// The source shape still accepts both. Folding the flat one in happens at the linker boundary, where
/// every other wire-to-semantic conversion happens, so the linked model has
/// one home per fact.
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
    /// How this command's events reach their subscribers.
    ///
    /// **A typed policy rather than a compiler choice**, because the two are
    /// different promises. Publishing directly is one write and one publish
    /// that can fail independently; publishing through a stored outbox makes
    /// the event part of the same transaction as the row and relays it after.
    /// A compiler that picked one would be choosing a delivery guarantee on
    /// the reader's behalf.
    #[serde(default, skip_serializing_if = "Delivery::is_default")]
    pub delivery: Delivery,
    pub bindings: Vec<ParameterBinding>,
    pub route: Option<OperationRoute>,
    pub internal: bool,
}

/// How a command's events are delivered.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Delivery {
    /// Publish inside the command. The default, and the weaker promise: the
    /// write and the publish can fail independently.
    #[default]
    Direct,
    /// Write the event to a stored outbox in the command's own transaction and
    /// relay it afterwards, so a committed row and an unpublished event cannot
    /// disagree.
    Outbox,
}

impl Delivery {
    fn is_default(&self) -> bool {
        *self == Self::Direct
    }
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
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum Precondition {
    Required,
    Optional,
    /// The operation states that it takes no precondition at all.
    ///
    /// **Not a `--if-match` value**, which is what `value(skip)` says: the
    /// CLI's flag chooses between insisting on the caller's version and
    /// checking one when it arrives, and "neither" is spelled by not passing
    /// the flag to a kind that has no compare-and-swap. JDL still says it,
    /// because a linked operation records what it decided.
    #[cfg_attr(feature = "cli", value(skip))]
    None,
}

impl Precondition {
    /// The canonical spelling, which is the JDL `if-match` word.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Optional => "optional",
            Self::None => "none",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationRoute {
    pub method: EndpointMethod,
    pub path: String,
    pub consumes: Option<RequestFormat>,
}

impl OperationRoute {
    /// `METHOD /path`, the one spelling `valid_route` accepts and the
    /// collision table is keyed by.
    ///
    /// Two routes that differ only in how a caller spelled the method are one
    /// route to Spring, so the key has to be the canonical form rather than
    /// whatever the source said.
    pub fn canonical(&self) -> String {
        format!("{} {}", self.method.wire_name(), self.path)
    }
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

/// What the author declared, and what the operation actually answers on.
///
/// One accessor rather than a match at each site, because the pair is what
/// every reader of a route needs: `derived::records` uses it to decide whether
/// a row is pinned, and the linker uses it to know a route was derived.
pub(crate) fn routes(kind: &OperationKind) -> (Option<&String>, Option<&OperationRoute>) {
    match kind {
        OperationKind::Command(spec) => (spec.route.as_ref(), spec.semantics.route.as_ref()),
        OperationKind::Query(spec) => (spec.route.as_ref(), spec.semantics.route.as_ref()),
        OperationKind::Transition(spec) => (spec.route.as_ref(), spec.semantics.route.as_ref()),
        OperationKind::Event(_) => (None, None),
    }
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
