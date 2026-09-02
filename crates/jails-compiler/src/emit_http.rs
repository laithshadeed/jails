//! Spring HTTP adapters for routed semantic operations.

mod proof;

use crate::CompileError;
use crate::emit_companion_test::JAVA_TEST_ROOT;
use crate::emit_java::{
    JAVA_ROOT, domain_import, entity, java_type, primary_key, render, with_suffix,
};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedTree};
use jails_model::{AppModel, Operation, OperationKind, Package, StableId};
use std::collections::BTreeSet;

/// How one controller takes its request, decided once beside the parameter
/// list it is decided from.
///
/// The controller renderer and the test renderer both need this answer, and
/// working it out separately is how a path-variable query gets a test that
/// POSTs a JSON body to a GET-only route, at a URI whose placeholder is never
/// expanded.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Binding {
    /// `@RequestBody` -- the request is JSON.
    Body,
    /// `@ModelAttribute` -- the request is parameters.
    Model,
    /// `@PathVariable` for the row key, then `@RequestBody`.
    Path,
}

pub(crate) fn lower_and_emit(
    model: &AppModel,
    output: &mut RenderedTree,
    spring_boot: Option<&str>,
) -> Result<(), CompileError> {
    let Some(capability) = model
        .capabilities
        .values()
        .find(|capability| capability.kind == "api")
    else {
        return Ok(());
    };
    for operation in model.operations.values() {
        let Some(files) = lower(model, capability.id.as_str(), operation, spring_boot)? else {
            continue;
        };
        for (path, file) in files {
            output.insert(path, file).map_err(CompileError::new)?;
        }
    }
    Ok(())
}

/// What to do with an absent `If-Match`.
///
/// A required precondition never reaches this -- Spring answers 400 for a
/// missing required header before the method runs -- so the only caller left
/// is the optional form, where absent means "no precondition".
fn precondition_absent(precondition: &Option<(String, String)>) -> bool {
    precondition
        .as_ref()
        .is_some_and(|(parameter, _)| parameter.contains("required = false"))
}

/// The private method that turns an `If-Match` header into a row version.
///
/// **It accepts the weak-validator prefix and the quotes RFC 9110 requires**,
/// because that is what a client library sends back after reading the `ETag`
/// this controller issued. A header that is not a version this resource could
/// have issued is a 400 -- jails could not read the request -- rather than any
/// of the statuses the outcome maps to, which are all about a request it read.
fn expected_version_parser(java: &str, optional: bool) -> String {
    let absent = if optional {
        "        if (ifMatch == null || ifMatch.isBlank()) {\n            return null;\n        }\n"
    } else {
        ""
    };
    let parse = match java {
        "long" | "Long" => "Long.parseLong",
        "int" | "Integer" => "Integer.parseInt",
        "short" | "Short" => "Short.parseShort",
        _ => "Long.parseLong",
    };
    format!(
        "\n    /**\n     * The version the caller believes the row is at.\n     */\n    private static {java} expectedVersion(String ifMatch) {{\n{absent}        String value = ifMatch.trim();\n        if (value.startsWith(\"W/\")) {{\n            value = value.substring(2);\n        }}\n        if (value.length() >= 2 && value.startsWith(\"\\\"\") && value.endsWith(\"\\\"\")) {{\n            value = value.substring(1, value.length() - 1);\n        }}\n        try {{\n            return {parse}(value);\n        }} catch (NumberFormatException malformed) {{\n            throw new ResponseStatusException(\n                    HttpStatus.BAD_REQUEST,\n                    \"If-Match is not a version this resource issued: \" + ifMatch,\n                    malformed);\n        }}\n    }}\n"
    )
}

