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
