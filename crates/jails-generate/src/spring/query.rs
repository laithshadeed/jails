//! `g query`: a read model over a resource that already exists.
//!
//! The sibling of `usecase` (`workflow.rs`, which documents the shape all three
//! share) and `transition`. What makes this one its own kind is the
//! projection: a query names a subset of the target's components, and that
//! subset drives the record, the select list and the row mapper together --
//! which is the same reason `sql.rs` owns one column list rather than five.

use super::workflow::{json_sample, scope_test_parts};
use super::*;

mod shape;
pub(crate) use shape::Bounds;
use shape::{DEFAULT_MAX_RESULTS, Projection, declared_ordering, resolve_join};

use crate::model::{Artifact, Layer, Slice};

pub(crate) fn query_files(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
    via: Option<&str>,
    bounds: Bounds<'_>,
    endpoint: Endpoint<'_>,
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
        ).into());
    }
    // A collection filter is still refused: `status:TaskStatus[]` could mean
    // `in (...)`, `= any(...)` or "all of these", and jails is not going to
    // pick one. An **optional** filter is a different question with one
    // answer -- absent means "do not filter on this" -- and `missing.md` M16
    // is what refusing it cost: an admin list with three independent filters
    // is eight queries, or none.
    if let Some(field) = fields.iter().find(|field| field.collection) {
        return Err(format!(
            "query {name} filter `{}` is a collection. A list could mean `in (...)`, `= \
             any(...)` or every one of them, and jails will not choose.\n       fix: declare \
             it as a scalar, or write the query by hand.",
            field.name
        )
        .into());
    }
    // A path template names filters, and every one of them has to be a filter
    // this query takes -- otherwise the URL matches and the value goes
    // nowhere. All or none, deliberately: a mix would need the controller to
    // build the criteria from a partial body plus some path variables, and
    // "which half came from where" is a rule nobody would remember.
    let variables = path_variables(endpoint.route);
    if !variables.is_empty() {
        let declared: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
        if let Some(unknown) = variables
            .iter()
            .find(|variable| !declared.contains(&variable.as_str()))
        {
            return Err(format!(
                "query {name}'s path names `{{{unknown}}}`, which is not one of its \
                 filters.\n       fix: declare it -- this query takes {}.",
                declared.join(", ")
            )
            .into());
        }
        if variables.len() != fields.len() {
            let missing: Vec<&str> = declared
                .iter()
                .copied()
                .filter(|field| !variables.iter().any(|variable| variable == field))
                .collect();
            return Err(format!(
                "query {name}'s path addresses {} of its {} filters, and {} would have to come \
                 from a request body.\n       fix: put {} in the path too, or take the \
                 template variables out of it.",
                variables.len(),
                fields.len(),
                missing.join(", "),
                missing.join(" and ")
            )
            .into());
        }
    }
    let target_fields = Target::read(slice, "query", name, target)?.fields;
    let join = via
        .map(|parent| resolve_join(slice, name, target, &target_fields, parent))
        .transpose()?;
    let target_table = crate::sql::table_name(target);
    // Which side of the join each filter reads. Without `--via` there is one
    // side, and the answer is the same as it always was.
    let mut filter_qualifiers = Vec::with_capacity(fields.len());
    for field in fields {
        let owner = target_fields
            .iter()
            .find(|candidate| candidate.name == field.name)
            .map(|found| (found, target, String::new()))
            .or_else(|| {
                let join = join.as_ref()?;
                join.parent_fields
                    .iter()
                    .find(|candidate| candidate.name == field.name)
                    .map(|found| {
                        (
                            found,
                            join.parent.as_str(),
                            format!("{}.", join.parent_table),
                        )
                    })
            });
        let Some((source, owner, qualifier)) = owner else {
            let known = |fields: &[crate::generate::Field]| {
                fields
                    .iter()
                    .map(|candidate| candidate.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            return Err(match &join {
                Some(join) => format!(
                    "query {name} filters `{}`, but neither {target} nor {} has a component with \
                     that name.\n       fix: filter one of: {}, {}.",
                    field.name,
                    join.parent,
                    known(&target_fields),
                    known(&join.parent_fields)
                ),
                None => format!(
                    "query {name} filters `{}`, but {target} has no component with that \
                     name.\n       fix: filter one of: {}.",
                    field.name,
                    known(&target_fields)
                ),
            }
            .into());
        };
        if usecase_normalized_type(&field.java_type) != usecase_normalized_type(&source.java_type) {
            return Err(format!(
                "query {name} declares `{}` as {}, but {owner} stores it as {}",
                field.name, field.java_type, source.java_type
            )
            .into());
        }
        // A joined query qualifies every column, including the target's own:
        // one unqualified name in a two-table select is a column reference
        // Postgres may or may not be able to resolve, depending on what the
        // other table happens to be called today.
        filter_qualifiers.push(if join.is_none() {
            String::new()
        } else if qualifier.is_empty() {
            format!("{target_table}.")
        } else {
            qualifier
        });
    }
    let target_columns = crate::sql::columns(&target_fields, slice.project(), domain, "row");
    // The receiver baked into every bind expression: the criteria record
    // the port takes. plan.md P3.4 renamed it from `query`, which the port
    // interface now owns.
    let filter_columns = crate::sql::columns(fields, slice.project(), domain, "criteria");
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
        ).into());
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
    let ordering = declared_ordering(name, target, &target_columns, bounds.order_by)?;
    if let Some(limit) = bounds.limit
        && limit == 0
    {
        return Err(jails_support::Failure::Told(format!(
            "query {name} declares `--limit 0`, which can only ever return nothing.\n                    fix: pass a positive row ceiling, or omit it for the default of \
             {DEFAULT_MAX_RESULTS}."
        )));
    }
    let projection = Projection {
        target_columns,
        filter_columns,
        filter_qualifiers,
        join,
        ordering,
        limit: bounds.limit.unwrap_or(DEFAULT_MAX_RESULTS),
    };
    Ok(vec![
        Artifact {
            kind: "query criteria",
            path: main_service.join(format!("{name}Criteria.java")),
            contents: query_record_java(slice, name, fields, endpoint),
        },
        Artifact {
            kind: "query",
            path: main_service.join(format!("{name}Query.java")),
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
            contents: jdbc_query_it_java(slice, name, &resource, fields, &projection),
        },
        Artifact {
            kind: "query controller",
            path: main_web.join(format!("{name}QueryController.java")),
            contents: query_controller_java(slice, name, target, fields, endpoint),
        },
        Artifact {
            kind: "query controller test",
            path: test_web.join(format!("{name}QueryControllerTest.java")),
            contents: query_controller_test_java(slice, name, &resource, fields),
        },
    ])
}

