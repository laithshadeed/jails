//! What jails writes to prove a routed operation's HTTP adapter, as opposed to
//! what that adapter is.
//!
//! Split from `emit_http.rs` by secret, the way the legacy engine's
//! `spring/query/proof.rs` is split from `query.rs`, and for the same reason
//! `bugs.md` B48 records: the fact a controller test turns on -- where the
//! request's values come from -- is one the route renderer already resolved,
//! and a test renderer that resolves it a second time drifts. A query binds
//! `@ModelAttribute`, so its test sends parameters; a command binds
//! `@RequestBody`, so its test sends JSON. One `Binding`, decided next to the
//! controller's own parameter list, is what keeps those two answers the same.
//!
//! The test is standalone -- `MockMvcTester.of(new XController(stub))` -- and
//! that is a requirement rather than a preference. A `@SpringBootTest` for
//! each operation controller in a project that also declared `db` would want a
//! datasource, so six adapters that need no database between them would drag
//! a container into Surefire.

use super::Binding;
use crate::CompileError;
use crate::emit_companion_test::json_sample;
use crate::emit_java::RecordComponent;
use jails_model::{AppModel, OperationRoute};
use std::collections::BTreeSet;

/// Everything the test needs that only the controller renderer knows.
pub(super) struct ControllerProof<'a> {
    pub(super) type_name: &'a str,
    pub(super) route: &'a OperationRoute,
    pub(super) binding: Binding,
    /// The entity the port answers with, already imported by the caller.
    pub(super) returns: &'a str,
    /// Whether the port's answer is one row or many.
    pub(super) many: bool,
    /// The components the operation's `Input` record actually declares --
    /// `emit_java::input_components`, the same list the record was rendered
    /// from. Not the operation's parameter labels, which for a query are a
    /// different spelling of the same fields.
    pub(super) components: &'a [RecordComponent<'a>],
    /// The Java type of the transition's key parameter, when there is one.
    pub(super) key_json: Option<String>,
    /// The JWT claims this controller proves the request against, and the
    /// base package its `ScopeAuthorizer` lives in.
    ///
    /// Read from the same `scope_fields` the controller's own constructor was
    /// built from: a scoped controller takes a second argument, and a test
    /// that passed only the port did not compile.
    pub(super) scopes: Option<Scopes<'a>>,
    pub(super) spring_boot: Option<&'a str>,
}

/// What a scoped controller needs beyond its port.
pub(super) struct Scopes<'a> {
    pub(super) base_package: String,
    pub(super) claims: Vec<&'a str>,
}

/// The Boot major at which `MockMvcTester` is the shape to render.
///
/// `MockMvcTester` arrived in Framework 6.2, which is Boot 3.4 -- but the line
/// is drawn at the major for the same reason `emit_unit` draws it there: the
/// project states a Boot version, and a minor-level threshold turns every
/// unreadable or unusual version string into a wrong answer rather than the
/// conservative one.
const MOCKMVC_TESTER_BOOT_MAJOR: u32 = 4;

