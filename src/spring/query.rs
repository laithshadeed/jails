//! `g query`: a read model over a resource that already exists.
//!
//! The sibling of `usecase` (`workflow.rs`, which documents the shape all three
//! share) and `transition`. What makes this one its own kind is the
//! projection: a query names a subset of the target's components, and that
//! subset drives the record, the select list and the row mapper together --
//! which is the same reason `sql.rs` owns one column list rather than five.

use super::workflow::{json_sample, scope_test_parts};
use super::*;

/// The two column lists one query reads through: what it selects, and what it
/// filters on. Both are derived from the same field spec in one place, which
/// is what stops a select and a where clause naming different columns.
struct Projection {
    target_columns: Vec<crate::sql::Column>,
    filter_columns: Vec<crate::sql::Column>,
}

use crate::model::{Artifact, Layer, Slice};

pub(crate) fn query_files(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
) -> jails_support::Result<Vec<Artifact>> {
    let root: &Path = slice.project().root();
    let service: &str = &slice.placed(Layer::Service);
    let web: &str = &slice.placed(Layer::Web);
    let domain: &str = &slice.owned(Layer::Domain);
    let adapters: &str = &slice.owned(Layer::Adapters);
    require_scope_authorizer(slice, "query", name, fields)?;
    if fields.is_empty() {
        return Err(format!(
            "query {name} needs at least one typed filter; use the scaffold's list endpoint for an unfiltered read"
        ));
    }
    if let Some(field) = fields.iter().find(|field| {
        field.optionality == crate::generate::Optionality::Nullable || field.collection
    }) {
        return Err(format!(
            "query {name} filter `{}` is optional or a collection. This first query contract only accepts required scalar equality filters so null/list semantics are never guessed.",
            field.name
        ));
    }
    let target_fields = Target::read(slice, "query", name, target)?.fields;
    for field in fields {
        let Some(target_field) = target_fields
            .iter()
            .find(|candidate| candidate.name == field.name)
        else {
            return Err(format!(
                "query {name} filters `{}`, but {target} has no component with that name",
                field.name
            ));
        };
        if usecase_normalized_type(&field.java_type)
            != usecase_normalized_type(&target_field.java_type)
        {
            return Err(format!(
                "query {name} declares `{}` as {}, but {target} stores it as {}",
                field.name, field.java_type, target_field.java_type
            ));
        }
    }
    let target_columns = crate::sql::columns(&target_fields, slice.project(), domain, "row");
    let filter_columns = crate::sql::columns(fields, slice.project(), domain, "query");
    let unmapped = target_columns
        .iter()
        .chain(filter_columns.iter())
        .filter(|column| !column.mapped())
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>();
    if !unmapped.is_empty() {
        return Err(format!(
            "query {name} cannot map database column(s): {}. Model collections/owned values separately or add an explicit mapping before generating the query.",
            unmapped.join(", ")
        ));
    }
    let main_service = crate::generate::main_dir(root, service);
    let main_adapters = crate::generate::main_dir(root, adapters);
    let test_adapters = crate::generate::test_dir(root, adapters);
    let main_web = crate::generate::main_dir(root, web);
    let test_web = crate::generate::test_dir(root, web);
    let resource = Target {
        name: target.to_string(),
        fields: target_fields,
    };
    let projection = Projection {
        target_columns,
        filter_columns,
    };
    Ok(vec![
        Artifact {
            kind: "query input",
            path: main_service.join(format!("{name}Query.java")),
            contents: query_record_java(slice, name, fields),
        },
        Artifact {
            kind: "query port",
            path: main_service.join(format!("{name}QueryPort.java")),
            contents: query_port_java(slice, name, target),
        },
        Artifact {
            kind: "JDBC query adapter",
            path: main_adapters.join(format!("Jdbc{name}Query.java")),
            contents: jdbc_query_java(slice, name, target, &projection),
        },
        Artifact {
            kind: "JDBC query integration test",
            path: test_adapters.join(format!("Jdbc{name}QueryIT.java")),
            contents: jdbc_query_it_java(slice, name, &resource, fields),
        },
        Artifact {
            kind: "query controller",
            path: main_web.join(format!("{name}QueryController.java")),
            contents: query_controller_java(slice, name, target, fields),
        },
        Artifact {
            kind: "query controller test",
            path: test_web.join(format!("{name}QueryControllerTest.java")),
            contents: query_controller_test_java(slice, name, &resource, fields),
        },
    ])
}

