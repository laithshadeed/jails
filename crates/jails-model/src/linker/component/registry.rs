//! One row per `ComponentKind`: which fields it may, must and must not carry.
//!
//! **A table, and exhaustive over the enum.** The linker used to answer these
//! questions inline, which meant a kind added without an arm was silently
//! permissive — `on` accepted where it means nothing, a route ignored, a
//! required reference not required. Here the compiler refuses to build until
//! the new kind has a row.
//!
//! `Presence` is three-valued for the same reason: `Optional` has to be a
//! deliberate answer rather than the absence of one, or the table decays back
//! into "whatever nobody thought about is allowed".

use super::super::Linker;
use crate::source;
use crate::{ComponentKind, EndpointMethod, RequestFormat};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(super) enum Presence {
    Forbidden,
    Optional,
    Required,
}

pub(super) struct Rule {
    pub(super) on: Presence,
    pub(super) yields: Presence,
    pub(super) route: Presence,
    pub(super) source: Presence,
    pub(super) bindings: bool,
    pub(super) forbidden_suffix: Option<&'static str>,
}

pub(super) fn rule(kind: ComponentKind) -> Rule {
    use ComponentKind as K;
    use Presence::{Forbidden as F, Optional as O, Required as R};
    match kind {
        K::Class => Rule::new(F, F, F, F, false, None),
        K::Interface => Rule::new(F, F, F, F, false, None),
        K::Service => Rule::new(F, F, F, F, false, Some("Service")),
        K::Controller => Rule::new(O, O, O, F, true, Some("Controller")),
        K::Sealed => Rule::new(F, F, F, F, false, None),
        K::Strategy => Rule::new(R, O, F, F, false, None),
        K::Handler => Rule::new(F, F, O, F, false, Some("Handler")),
        K::Command => Rule::new(O, F, F, F, false, Some("Command")),
        K::Cli => Rule::new(F, F, F, F, false, Some("Cli")),
        K::Cases => Rule::new(F, F, F, R, false, Some("Cases")),
        K::Client => Rule::new(O, O, R, F, false, Some("Client")),
        K::Fetcher => Rule::new(F, F, F, F, false, Some("Fetcher")),
        K::Job => Rule::new(F, F, F, F, false, Some("Job")),
        K::HttpWorkflow => Rule::new(R, F, F, F, false, Some("Workflow")),
        K::HttpSink => Rule::new(R, R, F, F, false, Some("HttpOutboxSink")),
        K::Idempotency => Rule::new(F, F, F, F, false, Some("Guard")),
        K::Auth => Rule::new(F, F, F, F, false, Some("TokenConfig")),
        K::Webhook => Rule::new(F, F, O, F, true, Some("WebhookController")),
        K::DurableJob => Rule::new(R, R, F, F, false, Some("Work")),
        K::Socket => Rule::new(F, F, O, F, false, Some("SocketHandler")),
        K::Presence => Rule::new(F, F, F, F, false, Some("Presence")),
        K::Test => Rule::new(F, F, F, F, false, Some("Test")),
        K::IntegrationTest => Rule::new(F, F, F, F, false, Some("IT")),
    }
}

impl Rule {
    const fn new(
        on: Presence,
        yields: Presence,
        route: Presence,
        source: Presence,
        bindings: bool,
        forbidden_suffix: Option<&'static str>,
    ) -> Self {
        Self {
            on,
            yields,
            route,
            source,
            bindings,
            forbidden_suffix,
        }
    }
}

pub(super) fn validate_presence(
    present: bool,
    policy: Presence,
    member: &str,
    kind: ComponentKind,
    path: &str,
    linker: &mut Linker,
) {
    match (present, policy) {
        (false, Presence::Required) => linker.problem(
            "model-component-member-missing",
            format!("{path}.{member}"),
            format!("component {} requires `{member}`", kind.label()),
            format!("add a `{member}` member"),
        ),
        (true, Presence::Forbidden) => linker.problem(
            "model-component-member-forbidden",
            format!("{path}.{member}"),
            format!("component {} does not accept `{member}`", kind.label()),
            format!("remove the `{member}` member"),
        ),
        _ => {}
    }
}

pub(super) fn validate_route(
    kind: ComponentKind,
    route: Option<&source::OperationRoute>,
    has_body: bool,
    path: &str,
    routes: &mut BTreeMap<String, String>,
    linker: &mut Linker,
) {
    // **The two rules the `SourceUnit` linker had and this one did not.** A
    // controller's request body is the `on` entity, and `GET`/`DELETE` do not
    // carry one -- so `g controller Verify --method get --on Request` refused
    // on the pre-v1 draft and silently emitted a body-bound `@GetMapping` once
    // the same command went through a v1 component. Found by porting the
    // draft's own test, which is the reason to port a test rather than delete
    // it.
    //
    // **Above the `route` guard**, because a controller with no `route` member
    // still answers on one: `component::endpoint` defaults the method to `GET`
    // and the path to the component's own name, so the declaration a reader
    // most easily writes is exactly the one an early return would not check.
    if kind == ComponentKind::Controller {
        let method = route.map_or(EndpointMethod::Get, |route| route.method);
        if has_body && !method.takes_body() {
            linker.problem(
                "model-controller-body-method",
                format!("{path}.on"),
                "this HTTP method does not carry the declared request body",
                "use post, put, or patch, or remove `on`",
            );
        }
        if route.and_then(|route| route.consumes) == Some(RequestFormat::Form) && !has_body {
            linker.problem(
                "model-controller-form-without-body",
                format!("{path}.route"),
                "form binding needs a request type",
                "declare `on <Entity>` or consume json",
            );
        }
    }
    let Some(route) = route else {
        return;
    };
    let method_allowed = match kind {
        ComponentKind::Webhook => route.method == EndpointMethod::Post,
        ComponentKind::Socket => route.method == EndpointMethod::Get,
        ComponentKind::Controller | ComponentKind::Handler | ComponentKind::Client => true,
        _ => false,
    };
    if !method_allowed {
        linker.problem(
            "model-component-route-method",
            format!("{path}.route"),
            format!(
                "{} is not valid for component {}",
                method_label(route.method),
                kind.label()
            ),
            "use a method accepted by the component registry",
        );
    }
    let encoded = format!("{} {}", method_label(route.method), route.path);
    if kind == ComponentKind::Client {
        let mut outbound = BTreeMap::new();
        linker.route(Some(&encoded), path, &mut outbound);
    } else {
        linker.route(Some(&encoded), path, routes);
    }
}

fn method_label(method: EndpointMethod) -> &'static str {
    match method {
        EndpointMethod::Get => "GET",
        EndpointMethod::Post => "POST",
        EndpointMethod::Put => "PUT",
        EndpointMethod::Patch => "PATCH",
        EndpointMethod::Delete => "DELETE",
    }
}