pub(super) fn controller_test(
    model: &AppModel,
    proof: ControllerProof<'_>,
) -> Result<(BTreeSet<String>, String), CompileError> {
    let ControllerProof {
        type_name,
        route,
        binding,
        returns,
        many,
        components,
        key_json,
        scopes,
        spring_boot,
    } = proof;
    let mut imports = BTreeSet::from([
        "org.junit.jupiter.api.Test".to_string(),
        "static org.assertj.core.api.Assertions.assertThat".to_string(),
    ]);

    // The stub is the port as a lambda, which every operation port admits
    // because each declares exactly one method. A transition takes the row key
    // as well, so its lambda takes two.
    let answer = sample_answer(model, returns, many, &mut imports);
    // A scoped operation's port takes the `ExecutionContext` the controller
    // built, so its lambda has one more parameter -- the same `scope_fields`
    // answer the constructor above was rendered from.
    let context = if scopes.is_some() { "context, " } else { "" };
    let answered = answer.as_deref().unwrap_or("null");
    let stub = match binding {
        Binding::Path => format!("({context}id, input) -> {answered}"),
        _ => format!("({context}input) -> {answered}"),
    };
    // `ScopeAuthorizer` is a final class over `Environment`, not an interface,
    // so the stub is a real one reading a `MockEnvironment`. Outside the `prod`
    // profile it answers from `app.security.dev.scopes.<claim>`, and its `*`
    // default is the one value `claim` refuses -- so each claim is stated.
    let authorizer = scopes.as_ref().map(|scopes| {
        imports.insert(format!("{}.ScopeAuthorizer", scopes.base_package));
        imports.insert("org.springframework.mock.env.MockEnvironment".to_string());
        let properties = scopes
            .claims
            .iter()
            .map(|claim| {
                format!("\n                    .withProperty(\"app.security.dev.scopes.{claim}\", \"sample\")")
            })
            .collect::<String>();
        format!(",\n            new ScopeAuthorizer(new MockEnvironment(){properties})")
    });
    let stub = format!("{stub}{}", authorizer.unwrap_or_default());

    let request = request_shape(model, binding, components, key_json.as_deref())?;
    // Emitted whole and disabled rather than omitted: a test that cannot be
    // built is a gap in coverage, and a gap nobody can see is the one that
    // stays. Guessing a value instead would not compile.
    let unbuildable = answer
        .is_none()
        .then(|| format!("a sample of {returns}"))
        .or_else(|| request.missing.clone());
    let (disabled_import, disabled) = match &unbuildable {
        Some(what) => {
            imports.insert("org.junit.jupiter.api.Disabled".to_string());
            (
                "",
                format!("    @Disabled(\"todo: supply {what} -- jails cannot build one\")\n"),
            )
        }
        None => ("", String::new()),
    };
    let _ = disabled_import;

    let classic = crate::emit_capability::boot_major(spring_boot)
        .is_some_and(|major| major < MOCKMVC_TESTER_BOOT_MAJOR);
    let verb = route.method.wire_name().to_ascii_lowercase();
    let body = if classic {
        imports.extend([
            "org.springframework.test.web.servlet.MockMvc".to_string(),
            "org.springframework.test.web.servlet.setup.MockMvcBuilders".to_string(),
            format!(
                "static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.{verb}"
            ),
            "static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status"
                .to_string(),
        ]);
        imports.extend(request.imports.iter().cloned());
        format!(
            "class {type_name}Test {{\n\n    private final MockMvc mvc = MockMvcBuilders.standaloneSetup(\n            new {type_name}({stub})).build();\n\n{disabled}    @Test\n    void answersOnItsDeclaredRoute() throws Exception {{\n        mvc.perform({verb}(\"{}\"{}){})\n                .andExpect(status().isOk());\n    }}\n\n    // Reader-owned tests belong below this stable boundary.\n}}",
            route.path, request.classic_uri_arguments, request.classic,
        )
    } else {
        imports.insert("org.springframework.test.web.servlet.assertj.MockMvcTester".to_string());
        imports.extend(request.imports.iter().cloned());
        format!(
            "class {type_name}Test {{\n\n    private final MockMvcTester mvc = MockMvcTester.of(\n            new {type_name}({stub}));\n\n{disabled}    @Test\n    void answersOnItsDeclaredRoute() {{\n        assertThat(mvc.{verb}()\n                .uri(\"{}\"{}){})\n                .hasStatusOk();\n    }}\n\n    // Reader-owned tests belong below this stable boundary.\n}}",
            route.path, request.uri_arguments, request.fluent,
        )
    };
    Ok((imports, body))
}

/// What the port hands back, sampled from the model.
///
/// The sample's *own* imports are only half of what the file needs. A
/// controller lives in the adapter package and the entity it answers with
/// lives in the domain one, so `new Message(...)` names two types this file
/// has not imported -- the entity, and every declared type its constructor
/// takes. `emit_companion_test`'s samples get those for free by sitting in the
/// same package as the record they build; this one does not.
fn sample_answer(
    model: &AppModel,
    returns: &str,
    many: bool,
    imports: &mut BTreeSet<String>,
) -> Option<String> {
    let row = crate::emit_companion_test::declared_sample_of(model, returns, imports)?;
    import_declared_closure(model, returns, imports, &mut BTreeSet::new());
    if many {
        imports.insert("java.util.List".to_string());
        Some(format!("List.of({row})"))
    } else {
        Some(row)
    }
}

