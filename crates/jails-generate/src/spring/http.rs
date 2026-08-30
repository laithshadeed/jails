//! Outbound HTTP: `client`, `fetcher` and `http-workflow`.
//!
//! Three kinds with one shared concern -- calling something that is not you --
//! and three different answers to it. `client` is a declarative interface
//! Spring implements, for an API you trust and whose shape you know.
//! `fetcher` is bounded and SSRF-safe, for a URL somebody else chose.
//! `http-workflow` is a durable traversal built on top of a fetcher.
//!
//! The reason they are together is the reason they are separate kinds: the
//! difference between "an API we integrate with" and "a URL from a request" is
//! the entire security boundary, and a single generic HTTP kind would put both
//! on the same side of it.

use super::*;

// ---------------------------------------------------------------------------
// `generate client` -- a declarative HTTP client.
// ---------------------------------------------------------------------------

/// The files for `jails generate client <Name>`.
///
/// Spring Boot 4 registers `@HttpExchange` interfaces itself, given
/// `@ImportHttpServices`, and binds each group's base URL to
/// `spring.http.serviceclient.<group>.base-url`. That combination replaces
/// the usual hand-written client: no `RestTemplate` field, no URI building,
/// no response-entity unwrapping, and the base URL is configuration rather
/// than a constant compiled into the jar.
/// What a caller said this client actually does, when they said anything.
///
/// `missing.md` M7: `--method`, `--on` and `--returns` were accepted and
/// silently discarded, so `g client Gamma --method post --on Rq` reported
/// success and wrote a `GetExchange` REST collection referencing neither. The
/// generated shape was 100% overwritten in the one real project that used it.
///
/// Naming any of the three switches the interface from that collection to the
/// one call it describes. `--path` alone does not: renaming the collection's
/// base path is a different, coherent thing to want.
pub(crate) struct Call<'a> {
    pub(crate) method: Option<jails_spec::spec::kind::HttpMethod>,
    pub(crate) accepts: Option<&'a str>,
    pub(crate) returns: Option<&'a str>,
    pub(crate) path: Option<&'a str>,
}

impl Call<'_> {
    /// Whether the caller described a call rather than accepting the
    /// collection.
    fn described(&self) -> bool {
        self.method.is_some() || self.accepts.is_some() || self.returns.is_some()
    }
}

pub(crate) fn client_files(slice: &Slice, name: &str, call: &Call<'_>) -> Vec<Artifact> {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.placed(Layer::Clients);
    let domain: &str = &slice.owned(Layer::Domain);
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    let group = client_group(name);
    if call.described() {
        return one_call_files(slice, name, call, pkg, domain, &group, &main, &test);
    }
    vec![
        Artifact {
            kind: "http client",
            path: main.join(format!("{name}Client.java")),
            contents: client_interface_java(pkg, name, call.path),
        },
        Artifact {
            kind: "http client registration",
            path: main.join(format!("{name}ClientConfig.java")),
            contents: client_config_java(pkg, name, &group),
        },
        Artifact {
            kind: "http client test",
            path: test.join(format!("{name}ClientTest.java")),
            contents: client_test_java(pkg, name, &group, call.path),
        },
    ]
}

/// The client for a call the caller described: one method, their verb, their
/// path, their types.
///
/// The test is generated whole and `@Disabled` when jails cannot fabricate the
/// types -- the same rule `g controller` follows, and for the same reason:
/// jails has no type model, so a stub response it invented would be a test of
/// its guess.
#[allow(clippy::too_many_arguments)]
fn one_call_files(
    slice: &Slice,
    name: &str,
    call: &Call<'_>,
    pkg: &str,
    domain: &str,
    group: &str,
    main: &Path,
    test: &Path,
) -> Vec<Artifact> {
    let method = call
        .method
        .unwrap_or(jails_spec::spec::kind::HttpMethod::Get);
    let path = call
        .path
        .map(str::to_string)
        .unwrap_or_else(|| format!("/{}", crate::sql::table_name(name).replace('_', "-")));
    let imports: String = [call.accepts, call.returns]
        .into_iter()
        .flatten()
        .map(|ty| crate::generate::import_of(pkg, domain, ty))
        .collect();
    let returns = call.returns.unwrap_or("String");
    let (parameter, argument, body_import, sample) = match call.accepts {
        Some(ty) => (
            format!("@RequestBody {ty} request"),
            "sample()".to_string(),
            "import org.springframework.web.bind.annotation.RequestBody;\n",
            // Generated whole and throwing rather than as a value jails
            // invented: it has no type model, so a body it made up would be a
            // test of its guess. Unreachable while the class is `@Disabled`,
            // and it names the work.
            format!(
                "\n    private static {ty} sample() {{\n        throw new \
                 UnsupportedOperationException(\n                \"todo: build the {ty} this \
                 call sends\");\n    }}\n"
            ),
        ),
        None => (String::new(), String::new(), "", String::new()),
    };
    let disabled = call.accepts.is_some() || call.returns.is_some();
    let _ = slice;
    vec![
        Artifact {
            kind: "http client",
            path: main.join(format!("{name}Client.java")),
            contents: crate::template::render(
                crate::template_here!("spring/client_call_java.java"),
                &[
                    ("pkg", pkg),
                    ("imports", &imports),
                    ("body_import", body_import),
                    ("exchange", method.exchange()),
                    ("name", name),
                    ("path", &path),
                    ("returns", returns),
                    ("parameter", &parameter),
                ],
            ),
        },
        Artifact {
            kind: "http client registration",
            path: main.join(format!("{name}ClientConfig.java")),
            contents: client_config_java(pkg, name, group),
        },
        Artifact {
            kind: "http client test",
            path: test.join(format!("{name}ClientTest.java")),
            contents: crate::template::render(
                crate::template_here!("spring/client_call_test_java.java"),
                &[
                    ("pkg", pkg),
                    ("imports", &imports),
                    (
                        "disabled_import",
                        if disabled {
                            "import org.junit.jupiter.api.Disabled;\n"
                        } else {
                            ""
                        },
                    ),
                    (
                        "disabled",
                        &if disabled {
                            format!(
                                "@Disabled(\"todo: build the {} this call needs, then delete \
                                 this @Disabled\")\n",
                                call.accepts.or(call.returns).unwrap_or("value")
                            )
                        } else {
                            String::new()
                        },
                    ),
                    ("name", name),
                    ("path", &path),
                    ("group", group),
                    ("argument", &argument),
                    ("sample", &sample),
                ],
            ),
        },
    ]
}