fn query_record_java(
    slice: &Slice,
    name: &str,
    fields: &[crate::generate::Field],
    endpoint: Endpoint<'_>,
) -> String {
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let class = format!("{name}Criteria");
    let mut source = crate::generate::bound_record_java(
        pkg,
        &class,
        fields,
        endpoint.binding_naming(slice.project()),
    );
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
        source = jails_java::tidy::normalize_imports(&source);
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
        crate::template_here!("spring/query_port_java.java"),
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
    let query_import = crate::generate::import_of(pkg, service, &format!("{name}Criteria"));
    let port_import = crate::generate::import_of(pkg, service, &format!("{name}Query"));
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
    let table = crate::sql::table_name(target);
    // A joined select qualifies everything, including the target's own
    // columns: one bare name across two tables is a reference Postgres may or
    // may not resolve, depending on what the other table happens to hold.
    let own = match &projection.join {
        Some(_) => format!("{table}."),
        None => String::new(),
    };
    let select = target_columns
        .iter()
        .map(|column| format!("            {own}{},", column.name))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end_matches(',')
        .to_string();
    let predicates = filter_columns
        .iter()
        .zip(&projection.filter_qualifiers)
        .map(|(column, qualifier)| {
            let equality = format!("{qualifier}{} = :{}", column.name, column.name);
            if column.not_null {
                return equality;
            }
            // `(cast(:x as type) is null or col = :x)`, and the cast is
            // load-bearing rather than decoration: PostgreSQL rejects a bare
            // `$1 is null` with "could not determine data type of parameter",
            // because that position gives the parameter no type to infer
            // from. The second occurrence needs none -- the column supplies
            // it. jails knows the type here because `Column` carries the one
            // the DDL declared.
            format!(
                "(cast(:{} as {}) is null or {equality})",
                column.name, column.sql_type
            )
        })
        .collect::<Vec<_>>()
        .join("\n                          and ");
    let from = match &projection.join {
        Some(join) => format!(
            "{table}\n                        join {parent} on {table}.{child} = {parent}.{key}",
            parent = join.parent_table,
            child = join.child_column,
            key = join.parent_column,
        ),
        None => table.clone(),
    };
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
    // Declared if the caller said so, otherwise newest first and not by key:
    // `order by id` over a random UUID is a stable random order presented to a
    // reader as their data. plan.md P4.4, P8.2.
    let order = if projection.ordering.is_empty() {
        crate::sql::ordering(target_columns)
    } else {
        projection.ordering.join(", ")
    }
    .split(", ")
    .map(|term| format!("{own}{term}"))
    .collect::<Vec<_>>()
    .join(", ");
    let limit = projection.limit.to_string();
    crate::template::render(
        crate::template_here!("spring/jdbc_query_java.java"),
        &[
            ("pkg", pkg),
            ("target_import", &*target_import),
            ("query_import", &*query_import),
            ("port_import", &*port_import),
            ("imports", &*imports),
            ("name", name),
            ("select", &*select),
            ("target", target),
            ("from", &*from),
            ("predicates", &*predicates),
            ("order", &order),
            ("limit", &*limit),
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
    projection: &Projection,
) -> String {
    let project = slice.project();
    let pkg: &str = &slice.owned(Layer::Adapters);
    let service: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let app: &str = &slice.owned(Layer::App);
    let target_fields: &[crate::generate::Field] = &resource.fields;
    let target: &str = &resource.name;
    // With a join, the row this test stores has to *match* the parent row it
    // stores first, or the query correctly returns nothing and the test fails
    // for a reason that has nothing to do with the query. The foreign key
    // component and every parent-side filter are read off the saved parent
    // rather than sampled independently.
    let join = projection.join.as_ref();
    let parent_samples = join.map(|join| {
        join.parent_fields
            .iter()
            .map(|field| crate::generate::sample_value(field, project, domain))
            .collect::<Option<Vec<_>>>()
    });
    let query_samples = fields
        .iter()
        .map(|field| match join {
            Some(join)
                if join
                    .parent_fields
                    .iter()
                    .any(|candidate| candidate.name == field.name) =>
            {
                Some(format!("parent.{}()", field.name))
            }
            _ => crate::generate::sample_value(field, project, domain),
        })
        .collect::<Option<Vec<_>>>();
    let target_samples = target_fields
        .iter()
        .map(|field| match join {
            Some(join) if join.child_component == field.name => {
                Some(format!("parent.{}()", join.parent_component))
            }
            _ => crate::generate::sample_value(field, project, domain),
        })
        .collect::<Option<Vec<_>>>();
    let disabled = query_samples.is_none()
        || target_samples.is_none()
        || parent_samples.as_ref().is_some_and(Option::is_none);
    let query_args = query_samples
        .unwrap_or_default()
        .join(",\n                ");
    let target_args = target_samples
        .unwrap_or_default()
        .join(",\n                ");
    let (parent_autowire, parent_import, parent_setup) = match join {
        Some(join) => (
            format!(
                "    @Autowired\n    private {}Repository parents;\n\n",
                join.parent
            ),
            crate::generate::import_of(pkg, domain, &join.parent)
                + &crate::generate::import_of(pkg, app, &format!("{}Repository", join.parent)),
            format!(
                "        {parent} parent = parents.save(new {parent}(\n                {args}));\n",
                parent = join.parent,
                args = parent_samples
                    .unwrap_or_default()
                    .unwrap_or_default()
                    .join(",\n                "),
            ),
        ),
        None => (String::new(), String::new(), String::new()),
    };
    let target_import = crate::generate::import_of(pkg, domain, target);
    let query_import = crate::generate::import_of(pkg, service, &format!("{name}Criteria"));
    let port_import = crate::generate::import_of(pkg, service, &format!("{name}Query"));
    let repository_import = crate::generate::import_of(pkg, app, &format!("{target}Repository"));
    let imports = java_literal_imports(target_fields, domain)
        .into_iter()
        .chain(java_literal_imports(fields, domain))
        .chain(
            join.map(|join| java_literal_imports(&join.parent_fields, domain))
                .unwrap_or_default(),
        )
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
        crate::template_here!("spring/jdbc_query_it_java.java"),
        &[
            ("pkg", pkg),
            ("target_import", &*target_import),
            ("query_import", &*query_import),
            ("port_import", &*port_import),
            ("repository_import", &*repository_import),
            ("parent_import", &*parent_import),
            ("imports", &*imports),
            ("disabled_import", disabled_import),
            ("annotation", annotation),
            ("name", name),
            ("target", target),
            ("parent_autowire", &*parent_autowire),
            ("parent_setup", &*parent_setup),
            ("target_args", &*target_args),
            ("query_args", &*query_args),
        ],
    )
}

/// The filters a route addresses by name, in the order the URL names them.
///
/// `--path /admin_api/messages/{userId}` used to be accepted as *text*: the
/// controller carried the template in its `@RequestMapping` and declared no
/// `@PathVariable` at all, so Spring matched the URL and then looked for a
/// request body nobody sent. A path jails cannot honour is a path jails must
/// not accept.
pub(super) fn path_variables(route: Option<&str>) -> Vec<String> {
    let Some(route) = route else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut rest = route;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        found.push(rest[open + 1..open + close].to_string());
        rest = &rest[open + close..];
    }
    found
}

