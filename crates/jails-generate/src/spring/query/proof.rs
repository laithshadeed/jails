//! What jails writes to prove a query, as opposed to what the query is.
//!
//! Split from `query.rs` by secret: that module decides the SQL, the port and
//! the route, and this one decides what a test of them has to look like --
//! which turns on a fact the rest of the module already resolved and this one
//! must not resolve again. `bugs.md` B48 was exactly that drift: the
//! controller renderer knew every filter came from the URL and the test
//! renderer did not, so a path-variable query got a test that POSTed a body to
//! a GET-only route at a URI whose `{userId}` was never expanded.

use super::*;

pub(super) fn query_controller_test_java(
    slice: &Slice,
    name: &str,
    resource: &Target,
    fields: &[crate::generate::Field],
    endpoint: Endpoint<'_>,
) -> String {
    let project = slice.project();
    let security: &str = slice.base();
    let service: &str = &slice.placed(Layer::Service);
    let web: &str = &slice.placed(Layer::Web);
    let domain: &str = &slice.owned(Layer::Domain);
    let target_fields: &[crate::generate::Field] = &resource.fields;
    let target: &str = &resource.name;
    let json = fields
        .iter()
        .map(|field| {
            json_sample(slice, field).map(|sample| format!("  \"{}\": {sample}", field.name))
        })
        .collect::<Option<Vec<_>>>();
    let target_samples = target_fields
        .iter()
        .map(|field| crate::generate::sample_value(field, project, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = json.is_none() || target_samples.is_none();
    let json = json.unwrap_or_default().join(",\n");
    let target_args = target_samples
        .unwrap_or_default()
        .join(",\n                    ");
    let port_import = crate::generate::import_of(web, service, &format!("{name}Query"));
    let target_import = crate::generate::import_of(web, domain, target);
    let imports = java_literal_imports(target_fields, domain)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let disabled_import = if disabled {
        "import org.junit.jupiter.api.Disabled;\n"
    } else {
        ""
    };
    let annotation = if disabled {
        "    @Disabled(\"todo: supply query/target samples Jails cannot fabricate\")\n"
    } else {
        ""
    };
    let (scope_import, scope_argument) = scope_test_parts(security, web, fields);
    // `bugs.md` B48: the controller renderer already worked out that every
    // filter comes from the URL, and this one did not know -- so a
    // path-variable query got a test that POSTed a JSON body to a GET-only
    // route, at a URI with `{userId}` never expanded. It failed at the URI,
    // before the verb or the body could matter:
    // `IllegalArgumentException: Not enough variable values available to
    // expand`. One endpoint, read by both, is what stops that recurring.
    if !path_variables(endpoint.route).is_empty() {
        // `MockMvcTester.uri(template, vararg)` fills the placeholders in the
        // order the template spells them, and the criteria record is
        // constructed positionally from the same field order -- which is why
        // the controller declares its parameters in *record* order too.
        let path_arguments = fields
            .iter()
            .map(|field| {
                json_sample(slice, field).map(|sample| sample.trim_matches('"').to_string())
            })
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default()
            .iter()
            .map(|sample| format!("\"{sample}\""))
            .collect::<Vec<_>>()
            .join(", ");
        return crate::template::render(
            crate::template_here!("spring/query_controller_path_test_java.java"),
            &[
                ("web", web),
                ("port_import", &*port_import),
                ("target_import", &*target_import),
                ("scope_import", &*scope_import),
                ("imports", &*imports),
                ("disabled_import", disabled_import),
                ("name", name),
                ("annotation", annotation),
                ("target", target),
                ("target_args", &*target_args),
                ("scope_argument", &*scope_argument),
                ("path_arguments", &path_arguments),
            ],
        );
    }
    // A form-bound query answers a GET and binds from the query string, so
    // its test sends parameters rather than a JSON body. One `Endpoint`, read
    // by the controller and by this -- `bugs.md` B48 is what happens when the
    // two work it out separately.
    if endpoint.consumes == jails_spec::spec::kind::WireFormat::Form {
        // A filter's sample is taken as if it were *present*, not as JSON
        // would render it. `json_sample` answers `null` for a `?` field, which
        // is right in a body and wrong here twice over: `status=null` is the
        // four-character string, and a test that sends it proves the filter is
        // never applied. An optional filter jails cannot sample is omitted --
        // absent is what "no filter" means on a query string.
        let params = fields
            .iter()
            .filter_map(|field| {
                let mut present = field.clone();
                present.optionality = crate::generate::Optionality::Required;
                let sample = json_sample(slice, &present)?;
                Some(format!(
                    "                .param(\"{}\", \"{}\")",
                    field.name,
                    sample.trim_matches('"')
                ))
            })
            .collect::<Vec<_>>()
            .join("\n");
        return crate::template::render(
            crate::template_here!("spring/query_controller_form_test_java.java"),
            &[
                ("web", web),
                ("port_import", &*port_import),
                ("target_import", &*target_import),
                ("scope_import", &*scope_import),
                ("imports", &*imports),
                ("disabled_import", disabled_import),
                ("name", name),
                ("annotation", annotation),
                ("target", target),
                ("target_args", &*target_args),
                ("scope_argument", &*scope_argument),
                ("params", &params),
            ],
        );
    }
    crate::template::render(
        // No classic form: `g query` refuses below Boot 3 because the adapter
        // it writes needs `JdbcClient`, so a Boot 2 test would have nothing to
        // exercise. `pending.md` §1.2.
        crate::template_here!("spring/query_controller_test_java.java"),
        &[
            ("web", web),
            ("port_import", &*port_import),
            ("target_import", &*target_import),
            ("scope_import", &*scope_import),
            ("imports", &*imports),
            ("disabled_import", disabled_import),
            ("name", name),
            ("annotation", annotation),
            ("json", &*json),
            ("target", target),
            ("target_args", &*target_args),
            ("scope_argument", &*scope_argument),
        ],
    )
}