/// The three settings every remote call needs, and none of which had a value.
///
/// `backend.md` §1 makes this the fourth of five reflexes and admits no
/// exceptions: *every remote call has a timeout, a bounded retry, and a
/// defined failure mode*. A generator whose entire subject is an outbound HTTP
/// call is the one place that cannot be left to the reader -- and it was: a
/// real generated client had no base URL, no connect timeout, no read timeout,
/// no retry and no auth, and `grep timeout application.properties` found only
/// Hikari's.
///
/// Written from the plan, beside the dependency splice, for the reason
/// `ensure_failsafe` and `ensure_assertj` are: a rule the reader has to
/// remember is a rule that decays, and the failure is silent until production.
///
/// The prefix and both timeout keys are `HttpClientProperties extends
/// HttpClientSettingsProperties` in `spring-boot-http-client`, checked in
/// `deps/spring-boot` at v4.0.0 rather than recalled. The base URL is
/// `.invalid` for the same reason `add cors`'s origin is: RFC 2606 reserves
/// it, so it can never resolve and is unmistakably a value somebody has to
/// replace -- which is better than the alternative failure, a first call that
/// dies on `URI with undefined scheme`, a message saying nothing about a
/// missing setting.
pub(crate) fn client_properties(group: &str) -> Vec<String> {
    vec![
        format!("# Where `{group}` points. Replace it: `.invalid` can never resolve, and an"),
        "# unset base URL fails the first call with `URI with undefined scheme`.".to_string(),
        format!("spring.http.serviceclient.{group}.base-url=https://example.invalid"),
        "# A stalled dependency holds a request thread until the client gives up, and with"
            .to_string(),
        "# no timeout that is never. Both halves are needed: connect covers a host that"
            .to_string(),
        "# does not answer, read covers one that answers and then stops.".to_string(),
        format!("spring.http.serviceclient.{group}.connect-timeout=2s"),
        format!("spring.http.serviceclient.{group}.read-timeout=5s"),
    ]
}

/// The configuration group a client's settings hang off.
pub(crate) fn client_group(name: &str) -> String {
    crate::sql::snake_case(name).replace('_', "-")
}

fn client_interface_java(pkg: &str, name: &str, route: Option<&str>) -> String {
    let path = route
        .map(str::to_string)
        .unwrap_or_else(|| format!("/{}", crate::sql::table_name(name).replace('_', "-")));
    crate::template::render(
        crate::template_here!("spring/client_interface_java.java"),
        &[("pkg", pkg), ("name", name), ("path", &*path)],
    )
}

fn client_config_java(pkg: &str, name: &str, group: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/client_config_java.java"),
        &[("pkg", pkg), ("name", name), ("group", group)],
    )
}

fn client_test_java(pkg: &str, name: &str, group: &str, route: Option<&str>) -> String {
    let path = route
        .map(str::to_string)
        .unwrap_or_else(|| format!("/{}", crate::sql::table_name(name).replace('_', "-")));
    crate::template::render(
        crate::template_here!("spring/client_test_java.java"),
        &[
            ("pkg", pkg),
            ("name", name),
            ("path", &*path),
            ("group", group),
        ],
    )
}

// ---------------------------------------------------------------------------
// `generate fetcher` -- bounded, SSRF-safe outbound bytes.
// ---------------------------------------------------------------------------

