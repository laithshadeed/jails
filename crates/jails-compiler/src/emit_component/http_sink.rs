//! `component http_sink <Name> { on <command> }`: one outbox destination.
//!
//! **A sink is a plug, and the socket is the outbox's.** `deliver outbox`
//! renders `<Command>OutboxSink` as managed ABI precisely so a project can add
//! destinations without the compiler knowing what they are; this is the one
//! destination generic enough to generate -- POST the JSON payload somewhere,
//! bounded. Anything else is a class the reader writes against the same port.
//!
//! **Every bound in it is there because its absence is silent.** No redirect
//! is followed, because a redirect to a private address is how an outbound
//! call gets aimed at the host it runs on. Only 2xx acknowledges, because a
//! provider that answers 500 and is treated as delivery loses the event
//! permanently while the outbox reports success. Both timeouts are set,
//! because a stalled provider otherwise holds a relay thread until it gives
//! up, and that is never. And the stable event id rides as `Idempotency-Key`
//! on every attempt, because the outbox is at-least-once by construction --
//! without it a retry is a second charge rather than a second try.

use super::{Emitted, Package, java, package};
use crate::CompileError;
use jails_contracts::PropertyEntry;
use jails_model::{
    AppModel, BuiltinType, Component, ComponentReference, Operation, OperationKind,
    ParameterSource, TypeRef,
};
use std::collections::BTreeSet;

/// Micrometer for the delivery counters, and Jackson through the `json`
/// capability -- which is a declaration rather than a dependency, so it is
/// checked instead.
pub(super) const DEPENDENCIES: &[(&str, &str)] =
    &[("org.springframework.boot", "spring-boot-starter-actuator")];

const SINK: crate::Template = crate::template!("spring/http_outbox_sink_java.java");
const TEST: crate::Template = crate::template!("spring/http_outbox_sink_test_java.java");

pub(super) fn files(
    model: &AppModel,
    component: &Component,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<Vec<Emitted>, CompileError> {
    let command = staging_command(model, component)?;
    let event = crate::emit_operation::outbox::relayed(model, command)?;
    if let Some(declared) = component.yields.as_ref() {
        // `--yields` is redundant here -- the command already names its event
        // -- but a reader who supplies it is stating a belief, and a belief
        // the model contradicts is worth refusing rather than ignoring.
        let agrees = matches!(
            declared,
            ComponentReference::Operation(id) if id == &event.id
        );
        if !agrees {
            return Err(CompileError::new(format!(
                "http sink `{}` names an event its outbox does not relay\n       fix: `{}` stages `{}`; drop the `yields` or point it there",
                component.label, command.label, event.label
            )));
        }
    }
    if !model
        .capabilities
        .values()
        .any(|capability| capability.kind == "json")
    {
        return Err(CompileError::new(format!(
            "http sink `{}` encodes its payload with `Json`\n       fix: declare `cap json` in the model",
            component.label
        )));
    }

    let name = &component.name;
    let usecase = &command.names.java_type;
    let event_type = crate::emit_java::with_suffix(&event.names.java_type, "Event");
    let pkg = package(model, Package::Jobs);
    let events = package(model, Package::DomainEvents);
    let adapters = package(model, Package::Adapters);
    let property = property(command, component);
    let value = |key: &str, default: &str| format!("${{{property}.{key}{default}}}");

    let sink = SINK
        .resolve(templates)?
        .replace("{{pkg}}", &pkg)
        .replace(
            "import {{adapters}}.Json;\n",
            &import(&pkg, &adapters, "Json"),
        )
        .replace(
            "import {{messaging}}.{{event}}Event;\n",
            &import(&pkg, &events, &event_type),
        )
        .replace("{{property}}", &property)
        .replace("{{url_value}}", &value("url", ""))
        .replace("{{bearer_token_value}}", &value("bearer-token", ":"))
        .replace(
            "{{connect_timeout_value}}",
            &value("connect-timeout-ms", ":2000"),
        )
        .replace(
            "{{request_timeout_value}}",
            &value("request-timeout-ms", ":5000"),
        )
        .replace("{{name}}", name)
        .replace("{{usecase}}", usecase)
        .replace("{{event}}Event", &event_type);

    let (arguments, disabled, sample_imports) = sample(model, event)?;
    // **What the sample expressions name.** Every one is a builtin literal,
    // but a literal is not import-free: `UUID.fromString(..)` and
    // `Instant.parse(..)` are types, and without these a payload carrying
    // either compiles everywhere except here.
    let sample_imports = sample_imports
        .iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let test = TEST.resolve(templates)?
        .replace("{{pkg}}", &pkg)
        .replace(
            "import {{messaging}}.{{event}}Event;\n",
            &import(&pkg, &events, &event_type),
        )
.replace("{{imports}}", &sample_imports)
        .replace(
            "{{disabled_import}}",
            if disabled {
                "import org.junit.jupiter.api.Disabled;\n"
            } else {
                ""
            },
        )
        .replace(
            "{{annotation}}",
            if disabled {
                "@Disabled(\"todo: supply a sample for the payload component jails cannot fabricate\")\n"
            } else {
                ""
            },
        )
        .replace("{{args}}", &arguments)
        .replace("{{name}}", name)
        .replace("{{event}}Event", &event_type);

    Ok(vec![
        java(
            component,
            "sink",
            &pkg,
            &format!("{name}HttpOutboxSink"),
            false,
            true,
            sink,
        )?,
        java(
            component,
            "test",
            &pkg,
            &format!("{name}HttpOutboxSinkTest"),
            true,
            true,
            test,
        )?,
    ])
}

/// The outbox command this sink is a destination of.
///
/// A sink whose command publishes directly has no port to implement and no
/// staged row to deliver, so it refuses by name: the fix is a policy on the
/// command, not a change here.
fn staging_command<'a>(
    model: &'a AppModel,
    component: &Component,
) -> Result<&'a Operation, CompileError> {
    let Some(ComponentReference::Operation(id)) = component.on.as_ref() else {
        return Err(CompileError::new(format!(
            "http sink `{}` has no outbox to deliver from\n       fix: point `on` at a command that declares `deliver outbox`",
            component.label
        )));
    };
    let command = model.operations.get(id).ok_or_else(|| {
        CompileError::new(format!(
            "http sink `{}` references missing operation `{id}`\n       fix: declare the command it delivers from",
            component.label
        ))
    })?;
    if crate::emit_operation::outbox::delivery(command) != jails_model::Delivery::Outbox {
        return Err(CompileError::new(format!(
            "http sink `{}` delivers from `{}`, which publishes directly\n       fix: add `deliver outbox` to that command, or remove the sink",
            component.label, command.label
        )));
    }
    Ok(command)
}

