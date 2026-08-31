//! Spring HTTP adapters for routed semantic operations.

use crate::CompileError;
use crate::emit_java::{
    JAVA_ROOT, domain_import, entity, java_type, primary_key, render, with_suffix,
};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedTree};
use jails_model::{AppModel, Operation, OperationKind, Package, StableId};
use std::collections::BTreeSet;

const JAVA_TEST_ROOT: &str = ".jails/generated/test/java";

pub(crate) fn lower_and_emit(
    model: &AppModel,
    spring_boot: Option<&str>,
    output: &mut RenderedTree,
) -> Result<(), CompileError> {
    let Some(capability) = model
        .capabilities
        .values()
        .find(|capability| capability.kind == "api")
    else {
        return Ok(());
    };
    for operation in model.operations.values() {
        let Some((path, file)) = lower(model, capability.id.as_str(), operation)? else {
            continue;
        };
        output.insert(path, file).map_err(CompileError::new)?;
        if let Some((path, file)) =
            companion_test(model, capability.id.as_str(), operation, spring_boot)?
        {
            output.insert(path, file).map_err(CompileError::new)?;
        }
    }
    Ok(())
}

fn lower(
    model: &AppModel,
    capability_id: &str,
    operation: &Operation,
) -> Result<Option<(ProjectPath, RenderedFile)>, CompileError> {
    let (target, route, port_package, port_type, return_type, parameters, mut imports) =
        match &operation.kind {
            OperationKind::Command(command) => {
                let Some(route) = command.route.as_deref() else {
                    return Ok(None);
                };
                let entity = entity(model, &command.on)?;
                (
                    entity,
                    route,
                    Package::ApplicationCommands,
                    with_suffix(&operation.names.java_type, "Command"),
                    entity.names.java_type.clone(),
                    "@RequestBody PORT.Input input".to_string(),
                    BTreeSet::from([
                        domain_import(model, entity),
                        "org.springframework.web.bind.annotation.RequestBody".to_string(),
                    ]),
                )
            }
            OperationKind::Query(query) => {
                let Some(route) = query.route.as_deref() else {
                    return Ok(None);
                };
                let entity = entity(model, &query.on)?;
                (
                    entity,
                    route,
                    Package::ApplicationQueries,
                    with_suffix(&operation.names.java_type, "Query"),
                    format!("List<{}>", entity.names.java_type),
                    "@ModelAttribute PORT.Input input".to_string(),
                    BTreeSet::from([
                        domain_import(model, entity),
                        "java.util.List".to_string(),
                        "org.springframework.web.bind.annotation.ModelAttribute".to_string(),
                    ]),
                )
            }
            OperationKind::Transition(transition) => {
                let Some(route) = transition.route.as_deref() else {
                    return Ok(None);
                };
                let (_, path) = split_route(route)?;
                if !path.contains("{id}") {
                    return Err(CompileError::new(format!(
                        "transition operation `{}` needs `{{id}}` in its API route\n       fix: set `route = \"PATCH /path/{{id}}\"` or remove the `api` capability",
                        operation.label
                    )));
                }
                let entity = entity(model, &transition.on)?;
                let primary_key = primary_key(entity)?;
                let mut imports = BTreeSet::from([
                    domain_import(model, entity),
                    "org.springframework.web.bind.annotation.PathVariable".to_string(),
                    "org.springframework.web.bind.annotation.RequestBody".to_string(),
                ]);
                let key_type = java_type(primary_key, &mut imports);
                (
                    entity,
                    route,
                    Package::ApplicationTransitions,
                    with_suffix(&operation.names.java_type, "Transition"),
                    entity.names.java_type.clone(),
                    format!("@PathVariable(\"id\") {key_type} id, @RequestBody PORT.Input input"),
                    imports,
                )
            }
            OperationKind::Event(_) => return Ok(None),
        };
    let (method, path) = split_route(route)?;
    let package = model.project.package_for(Package::AdaptersHttp);
    let type_name = with_suffix(&operation.names.java_type, "Controller");
    imports.extend([
        format!("{}.{port_type}", model.project.package_for(port_package)),
        "org.springframework.web.bind.annotation.RequestMapping".to_string(),
        "org.springframework.web.bind.annotation.RequestMethod".to_string(),
        "org.springframework.web.bind.annotation.RestController".to_string(),
    ]);
    let mut parameters = parameters.replace("PORT", &port_type);
    let scope_fields = target
        .fields
        .iter()
        .filter(|field| field.semantics.scope.is_some())
        .collect::<Vec<_>>();
    let (scope_member, scope_parameter, scope_assignment, context_setup, context_argument) =
        if scope_fields.is_empty() {
            (
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            )
        } else {
            imports.extend([
                format!(
                    "{}.ScopeAuthorizer",
                    model.project.package_for(Package::Base)
                ),
                format!(
                    "{}.ExecutionContext",
                    model.project.package_for(Package::Application)
                ),
                "java.util.Map".to_string(),
                "org.springframework.security.core.Authentication".to_string(),
            ]);
            parameters.push_str(", Authentication authentication");
            let entries = scope_fields
                .iter()
                .map(|field| {
                    let claim = java_string(
                        &field
                            .semantics
                            .scope
                            .as_ref()
                            .expect("selected scope field")
                            .claim,
                    );
                    format!(
                        "                Map.entry({claim}, scopes.claim(authentication, {claim}))"
                    )
                })
                .collect::<Vec<_>>()
                .join(",\n");
            (
                "\n    private final ScopeAuthorizer scopes;".to_string(),
                ", ScopeAuthorizer scopes".to_string(),
                "\n        this.scopes = scopes;".to_string(),
                format!(
                    "        var context = new ExecutionContext(Map.ofEntries(\n{entries}));\n"
                ),
                "context, ".to_string(),
            )
        };
    let invocation = if matches!(operation.kind, OperationKind::Transition(_)) {
        format!("operation.execute({context_argument}id, input)")
    } else {
        format!("operation.execute({context_argument}input)")
    };
    let body = format!(
        "@RestController\npublic final class {type_name} {{\n\n    private final {port_type} operation;{scope_member}\n\n    public {type_name}({port_type} operation{scope_parameter}) {{\n        this.operation = operation;{scope_assignment}\n    }}\n\n    @RequestMapping(path = \"{path}\", method = RequestMethod.{method})\n    public {return_type} execute({parameters}) {{\n{context_setup}        return {invocation};\n    }}\n}}"
    );
    let artifact_id = format!("art_{}_http", operation.id.as_str());
    let rendered = render(&package, &imports, &body, &artifact_id);
    let package_path = package.replace('.', "/");
    let path = ProjectPath::parse(format!("{JAVA_ROOT}/{package_path}/{type_name}.java"))
        .map_err(CompileError::new)?;
    Ok(Some((
        path,
        RenderedFile {
            kind: FileKind::JavaMain,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: true,
                semantic_ids: BTreeSet::from([
                    capability_id.to_string(),
                    operation.id.as_str().to_string(),
                ]),
                compiler_pass: "capability-api".to_string(),
            },
        },
    )))
}