fn query_controller_java(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
    endpoint: Endpoint<'_>,
) -> String {
    let route = endpoint.route;
    let variables = path_variables(route);
    if !variables.is_empty() {
        return query_controller_path_java(slice, name, target, fields, endpoint);
    }
    let security: &str = slice.base();
    let service: &str = &slice.placed(Layer::Service);
    let web: &str = &slice.placed(Layer::Web);
    let query_import = crate::generate::import_of(web, service, &format!("{name}Criteria"));
    let port_import = crate::generate::import_of(web, service, &format!("{name}Query"));
    // The contract when the caller names one, the derived shape when they do
    // not. `missing.md` M8: a derived path is a virtue greenfield and unusable
    // when the URLs are a fixed external contract.
    let path = route.map(str::to_string).unwrap_or_else(|| {
        format!(
            "/queries/{}",
            crate::sql::snake_case(name).replace('_', "-")
        )
    });
    let (
        scope_import,
        scope_field,
        scope_constructor,
        scope_assignment,
        scope_parameter,
        scope_checks,
    ) = scope_controller_parts(security, web, fields, "criteria");
    crate::template::render(
        crate::template_here!("spring/query_controller_java.java"),
        &[
            (
                "validation",
                crate::spring::validation_package(slice.project()),
            ),
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
            ("binding", endpoint.binding()),
            ("binding_import", endpoint.binding_import()),
        ],
    )
}

