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
pub fn client_files(slice: &Slice, name: &str) -> Vec<Artifact> {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.placed(Layer::Clients);
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    let group = crate::sql::snake_case(name).replace('_', "-");
    vec![
        Artifact {
            kind: "http client",
            path: main.join(format!("{name}Client.java")),
            contents: client_interface_java(pkg, name),
        },
        Artifact {
            kind: "http client registration",
            path: main.join("HttpClientsConfig.java"),
            contents: client_config_java(pkg, &group),
        },
        Artifact {
            kind: "http client test",
            path: test.join(format!("{name}ClientTest.java")),
            contents: client_test_java(pkg, name, &group),
        },
    ]
}

fn client_interface_java(pkg: &str, name: &str) -> String {
    let path = format!("/{}", crate::sql::table_name(name).replace('_', "-"));
    crate::template::render(
        crate::template_here!("spring/client_interface_java.java"),
        &[("pkg", pkg), ("name", name), ("path", &*path)],
    )
}

fn client_config_java(pkg: &str, group: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/client_config_java.java"),
        &[("pkg", pkg), ("group", group)],
    )
}

fn client_test_java(pkg: &str, name: &str, group: &str) -> String {
    let path = format!("/{}", crate::sql::table_name(name).replace('_', "-"));
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

pub fn fetcher_files(slice: &Slice, name: &str) -> Vec<Artifact> {
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

pub fn http_workflow_files(
    slice: &Slice,
    name: &str,
    fetcher: &str,
) -> jails_support::Result<Vec<Artifact>> {
    let root: &Path = slice.project().root();
    let jobs: &str = &slice.placed(Layer::Jobs);
    let clients: &str = &slice.owned(Layer::Clients);
    let web: &str = &slice.owned(Layer::Web);
    let pom = std::fs::read_to_string(root.join("pom.xml"))
        .map_err(|e| format!("failed to read pom.xml: {e}"))?;
    if !crate::pom::has_dependency(&pom, "org.springframework.boot", "spring-boot-starter-jdbc") {
        return Err(format!(
            "http-workflow {name} needs PostgreSQL/JDBC for its durable frontier.\n       fix: run `jails add db` first."
        ));
    }
    let fetcher_port =
        crate::generate::main_dir(root, clients).join(format!("{fetcher}Fetcher.java"));
    if !fetcher_port.is_file() {
        return Err(format!(
            "http-workflow {name} cannot find {fetcher}Fetcher.java.\n       fix: generate fetcher {fetcher} first."
        ));
    }
    let table = crate::sql::snake_case(name);
    let property = table.replace('_', "-");
    let migration_dir = root.join("src/main/resources/db/migration");
    let version = crate::generate::next_migration_version(&migration_dir)?;
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
            path: migration_dir.join(format!("V{version:03}__create_{table}_workflow.sql")),
            contents: http_workflow_migration(&table),
        },
    ])
}

fn http_workflow_migration(table: &str) -> String {
    format!(
        "create table {table}_runs (\n\
           id uuid primary key,\n\
           seed_url text not null,\n\
           origin_scheme text not null,\n\
           origin_host text not null,\n\
           origin_port integer not null,\n\
           status text not null check (status in ('QUEUED','RUNNING','SUCCEEDED','FAILED','CANCELLED')),\n\
           max_pages integer not null check (max_pages > 0),\n\
           max_depth integer not null check (max_depth >= 0),\n\
           pages_visited integer not null default 0 check (pages_visited >= 0),\n\
           robots_rules text,\n\
           cancel_requested boolean not null default false,\n\
           last_error text,\n\
           created_at timestamptz not null,\n\
           started_at timestamptz,\n\
           finished_at timestamptz\n\
         );\n\n\
         create table {table}_frontier (\n\
           run_id uuid not null references {table}_runs(id) on delete cascade,\n\
           url text not null,\n\
           depth integer not null check (depth >= -1),\n\
           kind text not null check (kind in ('POLICY','PAGE')),\n\
           state text not null check (state in ('PENDING','RUNNING','SUCCEEDED','FAILED','CANCELLED')),\n\
           attempts integer not null default 0 check (attempts >= 0),\n\
           max_attempts integer not null check (max_attempts > 0),\n\
           next_attempt_at timestamptz not null,\n\
           lease_until timestamptz,\n\
           last_error text,\n\
           primary key (run_id, url)\n\
         );\n\n\
         create index {table}_frontier_runnable_idx\n\
           on {table}_frontier (state, next_attempt_at)\n\
           where state in ('PENDING','RUNNING');\n\n\
         create table {table}_pages (\n\
           run_id uuid not null references {table}_runs(id) on delete cascade,\n\
           url text not null,\n\
           depth integer not null check (depth >= 0),\n\
           status_code integer not null,\n\
           content_type text not null,\n\
           discovered_at timestamptz not null,\n\
           primary key (run_id, url)\n\
         );\n"
    )
}