fn split_route(route: &str) -> Result<(&str, &str), CompileError> {
    route.split_once(' ').ok_or_else(|| {
        CompileError::new(format!(
            "linked operation contains invalid route `{route}`\n       fix: use `METHOD /path`"
        ))
    })
}

fn java_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The controller's companion test: a request through the real dispatcher.
///
/// **A generated controller with no generated test silently drops coverage**,
/// and the legacy engine has shipped one per operation since operations
/// existed. Moving engines emitted the controller and not the test, which is
/// the failure mode `emit_unit::controller_test` was written to warn about: a
/// canonical backend's refusals are loud and a missing test is not.
///
/// Standalone rather than `@SpringBootTest`, exactly like the legacy shape:
/// `MockMvcTester.of(new XController(stub))` builds the dispatcher around one
/// controller with a lambda standing in for the port, so the test needs no
/// context, no database and no container. That is what makes it affordable to
/// emit one per operation.
///
/// Three things disable it, and each would otherwise produce a file that does
/// not compile or a body jails invented:
///
/// - a scoped operation, whose controller also takes a `ScopeAuthorizer` and
///   an `Authentication` this test cannot mint;
/// - an entity with a component jails cannot build a sample of, so the stub's
///   return value cannot be constructed;
/// - a request parameter of a type with no JSON spelling.
///
/// Disabled, it still issues the request and asserts the status, because
/// `CLAUDE.md`'s rule for an undrivable route is to emit the test whole rather
/// than drop the coverage where nobody will see it.
fn companion_test(
    model: &AppModel,
    capability_id: &str,
    operation: &Operation,
    spring_boot: Option<&str>,
) -> Result<Option<(ProjectPath, RenderedFile)>, CompileError> {
    let (target, route, port_package, port_type, keyed, returns_list) = match &operation.kind {
        OperationKind::Command(command) => {
            let Some(route) = command.semantics.route.as_ref() else {
                return Ok(None);
            };
            (
                entity(model, &command.on)?,
                route,
                Package::ApplicationCommands,
                with_suffix(&operation.names.java_type, "Command"),
                false,
                false,
            )
        }
        OperationKind::Query(query) => {
            let Some(route) = query.semantics.route.as_ref() else {
                return Ok(None);
            };
            (
                entity(model, &query.on)?,
                route,
                Package::ApplicationQueries,
                with_suffix(&operation.names.java_type, "Query"),
                false,
                true,
            )
        }
        OperationKind::Transition(transition) => {
            let Some(route) = transition.semantics.route.as_ref() else {
                return Ok(None);
            };
            (
                entity(model, &transition.on)?,
                route,
                Package::ApplicationTransitions,
                with_suffix(&operation.names.java_type, "Transition"),
                true,
                false,
            )
        }
        OperationKind::Event(_) => return Ok(None),
    };

    let package = model.project.package_for(Package::AdaptersHttp);
    let controller = with_suffix(&operation.names.java_type, "Controller");
    let type_name = format!("{controller}Test");
    let mut imports = BTreeSet::from([
        "org.junit.jupiter.api.Test".to_string(),
        format!("{}.{port_type}", model.project.package_for(port_package)),
    ]);

    // **A scoped controller takes a second constructor argument, and
    // `@Disabled` does not excuse a field initialiser from compiling.** The
    // first version of this disabled the test and passed the port alone, so
    // three generated files failed `testCompile` with "actual and formal
    // argument lists differ in length" -- a disabled test still has to build.
    //
    // `ScopeAuthorizer` is a final class over an `Environment` rather than an
    // interface, so there is no lambda to stub it with; `MockEnvironment` is
    // spring-test's, already on the classpath of every test jails writes. That
    // leaves the reader one thing to supply -- an `Authentication` carrying
    // the claims -- which is what the `@Disabled` message names.
    let scoped = target
        .fields
        .iter()
        .any(|field| field.semantics.scope.is_some());
    let authorizer = if scoped {
        imports.insert(format!(
            "{}.ScopeAuthorizer",
            model.project.package_for(Package::Base)
        ));
        imports.insert("org.springframework.mock.env.MockEnvironment".to_string());
        ", new ScopeAuthorizer(new MockEnvironment())"
    } else {
        ""
    };
    // **The port's own parameter list, spelled as a lambda.** A scoped
    // operation's `execute` takes the `ExecutionContext` first, and a
    // transition takes the key before its input, so the stub's arity is those
    // two facts and nothing else. Getting it wrong is a lambda javac rejects
    // for "incompatible parameter types", which names neither.
    let stub = match (scoped, keyed) {
        (false, false) => "input ->",
        (false, true) => "(id, input) ->",
        (true, false) => "(context, input) ->",
        (true, true) => "(context, id, input) ->",
    };
    let constructed = crate::emit_companion_test::constructor_call(model, target, &mut imports);
    if constructed.is_some() {
        imports.insert(domain_import(model, target));
    }
    // **A query binds `@ModelAttribute`, not `@RequestBody`.** The controller
    // arm above says so, and a JSON body sent to one binds nothing: the record
    // arrives with every component null and the request is rejected before the
    // port is ever called. So a query's parameters go on the request as
    // parameters, and only a command or transition gets a body.
    let request_shape = if matches!(operation.kind, OperationKind::Query(_)) {
        request_parameters(model, operation).map(Request::Parameters)
    } else {
        request_body(model, operation).map(Request::Body)
    };

    // The stub returns whatever the port promises. A query answers a list, so
    // a single sampled row is a list of one -- enough to prove the route
    // dispatches and serialises, which is what a standalone test can know.
    let returned = match (&constructed, returns_list) {
        (Some(value), false) => value.clone(),
        (Some(value), true) => {
            imports.insert("java.util.List".to_string());
            format!("List.of({value})")
        }
        (None, _) => "null".to_string(),
    };

    let disabled_reason = if scoped {
        Some(format!(
            "todo: mint an Authentication carrying the scope claims {} proves, then delete this @Disabled",
            target.names.java_type
        ))
    } else if constructed.is_none() {
        Some(format!(
            "todo: supply a sample for a {} component -- jails cannot know how to build one",
            target.names.java_type
        ))
    } else if request_shape.is_none() {
        Some(
            "todo: supply the request arguments -- a parameter has a type jails cannot sample"
                .to_string(),
        )
    } else {
        None
    };
    if disabled_reason.is_some() {
        imports.insert("org.junit.jupiter.api.Disabled".to_string());
    }
    let disabled = disabled_reason.map_or_else(String::new, |reason| {
        format!("    @Disabled(\"{reason}\")\n")
    });

    let boot_major = crate::emit_capability::boot_major(spring_boot);
    let modern = boot_major.is_some_and(|major| major >= 4);
    let (field, request) = if modern {
        imports.insert("static org.assertj.core.api.Assertions.assertThat".to_string());
        imports.insert("org.springframework.test.web.servlet.assertj.MockMvcTester".to_string());
        (
            format!(
                "    private final MockMvcTester mvc = MockMvcTester.of(new {controller}({stub} {returned}{authorizer}));"
            ),
            fluent_request(&mut imports, route, request_shape.as_ref()),
        )
    } else {
        imports.insert(
            "static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status"
                .to_string(),
        );
        imports.insert("org.springframework.test.web.servlet.MockMvc".to_string());
        imports.insert(
            "static org.springframework.test.web.servlet.setup.MockMvcBuilders.standaloneSetup"
                .to_string(),
        );
        (
            format!(
                "    private final MockMvc mvc = standaloneSetup(new {controller}({stub} {returned}{authorizer})).build();"
            ),
            classic_request(&mut imports, route, request_shape.as_ref()),
        )
    };
    let throws = if modern { "" } else { " throws Exception" };

    let body = format!(
        "class {type_name} {{\n\n{field}\n\n    @Test\n{disabled}    void theRouteDispatches(){throws} {{\n{request}\n    }}\n\n    // Reader-owned tests belong below this stable boundary.\n}}"
    );
    let artifact_id = format!("art_{}_http_test", operation.id.as_str());
    let rendered = render(&package, &imports, &body, &artifact_id);
    let package_path = package.replace('.', "/");
    let path = ProjectPath::parse(format!("{JAVA_TEST_ROOT}/{package_path}/{type_name}.java"))
        .map_err(CompileError::new)?;
    Ok(Some((
        path,
        RenderedFile {
            kind: FileKind::JavaTest,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: true,
                semantic_ids: BTreeSet::from([
                    capability_id.to_string(),
                    operation.id.as_str().to_string(),
                ]),
                compiler_pass: "capability-api-test".to_string(),
            },
        },
    )))
}