pub(crate) fn fetcher_files(slice: &Slice, name: &str) -> Vec<Artifact> {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.placed(Layer::Clients);
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    let property = crate::sql::snake_case(name).replace('_', "-");
    vec![
        Artifact {
            kind: "safe fetch port",
            path: main.join(format!("{name}Fetcher.java")),
            contents: crate::template::render(
                crate::template_here!("spring/fetcher_port_java.java"),
                &[("pkg", pkg), ("name", name)],
            ),
        },
        Artifact {
            kind: "safe fetch adapter",
            path: main.join(format!("Safe{name}Fetcher.java")),
            contents: crate::template::render(
                crate::template_here!("spring/safe_fetcher_java.java"),
                &[("pkg", pkg), ("name", name), ("property", &property)],
            ),
        },
        Artifact {
            kind: "safe fetch adversarial test",
            path: test.join(format!("Safe{name}FetcherTest.java")),
            contents: crate::template::render(
                crate::template_here!("spring/safe_fetcher_test_java.java"),
                &[("pkg", pkg), ("name", name)],
            ),
        },
    ]
}

// ---------------------------------------------------------------------------
// `generate http-workflow` -- durable bounded traversal over a safe fetcher.
// ---------------------------------------------------------------------------

pub(crate) fn http_workflow_files(
    slice: &Slice,
    name: &str,
    fetcher: &str,
) -> jails_support::Result<Vec<Artifact>> {
    let root: &Path = slice.project().root();
    let jobs: &str = &slice.placed(Layer::Jobs);
    let clients: &str = &slice.owned(Layer::Clients);
    let web: &str = &slice.owned(Layer::Web);
    // Through the resolved project rather than a fresh read: in an aggregate
    // `app apply` the manifest's own `add db` has not been written yet, and a
    // recipe that reaches past the projection refuses a manifest that is
    // perfectly well ordered.
    if !slice.project().has_jdbc() {
        return Err(format!(
            "http-workflow {name} needs PostgreSQL/JDBC for its durable frontier.\n       fix: run `jails add db` first."
        ).into());
    }
    // Through the project, so a fetcher this same manifest declares two rows
    // above counts. `Path::is_file` answers about disk, and in one transition
    // nothing has been written yet.
    if !slice
        .project()
        .has_type(clients, &format!("{fetcher}Fetcher"))
    {
        return Err(format!(
            "http-workflow {name} cannot find {fetcher}Fetcher.java.\n       fix: generate fetcher {fetcher} first."
        ).into());
    }
    let table = crate::sql::snake_case(name);
    let property = table.replace('_', "-");
    let main_jobs = crate::generate::main_dir(root, jobs);
    let main_web = crate::generate::main_dir(root, web);
    let test_jobs = crate::generate::test_dir(root, jobs);
    Ok(vec![
        Artifact {
            kind: "scheduling",
            path: main_jobs.join("SchedulingConfig.java"),
            contents: scheduling_config_java(jobs),
        },
        Artifact {
            kind: "bounded HTTP workflow",
            path: main_jobs.join(format!("{name}Workflow.java")),
            contents: crate::template::render(
                crate::template_here!("spring/http_workflow_java.java"),
                &[
                    ("pkg", jobs),
                    ("clients", clients),
                    ("name", name),
                    ("fetcher", fetcher),
                    ("table", table.as_str()),
                    ("property", property.as_str()),
                ],
            ),
        },
        Artifact {
            kind: "bounded HTTP workflow controller",
            path: main_web.join(format!("{name}WorkflowController.java")),
            contents: crate::template::render(
                crate::template_here!("spring/http_workflow_controller_java.java"),
                &[
                    ("web", web),
                    ("pkg", jobs),
                    ("name", name),
                    ("property", property.as_str()),
                ],
            ),
        },
        Artifact {
            kind: "bounded HTTP workflow integration test",
            path: test_jobs.join(format!("{name}WorkflowIT.java")),
            contents: crate::template::render(
                crate::template_here!("spring/http_workflow_it_java.java"),
                &[
                    ("pkg", jobs),
                    ("clients", clients),
                    ("name", name),
                    ("fetcher", fetcher),
                    ("table", table.as_str()),
                    ("property", property.as_str()),
                ],
            ),
        },
        Artifact {
            kind: "bounded HTTP workflow migration",
            path: crate::generate::migration_file(
                slice.project(),
                &format!("create_{table}_workflow"),
            )?,
            contents: http_workflow_migration(&table),
        },
    ])
}

/// **The DDL lives in `templates/sql/http_workflow.sql`, and both engines read it.**
/// The canonical emitter renders the same table, and two copies of a
/// schema drift on exactly the column nobody re-reads -- a `select`
/// naming one the `create table` never had, found by `flyway migrate` in
/// a project that was working yesterday. `CLAUDE.md` states the rule for
/// the project files; it is the same rule.
fn http_workflow_migration(table: &str) -> String {
    crate::template_here!("sql/http_workflow.sql").replace("{{table}}", table)
}