fn query_record_java(slice: &Slice, name: &str, fields: &[crate::generate::Field]) -> String {
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let class = format!("{name}Query");
    let mut source = crate::generate::record_java(pkg, &class, fields);
    let mut imports = fields
        .iter()
        .filter(|field| field.owned && domain != pkg)
        .map(|field| format!("import {domain}.{};", field.java_type))
        .collect::<Vec<_>>();
    imports.sort();
    imports.dedup();
    if !imports.is_empty() {
        let package = format!("package {pkg};\n");
        source = source.replacen(&package, &format!("{package}\n{}\n", imports.join("\n")), 1);
        source = crate::generate::normalize_imports(&source);
    }
    source.replace(
        &format!(" * An immutable {class} value."),
        &format!(" * Typed filters for the {name} query."),
    )
}

fn query_port_java(slice: &Slice, name: &str, target: &str) -> String {
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let target_import = crate::generate::import_of(pkg, domain, target);
    crate::template::render(
        crate::template::template!("spring/query_port_java.java"),
        &[
            ("pkg", pkg),
            ("target_import", &*target_import),
            ("name", name),
            ("target", target),
        ],
    )
}

fn jdbc_query_java(slice: &Slice, name: &str, target: &str, projection: &Projection) -> String {
    let pkg: &str = &slice.owned(Layer::Adapters);
    let service: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let target_columns: &[crate::sql::Column] = &projection.target_columns;
    let filter_columns: &[crate::sql::Column] = &projection.filter_columns;
    let target_import = crate::generate::import_of(pkg, domain, target);
    let query_import = crate::generate::import_of(pkg, service, &format!("{name}Query"));
    let port_import = crate::generate::import_of(pkg, service, &format!("{name}QueryPort"));
    let mut imports = crate::sql::imports(target_columns)
        .into_iter()
        .chain(crate::sql::imports(filter_columns))
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    if target_columns.iter().any(|column| {
        column
            .read
            .as_deref()
            .is_some_and(|read| read.contains("Optional."))
    }) {
        imports.push_str("import java.util.Optional;\n");
    }
    for column in target_columns.iter().chain(filter_columns.iter()) {
        if crate::generate::builtin_by_java_name(&column.java_type).is_none() {
            imports.push_str(&crate::generate::import_of(pkg, domain, &column.java_type));
        }
    }
    let select = target_columns
        .iter()
        .map(|column| format!("            {},", column.name))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end_matches(',')
        .to_string();
    let predicates = filter_columns
        .iter()
        .map(|column| format!("{} = :{}", column.name, column.name))
        .collect::<Vec<_>>()
        .join("\n                          and ");
    let bindings = filter_columns
        .iter()
        .map(|column| {
            format!(
                "                .param(\"{}\", {})",
                column.name,
                column.write.as_deref().expect("mapped query column")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let map_args = target_columns
        .iter()
        .map(|column| {
            format!(
                "                {}",
                column.read.as_deref().expect("mapped target column")
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let table = crate::sql::table_name(target);
    let order = target_columns
        .iter()
        .find(|column| column.name == "id")
        .map(|column| column.name.as_str())
        .unwrap_or(&target_columns[0].name);
    crate::template::render(
        crate::template::template!("spring/jdbc_query_java.java"),
        &[
            ("pkg", pkg),
            ("target_import", &*target_import),
            ("query_import", &*query_import),
            ("port_import", &*port_import),
            ("imports", &*imports),
            ("name", name),
            ("select", &*select),
            ("target", target),
            ("table", &*table),
            ("predicates", &*predicates),
            ("order", order),
            ("bindings", &*bindings),
            ("map_args", &*map_args),
        ],
    )
}

fn jdbc_query_it_java(
    slice: &Slice,
    name: &str,
    resource: &Target,
    fields: &[crate::generate::Field],
) -> String {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.owned(Layer::Adapters);
    let service: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let app: &str = &slice.owned(Layer::App);
    let target_fields: &[crate::generate::Field] = &resource.fields;
    let target: &str = &resource.name;
    let query_samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let target_samples = target_fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = query_samples.is_none() || target_samples.is_none();
    let query_args = query_samples
        .unwrap_or_default()
        .join(",\n                ");
    let target_args = target_samples
        .unwrap_or_default()
        .join(",\n                ");
    let target_import = crate::generate::import_of(pkg, domain, target);
    let query_import = crate::generate::import_of(pkg, service, &format!("{name}Query"));
    let port_import = crate::generate::import_of(pkg, service, &format!("{name}QueryPort"));
    let repository_import = crate::generate::import_of(pkg, app, &format!("{target}Repository"));
    let imports = java_literal_imports(target_fields, domain)
        .into_iter()
        .chain(java_literal_imports(fields, domain))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let disabled_import = if disabled {
        "import org.junit.jupiter.api.Disabled;\n"
    } else {
        ""
    };
    let annotation = if disabled {
        "@Disabled(\"todo: supply query/target samples Jails cannot fabricate\")\n"
    } else {
        ""
    };
    crate::template::render(
        crate::template::template!("spring/jdbc_query_it_java.java"),
        &[
            ("pkg", pkg),
            ("target_import", &*target_import),
            ("query_import", &*query_import),
            ("port_import", &*port_import),
            ("repository_import", &*repository_import),
            ("imports", &*imports),
            ("disabled_import", disabled_import),
            ("annotation", annotation),
            ("name", name),
            ("target", target),
            ("target_args", &*target_args),
            ("query_args", &*query_args),
        ],
    )
}

fn query_controller_java(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
) -> String {
    let security: &str = slice.base();
    let service: &str = &slice.placed(Layer::Service);
    let web: &str = &slice.placed(Layer::Web);
    let query_import = crate::generate::import_of(web, service, &format!("{name}Query"));
    let port_import = crate::generate::import_of(web, service, &format!("{name}QueryPort"));
    let path = format!(
        "/queries/{}",
        crate::sql::snake_case(name).replace('_', "-")
    );
    let (
        scope_import,
        scope_field,
        scope_constructor,
        scope_assignment,
        scope_parameter,
        scope_checks,
    ) = scope_controller_parts(security, web, fields, "query");
    crate::template::render(
        crate::template::template!("spring/query_controller_java.java"),
        &[
            ("web", web),
            ("query_import", &*query_import),
            ("port_import", &*port_import),
            ("scope_import", &*scope_import),
            ("name", name),
            ("path", &*path),
            ("scope_field", &*scope_field),
            ("scope_constructor", &*scope_constructor),
            ("scope_assignment", &*scope_assignment),
            ("target", target),
            ("scope_parameter", &*scope_parameter),
            ("scope_checks", &*scope_checks),
        ],
    )
}

fn query_controller_test_java(
    slice: &Slice,
    name: &str,
    resource: &Target,
    fields: &[crate::generate::Field],
) -> String {
    let root: &Path = slice.project().root();
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
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = json.is_none() || target_samples.is_none();
    let json = json.unwrap_or_default().join(",\n");
    let target_args = target_samples
        .unwrap_or_default()
        .join(",\n                    ");
    let port_import = crate::generate::import_of(web, service, &format!("{name}QueryPort"));
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
    crate::template::render(
        crate::template::template!("spring/query_controller_test_java.java"),
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