/// The request body, as JSON, or `None` when a parameter has no JSON spelling.
///
/// Built from the linked parameters rather than the flat field list, for the
/// reason the event emitter records: the flat list can only name fields of the
/// target entity, so an operation carrying a typed component of its own would
/// get a body missing it.
fn request_body(model: &AppModel, operation: &Operation) -> Option<String> {
    let (parameters, entity_id) = match &operation.kind {
        OperationKind::Command(command) => (&command.semantics.parameters, &command.on),
        OperationKind::Query(query) => (&query.semantics.parameters, &query.on),
        OperationKind::Transition(transition) => (&transition.semantics.parameters, &transition.on),
        OperationKind::Event(_) => return None,
    };
    if parameters.is_empty() {
        return Some(String::new());
    }
    let _ = entity_id;
    let members = parameters
        .iter()
        .map(|parameter| {
            let jails_model::ParameterSource::Field(visible) = &parameter.source else {
                return None;
            };
            // **The parameter's own entity, not the operation's target.**
            // `VisibleField` carries an entity precisely because a joined
            // query filters on the far side: `UnreadForEmail --via User`
            // filters on `User.email`, which is not a field of `Message`, so
            // looking it up on the target found nothing and the test disabled
            // itself over a value jails could have sampled perfectly well.
            let field = model.entities.get(&visible.entity)?.field(&visible.field)?;
            let value = crate::emit_companion_test::json_sample(model, field)?;
            // The record component's own name, through the one projection, so
            // the body binds instead of leaving a primitive null.
            Some(format!(
                "                  \"{}\": {value}",
                crate::emit_java::parameter_member(model, parameter)
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(format!(
        "                {{\n{}\n                }}",
        members.join(",\n")
    ))
}

/// How a generated test hands its arguments to the route.
///
/// Two shapes because the controllers have two: a command or transition takes
/// `@RequestBody`, a query takes `@ModelAttribute`. Sending the wrong one binds
/// nothing and the request fails before the port is called, which is not a
/// failure the reader can act on.
enum Request {
    Body(String),
    Parameters(Vec<(String, String)>),
}

/// `MockMvcTester`'s fluent chain, which needs no `throws`.
fn fluent_request(
    imports: &mut BTreeSet<String>,
    route: &jails_model::OperationRoute,
    request: Option<&Request>,
) -> String {
    let verb = method_member(route.method);
    let uri = concrete_path(&route.path);
    let arguments = match request {
        Some(Request::Body(body)) if !body.is_empty() => {
            imports.insert("org.springframework.http.MediaType".to_string());
            format!(
                "\n                .contentType(MediaType.APPLICATION_JSON)\n                .content(\"\"\"\n{body}\"\"\")"
            )
        }
        Some(Request::Parameters(parameters)) => parameters
            .iter()
            .map(|(name, value)| format!("\n                .param(\"{name}\", \"{value}\")"))
            .collect::<String>(),
        _ => String::new(),
    };
    // Status only, and deliberately. The body is the reader's port's to
    // decide; asserting a shape jails invented would test jails' guess.
    format!(
        "        assertThat(mvc.{verb}()\n                .uri(\"{uri}\"){arguments})\n                .hasStatus2xxSuccessful();"
    )
}

/// The classic `perform(...)` shape, which exists in every Spring that has
/// MockMvc at all and is therefore the fallback rather than the other way
/// round.
fn classic_request(
    imports: &mut BTreeSet<String>,
    route: &jails_model::OperationRoute,
    request: Option<&Request>,
) -> String {
    let verb = method_member(route.method);
    imports.insert(format!(
        "static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.{verb}"
    ));
    let uri = concrete_path(&route.path);
    let arguments = match request {
        Some(Request::Body(body)) if !body.is_empty() => {
            imports.insert("org.springframework.http.MediaType".to_string());
            format!(
                "\n                .contentType(MediaType.APPLICATION_JSON)\n                .content(\"\"\"\n{body}\"\"\")"
            )
        }
        Some(Request::Parameters(parameters)) => parameters
            .iter()
            .map(|(name, value)| format!("\n                .param(\"{name}\", \"{value}\")"))
            .collect::<String>(),
        _ => String::new(),
    };
    format!(
        "        mvc.perform({verb}(\"{uri}\"){arguments})\n                .andExpect(status().is2xxSuccessful());"
    )
}

/// A query's filters as request parameters, or `None` when one has no textual
/// spelling jails can invent.
///
/// The same samples the body uses, with the JSON quoting taken off: a request
/// parameter is already text, so `"sample"` would arrive with its quotes.
fn request_parameters(model: &AppModel, operation: &Operation) -> Option<Vec<(String, String)>> {
    let OperationKind::Query(query) = &operation.kind else {
        return None;
    };
    if query.semantics.parameters.is_empty() {
        return Some(Vec::new());
    }
    query
        .semantics
        .parameters
        .iter()
        .map(|parameter| {
            let jails_model::ParameterSource::Field(visible) = &parameter.source else {
                return None;
            };
            let field = model.entities.get(&visible.entity)?.field(&visible.field)?;
            let value = crate::emit_companion_test::json_sample(model, field)?;
            let value = value
                .strip_prefix('"')
                .and_then(|rest| rest.strip_suffix('"'))
                .unwrap_or(&value)
                .to_string();
            Some((crate::emit_java::parameter_member(model, parameter), value))
        })
        .collect()
}

fn method_member(method: jails_model::EndpointMethod) -> &'static str {
    match method {
        jails_model::EndpointMethod::Get => "get",
        jails_model::EndpointMethod::Post => "post",
        jails_model::EndpointMethod::Put => "put",
        jails_model::EndpointMethod::Patch => "patch",
        jails_model::EndpointMethod::Delete => "delete",
    }
}

/// A route template with its variables filled in, because a test issues a
/// concrete request. `{id}` is the only variable the canonical controllers
/// bind, and `1` is the sample every builtin key type accepts as text.
fn concrete_path(path: &str) -> String {
    path.replace("{id}", "1")
}