/// Every declared type a sample of `java_type` mentions, imported.
///
/// Recursive because a record's sample constructs its components, and one of
/// those may itself be a declared record or enum. `seen` is what stops a
/// self-referencing model looping -- the same guard `declared_sample` needs,
/// for the same reason.
fn import_declared_closure(
    model: &AppModel,
    java_type: &str,
    imports: &mut BTreeSet<String>,
    seen: &mut BTreeSet<String>,
) {
    if !seen.insert(java_type.to_string()) {
        return;
    }
    let Some(entity) = model
        .entities
        .values()
        .find(|entity| entity.active && entity.names.java_type == java_type)
    else {
        return;
    };
    imports.insert(crate::emit_java::domain_import(model, entity));
    for field in &entity.fields {
        if let jails_model::TypeRef::External(external) = &field.ty {
            import_declared_closure(model, external, imports, seen);
        }
    }
}

/// The request one controller accepts, in both MockMvc shapes.
struct Request {
    fluent: String,
    classic: String,
    uri_arguments: String,
    classic_uri_arguments: String,
    imports: BTreeSet<String>,
    /// What jails could not build, when it could not.
    missing: Option<String>,
}

/// The request's own lines, carrying the newline that separates them from the
/// URI -- so a request with nothing to add leaves no blank line behind rather
/// than one the formatter then has to be told about.
fn prefixed(lines: &[String]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        format!("\n{}", lines.join("\n"))
    }
}

fn request_shape(
    model: &AppModel,
    binding: Binding,
    components: &[RecordComponent<'_>],
    key_json: Option<&str>,
) -> Result<Request, CompileError> {
    let mut imports = BTreeSet::new();
    let mut missing = None;
    match binding {
        // `@ModelAttribute` binds from the query string and the form body, so
        // the request states parameters. An optional filter jails cannot
        // sample is *omitted* rather than sent as `null`: absent is what "no
        // filter" means to a query, and the four-character string `null` is
        // what sending it would actually mean.
        Binding::Model => {
            let mut fluent = Vec::new();
            let mut classic = Vec::new();
            for component in components {
                // **An optional filter is sent with a value, not omitted and
                // not `null`.** Omitting it leaves the adapter's
                // `if (input.status().isPresent())` arm -- the half of the
                // query that builds a predicate -- unproven, and `status=null`
                // sends the four-character string, which the binder reads as a
                // filter for the literal text `null`. Sampling it as present
                // is the only one of the three that drives the code.
                let Some(value) = json_sample(model, component.ty) else {
                    if component.required {
                        missing.get_or_insert(format!("a sample for `{}`", component.name));
                    }
                    continue;
                };
                let value = value.trim_matches('"').to_string();
                let line = format!(
                    "                .param(\"{}\", \"{value}\")",
                    component.name
                );
                fluent.push(line.clone());
                classic.push(line);
            }
            Ok(Request {
                fluent: prefixed(&fluent),
                classic: prefixed(&classic),
                uri_arguments: String::new(),
                classic_uri_arguments: String::new(),
                imports,
                missing,
            })
        }
        // `@RequestBody`, with or without a path variable in front of it.
        Binding::Body | Binding::Path => {
            let mut fields = Vec::new();
            for component in components {
                // A component the record declares `Optional<T>` binds from
                // `null` and from an absent key alike; stating it is the
                // stronger request, because it proves the compact constructor
                // normalises what a caller can actually send.
                if !component.required {
                    fields.push(format!("  \"{}\": null", component.name));
                    continue;
                }
                match json_sample(model, component.ty) {
                    Some(value) => fields.push(format!("  \"{}\": {value}", component.name)),
                    None => {
                        missing.get_or_insert(format!("a sample for `{}`", component.name));
                        fields.push(format!("  \"{}\": null", component.name));
                    }
                }
            }
            let json = fields.join(",\n");
            imports.insert("org.springframework.http.MediaType".to_string());
            let (uri_arguments, classic_uri_arguments) = match (binding, key_json) {
                (Binding::Path, Some(key)) => {
                    let key = key.trim_matches('"').to_string();
                    (format!(", \"{key}\""), format!(", \"{key}\""))
                }
                (Binding::Path, None) => {
                    missing.get_or_insert("a sample for the row key".to_string());
                    (", \"1\"".to_string(), ", \"1\"".to_string())
                }
                _ => (String::new(), String::new()),
            };
            let fluent = format!(
                "\n                .contentType(MediaType.APPLICATION_JSON)\n                .content(\"\"\"\n{{\n{json}\n}}\n\"\"\")"
            );
            let classic = fluent.clone();
            Ok(Request {
                fluent,
                classic,
                uri_arguments,
                classic_uri_arguments,
                imports,
                missing,
            })
        }
    }
}