/// The GET-by-path variant: every filter is in the URL, so there is no body.
fn query_controller_path_java(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
    endpoint: Endpoint<'_>,
) -> String {
    let security: &str = slice.base();
    let service: &str = &slice.placed(Layer::Service);
    let web: &str = &slice.placed(Layer::Web);
    let query_import = crate::generate::import_of(web, service, &format!("{name}Criteria"));
    let port_import = crate::generate::import_of(web, service, &format!("{name}Query"));
    let (
        scope_import,
        scope_field,
        scope_constructor,
        scope_assignment,
        scope_parameter,
        scope_checks,
    ) = scope_controller_parts(security, web, fields, "criteria");
    // Declared in the order the *record* takes them, not the order the URL
    // spells them: the criteria is constructed positionally, and a URL that
    // happened to name them the other way round would compile and silently
    // swap two values of the same type.
    let path_parameters = fields
        .iter()
        .map(|field| {
            format!(
                "@PathVariable {} {}",
                crate::generate::declared_type(field),
                field.name
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let criteria_arguments = fields
        .iter()
        .map(|field| field.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    crate::template::render(
        crate::template_here!("spring/query_controller_path_java.java"),
        &[
            ("web", web),
            ("query_import", &*query_import),
            ("port_import", &*port_import),
            ("scope_import", &*scope_import),
            ("name", name),
            ("path", endpoint.route.unwrap_or_default()),
            ("scope_field", &*scope_field),
            ("scope_constructor", &*scope_constructor),
            ("scope_assignment", &*scope_assignment),
            ("target", target),
            ("scope_parameter", &*scope_parameter),
            ("scope_checks", &*scope_checks),
            ("path_parameters", &*path_parameters),
            ("criteria_arguments", &*criteria_arguments),
        ],
    )
}

fn query_controller_test_java(
    slice: &Slice,
    name: &str,
    resource: &Target,
    fields: &[crate::generate::Field],
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

#[cfg(test)]
mod join_tests {
    use super::*;
    use crate::generate::parse_fields;
    use crate::model::Project;

    /// A project with two records on disk and a JDBC starter, which is all
    /// `g query` reads.
    fn two_records(label: &str) -> (std::path::PathBuf, Project) {
        let (root, _) = crate::spring::scratch_project(
            label,
            r#"<project><dependencies><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-jdbc</artifactId></dependency></dependencies></project>"#,
        );
        std::fs::create_dir_all(root.join("src/main/java/com/example/demo/domain")).unwrap();
        for (name, specs) in [
            ("Owner", vec!["id:uuid@pk", "email:string!"]),
            ("Item", vec!["id:uuid@pk", "ownerId:uuid", "name:string!"]),
        ] {
            let fields = parse_fields(
                &specs
                    .iter()
                    .map(|spec| (*spec).to_string())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            std::fs::write(
                root.join(format!("src/main/java/com/example/demo/domain/{name}.java")),
                crate::generate::record_java("com.example.demo.domain", name, &fields),
            )
            .unwrap();
        }
        let project = Project::load(&root).unwrap();
        (root, project)
    }

    #[test]
    fn a_join_reads_a_filter_the_target_does_not_own() {
        let (root, project) = two_records("query-via-join");
        let fields = parse_fields(&["email:string!".to_string()]).unwrap();
        let files = query_files(
            &Slice::new(&project, None),
            "ItemsByOwnerEmail",
            "Item",
            &fields,
            Some("Owner"),
            Bounds {
                order_by: None,
                limit: None,
            },
            Endpoint::json(),
        )
        .unwrap();
        let adapter = &files[2].contents;

        assert!(
            adapter.contains("join owners on items.owner_id = owners.id"),
            "{adapter}"
        );
        assert!(adapter.contains("where owners.email = :email"), "{adapter}");
        // Everything qualified, including the target's own: one bare name
        // across two tables is a reference Postgres may or may not resolve.
        assert!(adapter.contains("items.id,"), "{adapter}");
        assert!(!adapter.contains("order by id"), "{adapter}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_join_refuses_when_two_components_could_be_the_reference() {
        let (root, project) = two_records("query-via-ambiguous");
        std::fs::write(
            root.join("src/main/java/com/example/demo/domain/Item.java"),
            crate::generate::record_java(
                "com.example.demo.domain",
                "Item",
                &parse_fields(&[
                    "id:uuid@pk".to_string(),
                    "buyerId:uuid".to_string(),
                    "sellerId:uuid".to_string(),
                ])
                .unwrap(),
            ),
        )
        .unwrap();
        let project = Project::load(project.root()).unwrap();
        let filters = parse_fields(&["email:string!".to_string()]).unwrap();
        let error = query_files(
            &Slice::new(&project, None),
            "ItemsByOwnerEmail",
            "Item",
            &filters,
            Some("Owner"),
            Bounds {
                order_by: None,
                limit: None,
            },
            Endpoint::json(),
        )
        .unwrap_err();

        assert!(error.contains("buyerId, sellerId"), "{error}");
        assert!(error.contains("`ownerId`"), "{error}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_declared_order_and_ceiling_replace_the_silent_ones() {
        let (root, project) = two_records("query-order-limit");
        let fields = parse_fields(&["ownerId:uuid".to_string()]).unwrap();
        let files = query_files(
            &Slice::new(&project, None),
            "ItemsByOwner",
            "Item",
            &fields,
            None,
            Bounds {
                // The component and the column are both accepted: each names
                // exactly one column, and refusing one would be arbitrary.
                order_by: Some("name desc, id"),
                limit: Some(20),
            },
            Endpoint::json(),
        )
        .unwrap();
        let adapter = &files[2].contents;

        assert!(adapter.contains("order by name desc, id"), "{adapter}");
        assert!(adapter.contains("MAX_RESULTS = 20"), "{adapter}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_order_by_a_component_the_target_lacks_lists_the_ones_it_has() {
        let (root, project) = two_records("query-order-unknown");
        let fields = parse_fields(&["ownerId:uuid".to_string()]).unwrap();
        let error = query_files(
            &Slice::new(&project, None),
            "ItemsByOwner",
            "Item",
            &fields,
            None,
            Bounds {
                order_by: Some("sentAt desc"),
                limit: None,
            },
            Endpoint::json(),
        )
        .unwrap_err();

        assert!(error.contains("orders by `sentAt`"), "{error}");
        assert!(error.contains("id, ownerId, name"), "{error}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_ceiling_of_zero_is_refused_rather_than_generated() {
        let (root, project) = two_records("query-zero-limit");
        let fields = parse_fields(&["ownerId:uuid".to_string()]).unwrap();
        let error = query_files(
            &Slice::new(&project, None),
            "ItemsByOwner",
            "Item",
            &fields,
            None,
            Bounds {
                order_by: None,
                limit: Some(0),
            },
            Endpoint::json(),
        )
        .unwrap_err();

        assert!(error.contains("can only ever return nothing"), "{error}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_filter_on_neither_side_names_both() {
        let (root, project) = two_records("query-via-unknown");
        let filters = parse_fields(&["nickname:string!".to_string()]).unwrap();
        let error = query_files(
            &Slice::new(&project, None),
            "ItemsByNickname",
            "Item",
            &filters,
            Some("Owner"),
            Bounds {
                order_by: None,
                limit: None,
            },
            Endpoint::json(),
        )
        .unwrap_err();

        assert!(error.contains("neither Item nor Owner"), "{error}");
        std::fs::remove_dir_all(root).unwrap();
    }
}
