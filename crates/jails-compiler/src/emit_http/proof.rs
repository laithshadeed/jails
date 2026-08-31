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
    pub(super) spring_boot: Option<&'a str>,
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
    let stub = match (&answer, binding) {
        (Some(answer), Binding::Path) => format!("(id, input) -> {answer}"),
        (Some(answer), _) => format!("input -> {answer}"),
        (None, Binding::Path) => "(id, input) -> null".to_string(),
        (None, _) => "input -> null".to_string(),
    };

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
            "class {type_name}Test {{\n\n    private final MockMvc mvc = MockMvcBuilders.standaloneSetup(\n            new {type_name}({stub})).build();\n\n{disabled}    @Test\n    void answersOnItsDeclaredRoute() throws Exception {{\n        mvc.perform({verb}(\"{}\"{})\n{})\n                .andExpect(status().isOk());\n    }}\n\n    // Reader-owned tests belong below this stable boundary.\n}}",
            route.path, request.classic_uri_arguments, request.classic,
        )
    } else {
        imports.insert("org.springframework.test.web.servlet.assertj.MockMvcTester".to_string());
        imports.extend(request.imports.iter().cloned());
        format!(
            "class {type_name}Test {{\n\n    private final MockMvcTester mvc = MockMvcTester.of(\n            new {type_name}({stub}));\n\n{disabled}    @Test\n    void answersOnItsDeclaredRoute() {{\n        assertThat(mvc.{verb}()\n                .uri(\"{}\"{})\n{})\n                .hasStatusOk();\n    }}\n\n    // Reader-owned tests belong below this stable boundary.\n}}",
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
                // An absent optional filter is *omitted*, not sent as `null`:
                // absent is what "no filter" means on a query string, and
                // `status=null` is the four-character string.
                if !component.required {
                    continue;
                }
                let Some(value) = json_sample(model, component.ty) else {
                    missing.get_or_insert(format!("a sample for `{}`", component.name));
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
                fluent: fluent.join("\n"),
                classic: classic.join("\n"),
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
                "                .contentType(MediaType.APPLICATION_JSON)\n                .content(\"\"\"\n{{\n{json}\n}}\n\"\"\")"
            );
            let classic = format!(
                "                .contentType(MediaType.APPLICATION_JSON)\n                .content(\"\"\"\n{{\n{json}\n}}\n\"\"\")"
            );
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