/// One `<Event>Event(...)` argument list for the generated contract test.
///
/// Returns the arguments and whether the test has to be `@Disabled`: a
/// project-owned type is one jails cannot construct, and emitting a guess
/// would produce a test that does not compile while emitting nothing would
/// drop the coverage silently.
pub(crate) fn sample(
    model: &AppModel,
    event: &Operation,
) -> Result<(String, bool, BTreeSet<String>), CompileError> {
    let OperationKind::Event(payload) = &event.kind else {
        unreachable!("`relayed` has already checked the kind");
    };
    let mut disabled = false;
    let mut arguments = Vec::new();
    let mut imports = BTreeSet::new();
    for parameter in &payload.semantics.parameters {
        let ty = match &parameter.source {
            ParameterSource::Typed(ty) => ty.clone(),
            ParameterSource::Field(visible) => {
                let owner = crate::emit_java::entity(model, &visible.entity)?;
                let field = owner.field(&visible.field).ok_or_else(|| {
                    CompileError::new(format!(
                        "outbox event `{}` references missing field `{}`",
                        event.label, visible.field
                    ))
                })?;
                if !parameter.required {
                    arguments.push("Optional.empty()".to_string());
                    continue;
                }
                field.ty.clone()
            }
        };
        match ty {
            TypeRef::Builtin(builtin) => {
                imports.extend(builtin.semantics().java_import.map(str::to_string));
                arguments.push(builtin_sample(builtin));
            }
            TypeRef::External(_) => {
                disabled = true;
                arguments.push("null".to_string());
            }
        }
    }
    Ok((arguments.join(",\n                "), disabled, imports))
}

fn builtin_sample(builtin: BuiltinType) -> String {
    builtin.semantics().sample.to_string()
}

/// The `outbox.<command>.http.<sink>` prefix this sink's settings hang off.
///
/// Both halves are stable labels rather than Java names, so renaming either
/// type leaves the deployed configuration alone -- the same rule every other
/// projection here follows.
fn property(command: &Operation, component: &Component) -> String {
    format!(
        "outbox.{}.http.{}",
        command.label.replace('_', "-"),
        component.label.replace('_', "-")
    )
}

/// The URL, and only the URL.
///
/// The other three settings have working defaults stated in the class, so
/// writing them here would be a value the reader has to keep in step with a
/// constructor that already says it. This one has no default that could work,
/// and the class is `@ConditionalOnProperty` on it: without the key the sink
/// is simply not a bean, and the relay delivers to whatever else is there
/// rather than failing to start.
pub(super) fn properties(
    model: &AppModel,
    component: &Component,
) -> Result<Vec<PropertyEntry>, CompileError> {
    let command = staging_command(model, component)?;
    Ok(vec![PropertyEntry {
        key: format!("{}.url", property(command, component)),
        value: "https://example.invalid/events".to_string(),
    }])
}

/// One import line, or nothing when the two packages are the same.
fn import(user: &str, owner: &str, class: &str) -> String {
    if user == owner {
        String::new()
    } else {
        format!("import {owner}.{class};\n")
    }
}