fn lower(
    model: &AppModel,
    capability_id: &str,
    operation: &Operation,
    spring_boot: Option<&str>,
) -> Result<Option<Vec<(ProjectPath, RenderedFile)>>, CompileError> {
    // The typed route the linker resolved, not the flat `.jails/model.toml`
    // rendering beside it: that one is `None` whenever the convention supplied
    // the path, which is every operation whose author did not pin one.
    let Some(route) = operation.route() else {
        return Ok(None);
    };
    let (method, path) = (route.method.wire_name(), route.path.as_str());
    let mut key_sample = None;
    let mut path_member = String::from("id");
    let mut precondition: Option<(String, String)> = None;
    let mut precondition_field: Option<(jails_model::Field, bool)> = None;
    let mut optional_answer = false;
    let (target, binding, port_package, port_type, return_type, parameters, mut imports) =
        match &operation.kind {
            OperationKind::Command(command) => {
                let entity = entity(model, &command.on)?;
                // **A declared `consumes form` reaches the controller.** It
                // reaches the `Input` record too -- that is where `@BindParam`
                // comes from -- and stopping there would have every form-bound
                // command ask Spring for a JSON body and answer 415 for the
                // form its own proof posts.
                let form = route.consumes == Some(jails_model::RequestFormat::Form);
                let (parameter, binder) = if form {
                    (
                        "@ModelAttribute PORT.Input input",
                        "org.springframework.web.bind.annotation.ModelAttribute",
                    )
                } else {
                    (
                        "@RequestBody PORT.Input input",
                        "org.springframework.web.bind.annotation.RequestBody",
                    )
                };
                // **A command that resolves its key answers `Optional`**, and
                // the empty case is a 404: the caller named a parent that is
                // not there, which is a request about a row rather than a
                // fault in the handler.
                let (answer, imports) = if command.semantics.resolutions.is_empty() {
                    (
                        entity.names.java_type.clone(),
                        BTreeSet::from([domain_import(model, entity), binder.to_string()]),
                    )
                } else {
                    optional_answer = true;
                    (
                        format!("ResponseEntity<{}>", entity.names.java_type),
                        BTreeSet::from([
                            domain_import(model, entity),
                            binder.to_string(),
                            "org.springframework.http.ResponseEntity".to_string(),
                        ]),
                    )
                };
                (
                    entity,
                    if form { Binding::Model } else { Binding::Body },
                    Package::ApplicationCommands,
                    with_suffix(&operation.names.java_type, "Command"),
                    answer,
                    parameter.to_string(),
                    imports,
                )
            }
            OperationKind::Query(query) => {
                let entity = entity(model, &query.on)?;
                // **`@ModelAttribute` unless the route declares a JSON body.**
                // It binds request parameters *and* URI template variables, so
                // one annotation answers `?status=open` and `/tickets/{userId}`
                // alike -- which is why a query is a GET by default and why a
                // path variable needs no second binding. A declared
                // `consumes json` is the reader saying the filters ride in a
                // body instead, and that is the only shape with one.
                let json_body = route.consumes == Some(jails_model::RequestFormat::Json);
                let (parameter, binder) = if json_body {
                    (
                        "@RequestBody PORT.Input input",
                        "org.springframework.web.bind.annotation.RequestBody",
                    )
                } else {
                    (
                        "@ModelAttribute PORT.Input input",
                        "org.springframework.web.bind.annotation.ModelAttribute",
                    )
                };
                (
                    entity,
                    if json_body {
                        Binding::Body
                    } else {
                        Binding::Model
                    },
                    Package::ApplicationQueries,
                    with_suffix(&operation.names.java_type, "Query"),
                    format!("List<{}>", entity.names.java_type),
                    parameter.to_string(),
                    BTreeSet::from([
                        domain_import(model, entity),
                        "java.util.List".to_string(),
                        binder.to_string(),
                    ]),
                )
            }
            OperationKind::Transition(transition) => {
                let entity = entity(model, &transition.on)?;
                // **The row this transition selects, which is not always the
                // primary key.** `--select userId` addresses the row by another
                // unique component, and every admin frontend then puts *that*
                // in the URL -- so the placeholder the route has to carry is
                // named after the selector rather than after `id`.
                let key = match transition.semantics.select.as_slice() {
                    [] => primary_key(entity)?,
                    [only] => entity
                        .fields
                        .iter()
                        .find(|field| &field.id == only)
                        .ok_or_else(|| {
                            CompileError::new(format!(
                                "transition operation `{}` selects field `{only}`, which entity `{}` does not declare",
                                operation.label, entity.id
                            ))
                        })?,
                    _ => {
                        return Err(CompileError::new(format!(
                            "transition operation `{}` selects more than one field, which no single path variable can carry\n       fix: select one component, or remove the `api` capability",
                            operation.label
                        )));
                    }
                };
                let member = key.names.java_member.clone();
                if !path.contains(&format!("{{{member}}}")) {
                    return Err(CompileError::new(format!(
                        "transition operation `{}` needs `{{{member}}}` in its API route\n       fix: set `route = \"PATCH /path/{{{member}}}\"` or remove the `api` capability",
                        operation.label
                    )));
                }
                path_member = member.clone();
                let mut imports = BTreeSet::from([
                    domain_import(model, entity),
                    "org.springframework.web.bind.annotation.PathVariable".to_string(),
                    "org.springframework.web.bind.annotation.RequestBody".to_string(),
                ]);
                // **The version arrives as `If-Match`, so the controller reads
                // the header rather than the body.** Spring answers 400 for a
                // missing *required* header before any code jails wrote runs,
                // which is why the optional form has to say `required = false`
                // rather than rely on a null check further in.
                precondition_field = crate::emit_java::precondition(entity, transition)
                    .map(|version| (version.field.clone(), version.required));
                precondition = crate::emit_java::precondition(entity, transition).map(|version| {
                    imports.insert("org.springframework.http.HttpHeaders".to_string());
                    imports.insert(
                        "org.springframework.web.bind.annotation.RequestHeader".to_string(),
                    );
                    let optional = if version.required {
                        String::new()
                    } else {
                        ", required = false".to_string()
                    };
                    (
                        format!(
                            "@RequestHeader(value = HttpHeaders.IF_MATCH{optional}) String ifMatch"
                        ),
                        version.java_type(&mut imports),
                    )
                });
                let key_type = java_type(key, &mut imports);
                // The placeholder the test has to expand, sampled from the
                // model's own key rather than from a literal that happens to
                // parse: a `uuid` key rejects `"1"` at the path variable,
                // before the handler runs.
                key_sample = crate::emit_companion_test::json_sample(model, &key.ty);
                // **A form-bound transition reads parameters, not a body.**
                // With `@RequestBody` on a `consumes form` route Spring
                // matches the URL and then answers 415 for the form the caller
                // sends. `@ModelAttribute` binds the query string, the form
                // body and the URI template variables alike, which is the one
                // annotation that can.
                let form = route.consumes == Some(jails_model::RequestFormat::Form);
                let binding = if form {
                    imports.remove("org.springframework.web.bind.annotation.RequestBody");
                    imports.insert(
                        "org.springframework.web.bind.annotation.ModelAttribute".to_string(),
                    );
                    "@ModelAttribute"
                } else {
                    "@RequestBody"
                };
                (
                    entity,
                    if form { Binding::Model } else { Binding::Path },
                    Package::ApplicationTransitions,
                    with_suffix(&operation.names.java_type, "Transition"),
                    entity.names.java_type.clone(),
                    format!(
                        "@PathVariable(\"{member}\") {key_type} {member}, {binding} PORT.Input input"
                    ),
                    imports,
                )
            }
            OperationKind::Event(_) => return Ok(None),
        };
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
    // The version a proof states, sampled from the model's own column so a
    // `long` key is not proved with a string that happens to parse.
    let version_sample = precondition_field.as_ref().map(|(field, required)| {
        (
            crate::emit_companion_test::json_sample(model, &field.ty)
                .unwrap_or_else(|| "1".to_string()),
            *required,
        )
    });
    let (expected_setup, expected_parser, expected_argument) = match &precondition {
        Some((parameter, java)) => {
            parameters.push_str(", ");
            parameters.push_str(parameter);
            imports.insert("org.springframework.http.HttpStatus".to_string());
            imports.insert("org.springframework.web.server.ResponseStatusException".to_string());
            (
                format!("        {java} expectedVersion = expectedVersion(ifMatch);\n"),
                expected_version_parser(java, precondition_absent(&precondition)),
                ", expectedVersion".to_string(),
            )
        }
        None => (String::new(), String::new(), String::new()),
    };
    let invocation = if matches!(operation.kind, OperationKind::Transition(_)) {
        format!("operation.execute({context_argument}{path_member}, input{expected_argument})")
    } else if optional_answer {
        format!(
            "operation.execute({context_argument}input)\n                .map(ResponseEntity::ok)\n                .orElseGet(() -> ResponseEntity.notFound().build())"
        )
    } else {
        format!("operation.execute({context_argument}input)")
    };
    let body = format!(
        "@RestController\npublic final class {type_name} {{\n\n    private final {port_type} operation;{scope_member}\n\n    public {type_name}({port_type} operation{scope_parameter}) {{\n        this.operation = operation;{scope_assignment}\n    }}\n\n    @RequestMapping(path = \"{path}\", method = RequestMethod.{method})\n    public {return_type} execute({parameters}) {{\n{context_setup}{expected_setup}        return {invocation};\n    }}\n{expected_parser}}}"
    );
    let artifact_id = format!("art_{}_http", operation.id.as_str());
    let rendered = render(&package, &imports, &body, &artifact_id);
    let package_path = package.replace('.', "/");
    let path = ProjectPath::parse(format!("{JAVA_ROOT}/{package_path}/{type_name}.java"))
        .map_err(CompileError::new)?;
    let semantic_ids =
        BTreeSet::from([capability_id.to_string(), operation.id.as_str().to_string()]);
    let controller = (
        path,
        RenderedFile {
            kind: FileKind::JavaMain,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: true,
                semantic_ids: semantic_ids.clone(),
                compiler_pass: "capability-api".to_string(),
            },
        },
    );

    // The `Input` record the controller binds to, read from the one place
    // that decides it -- see `emit_java::input_components`.
    // The declared-type imports `input_components` collects are the *record's*
    // -- the test names those types only inside a JSON literal, so they are
    // deliberately discarded here rather than emitted unused.
    let components = crate::emit_java::input_components(model, operation, &mut BTreeSet::new())?;

    // The companion test is the point of the adapter existing at all. Without
    // it the `api` capability writes a controller nothing ever dispatches a
    // request to -- which compiles, starts, and proves nothing.
    let (test_imports, test_body) = proof::controller_test(
        model,
        proof::ControllerProof {
            type_name: &type_name,
            route,
            binding,
            returns: &target.names.java_type,
            many: matches!(operation.kind, OperationKind::Query(_)),
            components: &components,
            key_json: key_sample,
            keyed: matches!(operation.kind, OperationKind::Transition(_)),
            precondition: version_sample,
            binder: (route.consumes == Some(jails_model::RequestFormat::Form)).then(|| {
                crate::emit_java::Binder {
                    model,
                    declared: operation.bindings(),
                }
            }),
            scopes: (!scope_fields.is_empty()).then(|| proof::Scopes {
                base_package: model.project.package_for(Package::Base),
                claims: scope_fields
                    .iter()
                    .map(|field| {
                        field
                            .semantics
                            .scope
                            .as_ref()
                            .expect("selected scope field")
                            .claim
                            .as_str()
                    })
                    .collect(),
            }),
            spring_boot,
        },
    )?;
    let test_artifact = format!("art_{}_http_test", operation.id.as_str());
    let test_path = ProjectPath::parse(format!(
        "{JAVA_TEST_ROOT}/{package_path}/{type_name}Test.java"
    ))
    .map_err(CompileError::new)?;
    let test = (
        test_path,
        RenderedFile {
            kind: FileKind::JavaTest,
            mode: FileMode::Regular,
            bytes: render(&package, &test_imports, &test_body, &test_artifact).into_bytes(),
            provenance: Provenance {
                artifact_id: test_artifact,
                ejection_id: None,
                ejectable: true,
                semantic_ids,
                compiler_pass: "capability-api".to_string(),
            },
        },
    );
    Ok(Some(vec![controller, test]))
}

fn java_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
