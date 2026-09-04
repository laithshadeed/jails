//! Declarations that are not entities: enums, sealed types, values, services,
//! clients, jobs, DTOs and the rest.
//!
//! An entity is a stored thing with fields, a table and a lifecycle. A
//! `Component` is everything else a project declares, and it is one type with
//! a `kind` rather than a family of types because the *shape* they share is
//! real — a label, a stable ID, parameters, optional `on`/`yields` references,
//! optional variants, an optional route — and the differences are what the
//! emitter does with them, not what the model holds.
//!
//! That is the decision worth defending, because the alternative keeps
//! suggesting itself. Fifteen structs would let each kind state its own
//! constraints in its own type; they would also mean fifteen parse arms,
//! fifteen patch variants, fifteen linker arms and fifteen match arms in every
//! pass that walks components — and a kind added without one of them would be
//! a silent no-op rather than a compile error. `ComponentKind` being an enum
//! is what lets `registry.rs` state, once and exhaustively, that every kind
//! either emits or refuses.

use crate::operation::{OperationRoute, ParameterBinding, ParameterConstraints};
use crate::{ComponentId, ComponentVariantId, EntityId, OperationId, TypeRef};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Component {
    pub id: ComponentId,
    pub label: String,
    pub name: String,
    pub kind: ComponentKind,
    pub parameters: Vec<ComponentParameter>,
    pub on: Option<ComponentReference>,
    pub yields: Option<ComponentReference>,
    pub route: Option<OperationRoute>,
    pub bindings: Vec<ParameterBinding>,
    pub variants: Vec<ComponentVariant>,
    pub source: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ComponentParameter {
    pub name: String,
    pub ty: TypeRef,
    pub required: bool,
    pub constraints: ParameterConstraints,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ComponentVariant {
    pub id: ComponentVariantId,
    pub name: String,
    pub parameters: Vec<ComponentParameter>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ComponentReference {
    Entity(EntityId),
    Operation(OperationId),
    Component(ComponentId),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentKind {
    Class,
    Interface,
    Service,
    Controller,
    Sealed,
    Strategy,
    Handler,
    Command,
    Cli,
    Cases,
    Client,
    Fetcher,
    Job,
    HttpWorkflow,
    HttpSink,
    Idempotency,
    Auth,
    Webhook,
    DurableJob,
    Socket,
    Presence,
    Test,
    IntegrationTest,
}

impl ComponentKind {
    pub const ALL: [Self; 23] = [
        Self::Class,
        Self::Interface,
        Self::Service,
        Self::Controller,
        Self::Sealed,
        Self::Strategy,
        Self::Handler,
        Self::Command,
        Self::Cli,
        Self::Cases,
        Self::Client,
        Self::Fetcher,
        Self::Job,
        Self::HttpWorkflow,
        Self::HttpSink,
        Self::Idempotency,
        Self::Auth,
        Self::Webhook,
        Self::DurableJob,
        Self::Socket,
        Self::Presence,
        Self::Test,
        Self::IntegrationTest,
    ];

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "class" => Ok(Self::Class),
            "interface" => Ok(Self::Interface),
            "service" => Ok(Self::Service),
            "controller" => Ok(Self::Controller),
            "sealed" => Ok(Self::Sealed),
            "strategy" => Ok(Self::Strategy),
            "handler" => Ok(Self::Handler),
            "command" => Ok(Self::Command),
            "cli" => Ok(Self::Cli),
            "cases" => Ok(Self::Cases),
            "client" => Ok(Self::Client),
            "fetcher" => Ok(Self::Fetcher),
            "job" => Ok(Self::Job),
            "http-workflow" => Ok(Self::HttpWorkflow),
            "http-sink" => Ok(Self::HttpSink),
            "idempotency" => Ok(Self::Idempotency),
            "auth" => Ok(Self::Auth),
            "webhook" => Ok(Self::Webhook),
            "durable-job" => Ok(Self::DurableJob),
            "socket" => Ok(Self::Socket),
            "presence" => Ok(Self::Presence),
            "test" => Ok(Self::Test),
            "integration-test" => Ok(Self::IntegrationTest),
            _ => Err(format!("unknown component kind `{value}`")),
        }
    }

    /// The Java type this kind generates for a declared stem.
    ///
    /// **The suffix is the convention and the stem is the author's**, which is
    /// why a component named `BillingService` is refused: the kind would add
    /// `Service` to it and the file would be `BillingServiceService`. It lives
    /// on the kind rather than in the linker because `derived::records` needs
    /// the same answer -- JDL v1 §18.4 makes it one of the values `model
    /// explain` shows, and a second copy of a suffix table is a second answer.
    pub fn primary_type(self, name: &str) -> String {
        match self {
            Self::Service => format!("{name}Service"),
            Self::Controller => format!("{name}Controller"),
            Self::Handler => format!("{name}Handler"),
            Self::Command => format!("{name}Command"),
            Self::Cli => format!("{name}Cli"),
            Self::Cases => format!("{name}Cases"),
            Self::Client => format!("{name}Client"),
            Self::Fetcher => format!("{name}Fetcher"),
            Self::Job => format!("{name}Job"),
            Self::HttpWorkflow => format!("{name}Workflow"),
            Self::HttpSink => format!("{name}HttpOutboxSink"),
            Self::Idempotency => format!("{name}Guard"),
            Self::Auth => format!("{name}TokenConfig"),
            Self::Webhook => format!("{name}Verifier"),
            Self::DurableJob => format!("{name}Work"),
            Self::Socket => format!("{name}SocketHandler"),
            Self::Presence => format!("{name}Presence"),
            Self::Test => format!("{name}Test"),
            Self::IntegrationTest => format!("{name}IT"),
            Self::Class | Self::Interface | Self::Sealed | Self::Strategy => name.to_string(),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Service => "service",
            Self::Controller => "controller",
            Self::Sealed => "sealed",
            Self::Strategy => "strategy",
            Self::Handler => "handler",
            Self::Command => "command",
            Self::Cli => "cli",
            Self::Cases => "cases",
            Self::Client => "client",
            Self::Fetcher => "fetcher",
            Self::Job => "job",
            Self::HttpWorkflow => "http-workflow",
            Self::HttpSink => "http-sink",
            Self::Idempotency => "idempotency",
            Self::Auth => "auth",
            Self::Webhook => "webhook",
            Self::DurableJob => "durable-job",
            Self::Socket => "socket",
            Self::Presence => "presence",
            Self::Test => "test",
            Self::IntegrationTest => "integration-test",
        }
    }
}

pub(crate) fn references_entity(component: &Component, target: &EntityId) -> bool {
    matches!(
        component.on.as_ref(),
        Some(ComponentReference::Entity(id)) if id == target
    ) || matches!(
        component.yields.as_ref(),
        Some(ComponentReference::Entity(id)) if id == target
    )
}

pub(crate) fn references_operation(component: &Component, target: &OperationId) -> bool {
    matches!(
        component.on.as_ref(),
        Some(ComponentReference::Operation(id)) if id == target
    ) || matches!(
        component.yields.as_ref(),
        Some(ComponentReference::Operation(id)) if id == target
    )
}
