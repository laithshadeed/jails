//! Framework-shaped components, lowered to Java.
//!
//! **Why this is not `emit_unit.rs`.** A [`jails_model::SourceUnit`] is one
//! plain Java type: a package, a name, maybe variants and an endpoint.
//! `linker::component` projects the eight unit-shaped component kinds onto
//! that and returns `None` for the rest, and the rest are not shaped like it —
//! `client` is an interface *and* a registration bean *and* a test *and* a
//! build dependency *and* three properties, from one declaration. Projecting
//! it through `SourceUnit` would drop the last two before the emitter ever saw
//! them.
//!
//! **Twelve of the fourteen kinds are [`Recipe`] rows** -- files, dependencies,
//! properties and the typed values their templates spell, rendered by the one
//! loop in [`crate::recipe`]. `http_sink` and `durable_job` stay functions:
//! each reaches across the model to another node -- the outbox command a
//! sink delivers from, the entity a durable job produces -- and renders a
//! sample argument list from *that* node's fields, which is structure the
//! recipe's closed key vocabulary does not spell.
//!
//! **The Java bodies are the templates under `templates/spring/`**, pulled in
//! with `include_str!`: two copies of a template drift on exactly the details
//! nobody re-reads, and neither drift is visible where anyone looks.

use crate::CompileError;
use crate::recipe::{
    BootCondition, DependencySpec, Fragment, Import, JavaFile, Naming, Need, Node, Placement,
    PropertySpec, Recipe, SourceSet, Want,
};
use jails_contracts::{
    BuildDependency, FileKind, FileMode, ProjectPath, PropertyEntry, RenderedFile, RenderedTree,
};
use jails_model::{AppModel, Component, ComponentKind, Package, SettingTarget, StableId};
pub(crate) use node::Key;
use std::collections::BTreeSet;

mod cli;
mod durable_job;
mod handler;
pub(crate) mod http_sink;
mod http_workflow;
mod idempotency;
mod job;
mod node;
mod presence;

const MAIN_ROOT: &str = crate::recipe::MAIN_ROOT;
const TEST_ROOT: &str = crate::recipe::TEST_ROOT;

const NO_NEEDS: &[Need] = &[];
const NO_DEPENDENCIES: &[DependencySpec] = &[];
const NO_PROPERTIES: &[PropertySpec] = &[];

/// A row with the fields every component recipe leaves empty.
const fn recipe(
    keys: &'static [Key],
    requires: &'static [Need],
    files: &'static [JavaFile<Component>],
    dependencies: &'static [DependencySpec],
    properties: &'static [PropertySpec],
) -> Recipe<Component> {
    Recipe {
        substitutions: &[],
        keys,
        fragments: &[],
        requires,
        files,
        files_when: BootCondition::Any,
        resources: &[],
        dependencies,
        properties,
        compose_services: &[],
        build_features: &[],
        default_package: base_package,
        pass: "components",
        minimum_boot: None,
    }
}

/// A main-source file placed in one layer.
const fn main(
    role: &'static str,
    template: crate::Template,
    layer: Package,
    class: Naming<Component>,
    ejectable: bool,
) -> JavaFile<Component> {
    JavaFile {
        role,
        template,
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Layer(layer),
        ejectable,
        class,
        template_class: Naming::Fixed(""),
    }
}

/// A test-source file placed in one layer.
const fn test(
    role: &'static str,
    template: crate::Template,
    layer: Package,
    class: Naming<Component>,
) -> JavaFile<Component> {
    JavaFile {
        source_set: SourceSet::Test,
        ..main(role, template, layer, class, true)
    }
}

fn base_package(model: &AppModel, _: &Component) -> String {
    model.project.package_for(Package::Base)
}

/// One `application.properties` entry in the main target.
const fn property(key: &'static str, value: &'static str) -> PropertySpec {
    PropertySpec {
        key,
        value,
        target: SettingTarget::Main,
        boot: BootCondition::Any,
    }
}

/// `component fetcher <Name>`: bounded, SSRF-safe outbound bytes.
///
/// A port, a safe adapter and an adversarial test. The adapter is the whole
/// point: fetching a URL a caller supplies is the one outbound call that can
/// be aimed at the host it runs on, so the generated implementation pins a
/// connect and response timeout, a maximum response size, a redirect limit and
/// an allowed content-type list, and refuses anything else. The test is
/// adversarial rather than happy-path for the same reason -- a fetcher that
/// passes "it downloads a page" tells you nothing about the case it exists for.
///
/// **Its settings are `@Value` defaults in the adapter, not properties.**
/// Every one has a working default, so a project that writes none still gets
/// the bounds; a property file entry would be a value a reader has to keep in
/// step with a class that already states it. That is a deliberate difference
/// from `client`, whose base URL has no default that could work.
///
/// Micrometer for the fetch counters the adapter records, and Apache
/// HttpClient because the JDK client follows a redirect to a private address
/// without asking -- the bound the adapter needs is only expressible on a
/// client that lets it inspect each hop.
const FETCHER: Recipe<Component> = recipe(
    &[Key::Property],
    NO_NEEDS,
    &[
        // The port is managed ABI: every generated caller names it.
        main(
            "port",
            crate::template!("spring/fetcher_port_java.java"),
            Package::Clients,
            Naming::Suffix("Fetcher"),
            false,
        ),
        main(
            "adapter",
            crate::template!("spring/safe_fetcher_java.java"),
            Package::Clients,
            Naming::Wrap("Safe", "Fetcher"),
            true,
        ),
        test(
            "test",
            crate::template!("spring/safe_fetcher_test_java.java"),
            Package::Clients,
            Naming::Wrap("Safe", "FetcherTest"),
        ),
    ],
    &[
        DependencySpec::managed("org.apache.httpcomponents.client5", "httpclient5"),
        DependencySpec::managed("org.springframework.boot", "spring-boot-starter-actuator"),
    ],
    NO_PROPERTIES,
);

/// `component auth <Name>`: the encoder, the decoder and the tokens they
/// agree on.
///
/// Checked against the model rather than the build file: in one transition
/// the capability this same model declares has not been spliced into the pom
/// yet.
const AUTH: Recipe<Component> = recipe(
    &[Key::Issuer],
    &[Need {
        want: Want::Capability("security"),
        why: "needs Spring Security: the encoder, the decoder and the filter chain that reads the token are one story",
    }],
    &[
        main(
            "config",
            crate::template!("spring/auth_config_java.java"),
            Package::Base,
            Naming::Suffix("TokenConfig"),
            true,
        ),
        main(
            "tokens",
            crate::template!("spring/auth_tokens_java.java"),
            Package::Base,
            Naming::Suffix("Tokens"),
            true,
        ),
        test(
            "test",
            crate::template!("spring/auth_tokens_test_java.java"),
            Package::Base,
            Naming::Suffix("TokensTest"),
        ),
    ],
    NO_DEPENDENCIES,
    &[property(
        "app.auth.secret",
        "replace-me-with-a-32-byte-secret",
    )],
);

/// `component client <Name>`: a declarative HTTP interface and the bean that
/// proxies it.
///
/// The base URL has no default that could work, so it is a property with a
/// visibly invalid value rather than a `@Value` default.
const CLIENT: Recipe<Component> = recipe(
    &[Key::Group, Key::Path("/{{property}}")],
    NO_NEEDS,
    &[
        main(
            "interface",
            crate::template!("spring/client_interface_java.java"),
            Package::Clients,
            Naming::Suffix("Client"),
            false,
        ),
        main(
            "config",
            crate::template!("spring/client_config_java.java"),
            Package::Clients,
            Naming::Suffix("ClientConfig"),
            true,
        ),
        test(
            "test",
            crate::template!("spring/client_test_java.java"),
            Package::Clients,
            Naming::Suffix("ClientTest"),
        ),
    ],
    &[DependencySpec::managed(
        "org.springframework.boot",
        "spring-boot-starter-restclient",
    )],
    &[
        property(
            "spring.http.serviceclient.{{group}}.base-url",
            "https://example.invalid",
        ),
        property("spring.http.serviceclient.{{group}}.connect-timeout", "2s"),
        property("spring.http.serviceclient.{{group}}.read-timeout", "5s"),
    ],
);

/// `component job <Name>`: a scheduled bean and its test.
///
/// The `SchedulingConfig` it needs belongs to every job in the model rather
/// than to one, so it is emitted once from [`emit`] rather than by
/// each row -- a managed tree refuses two units writing the same path.
const JOB: Recipe<Component> = recipe(
    &[Key::Property],
    NO_NEEDS,
    &[
        main(
            "job",
            crate::template!("spring/job_java.java"),
            Package::Jobs,
            Naming::Suffix("Job"),
            true,
        ),
        test(
            "test",
            crate::template!("spring/job_test_java.java"),
            Package::Jobs,
            Naming::Suffix("JobTest"),
        ),
    ],
    NO_DEPENDENCIES,
    NO_PROPERTIES,
);

/// `component handler <Name>`: HTTP with no framework in it.
///
/// The JDK's own `HttpHandler`, so a project with no Spring on the classpath
/// still has a way to serve a resource. Thin by construction: it binds,
/// routes, and maps outcomes to status codes, and holds no rules -- which is
/// what lets the same service be driven from a CLI.
///
/// **`ApiError` belongs to every handler in the model**, so it is emitted once
/// from [`emit`] rather than by each one -- the same rule
/// `SchedulingConfig` follows, and for the same reason.
const HANDLER: Recipe<Component> = recipe(
    &[Key::Path("/{{property}}")],
    NO_NEEDS,
    &[
        JavaFile {
            // Skipped when the two packages coincide: importing a sibling is a
            // compile error, which is what `--package ''` produces.
            imports: &[Import::From(Package::Domain, "ApiError")],
            ..main(
                "handler",
                crate::template!("spring/handler_java.java"),
                Package::Api,
                Naming::Suffix("Handler"),
                true,
            )
        },
        test(
            "test",
            crate::template!("spring/handler_test_java.java"),
            Package::Api,
            Naming::Suffix("HandlerTest"),
        ),
    ],
    NO_DEPENDENCIES,
    NO_PROPERTIES,
);

/// `component socket <Name>`: a WebSocket endpoint.
///
/// A handler, its registration and a test. Both main files sit in the `web`
/// layer because a socket endpoint is an inbound HTTP surface, and the
/// registration is separate from the handler for the reason every Spring
/// registration here is: the class the reader edits should not also be the
/// class that decides where it is mounted. Spring's WebSocket support is not
/// in the web starter.
const SOCKET: Recipe<Component> = recipe(
    &[Key::Path("/ws/{{property}}")],
    NO_NEEDS,
    &[
        main(
            "handler",
            crate::template!("spring/socket_handler_java.java"),
            Package::Web,
            Naming::Suffix("SocketHandler"),
            true,
        ),
        main(
            "config",
            crate::template!("spring/socket_config_java.java"),
            Package::Web,
            Naming::Suffix("SocketConfig"),
            true,
        ),
        test(
            "test",
            crate::template!("spring/socket_handler_test_java.java"),
            Package::Web,
            Naming::Suffix("SocketHandlerTest"),
        ),
    ],
    &[DependencySpec::managed(
        "org.springframework.boot",
        "spring-boot-starter-websocket",
    )],
    NO_PROPERTIES,
);

/// `component webhook <Name>`: an inbound call somebody else makes.
///
/// A verifier, a controller and a test. The split is the whole design: the
/// verifier is a plain class with no framework in it, so the signature check
/// can be tested without starting a context, and the controller is the thin
/// layer that reads the two headers and hands the raw body over. The verifier
/// is framework-free, so it goes in the base package; the controller is an
/// inbound HTTP surface and goes in `web`.
///
/// **The shared secret is a property with no default**, derived from the
/// declaration rather than asked for -- `stripe` becomes `app.stripe.secret`.
/// Derived so `destroy` can find it and two projects spell it the same way,
/// and without a default because a webhook whose secret silently defaults is a
/// webhook anybody can call.
///
/// It is *declared* all the same, with a value that is visibly not a secret.
/// `@Value("${app.stripe.secret}")` with nothing declaring the key does not
/// fail safe -- it fails `contextLoads`, so the project does not start at all
/// and the reader is told about a placeholder rather than about a webhook. A
/// line in `application.properties` reading `replace-me` is the same warning
/// delivered where they can act on it.
const WEBHOOK: Recipe<Component> = recipe(
    &[
        Key::Property,
        Key::Path("{{property}}"),
        Key::TimestampHeader,
        Key::SignatureHeader,
    ],
    NO_NEEDS,
    &[
        main(
            "verifier",
            crate::template!("spring/webhook_verifier_java.java"),
            Package::Base,
            Naming::Suffix("Verifier"),
            true,
        ),
        JavaFile {
            imports: &[Import::Role("verifier")],
            ..main(
                "controller",
                crate::template!("spring/webhook_controller_java.java"),
                Package::Web,
                Naming::Suffix("WebhookController"),
                true,
            )
        },
        test(
            "test",
            crate::template!("spring/webhook_verifier_test_java.java"),
            Package::Base,
            Naming::Suffix("VerifierTest"),
        ),
    ],
    NO_DEPENDENCIES,
    &[property(
        "app.{{property}}.secret",
        "replace-me-with-the-providers-signing-secret",
    )],
);

/// `component command <Name>`: one CLI subcommand.
///
/// Plain Java with no framework in it, which is why it works in a `new-cli`
/// project as well as a Spring one.
///
/// **`run` returns an exit code instead of calling `System.exit`**, and takes
/// its output streams as arguments instead of reaching for `System.out`. Both
/// exist so a test can drive the whole command in-process and assert on what
/// it printed, and `main` stays the only place that exits.
///
/// The registration into the project's dispatcher is a reader-file patch, not
/// something emitted here -- see `DocumentIntent::EnsureCommandRegistration`.
/// Hand-pasting a dispatch line after every `g command` is exactly the
/// plumbing this tool exists to remove.
const COMMAND: Recipe<Component> = recipe(
    &[Key::Word],
    NO_NEEDS,
    &[
        main(
            "command",
            crate::template!("spring/command_java.java"),
            Package::Cli,
            Naming::Suffix("Command"),
            true,
        ),
        test(
            "test",
            crate::template!("generate/command_test.java"),
            Package::Cli,
            Naming::Suffix("CommandTest"),
        ),
    ],
    NO_DEPENDENCIES,
    NO_PROPERTIES,
);

/// `component cli <Name>`: a dispatcher for the commands this project has.
///
/// Plain Java, like `command`, and found by the same shape: a registry of
/// `Command` plus a `return commands;` anchor. That shape is what lets
/// `g command` register itself into either this or the `App.java` a `new-cli`
/// project already has, without either knowing about the other. The registry
/// is the one structural fragment: one `commands.put(...)` per command that
/// named this dispatcher, rendered by [`cli::registrations`].
const CLI: Recipe<Component> = Recipe {
    fragments: &[Fragment::Rendered {
        key: "registrations",
        render: cli::registrations,
    }],
    ..recipe(
        &[Key::Program],
        NO_NEEDS,
        &[
            JavaFile {
                template_class: Naming::Suffix("Cli"),
                ..main(
                    "cli",
                    crate::template!("spring/cli_java.java"),
                    Package::Cli,
                    Naming::Suffix("Cli"),
                    true,
                )
            },
            JavaFile {
                template_class: Naming::Suffix("Cli"),
                ..test(
                    "test",
                    crate::template!("spring/cli_test_java.java"),
                    Package::Cli,
                    Naming::Suffix("CliTest"),
                )
            },
        ],
        NO_DEPENDENCIES,
        NO_PROPERTIES,
    )
};

/// `component presence <Name>`: who is here, across every node.
///
/// **PostgreSQL is a precondition, not a preference.** Presence held in one
/// process's memory is correct on one node and wrong on two, with nothing to
/// say which -- the application works, and the answer is silently partial. So
/// the store is a table, and a member seen by *any* node is present.
///
/// A departure is a delete and there is no `left_at`: a row exists only while
/// somebody is there, which is what makes `present` a single predicate rather
/// than a join against a history. The sweep that removes rows for a crashed
/// node is scheduled, which is why this shares `SchedulingConfig` with `job`.
const PRESENCE: Recipe<Component> = recipe(
    &[Key::Property, Key::Table("_presence")],
    &[Need {
        want: Want::Database,
        why: "needs PostgreSQL/JDBC: presence held in one process's memory is correct on one node and wrong on two, with nothing to say which",
    }],
    &[
        // The port is managed ABI: the store and every caller name it.
        main(
            "port",
            crate::template!("spring/presence_port_java.java"),
            Package::Application,
            Naming::Suffix("Presence"),
            false,
        ),
        JavaFile {
            imports: &[Import::Role("port")],
            ..main(
                "store",
                crate::template!("spring/presence_store_java.java"),
                Package::AdaptersJdbc,
                Naming::Wrap("Jdbc", "Presence"),
                true,
            )
        },
        // The container config is a fact about the *model*, not a file on
        // disk, and a different question from whether SQL is reachable: the
        // guard above passes for a project carrying its own JDBC starter, and
        // that project has no `TestcontainersConfig` for this test to import.
        JavaFile {
            imports: &[Import::ContainerSupport],
            source_set: SourceSet::IntegrationTest,
            ..main(
                "it",
                crate::template!("spring/presence_it_java.java"),
                Package::AdaptersJdbc,
                Naming::Wrap("Jdbc", "PresenceIT"),
                true,
            )
        },
    ],
    NO_DEPENDENCIES,
    NO_PROPERTIES,
);

/// `component idempotency <Name>`: a retained result, not just a unique row.
///
/// **The distinction is easy to lose.** A `@unique` column already gives one
/// row per key. What it withholds is the *result*: a retry finds the row,
/// fails the insert, and gets a 409 -- telling a caller that never saw the
/// first response that the work happened, while still withholding what
/// happened. So this generates a receipt record, a store port, its JDBC
/// adapter, a guard and a test, and the guard has four outcomes: run, replay,
/// refuse a reused key, or tell an in-flight retry to come back.
///
/// Domain-blind by construction: the scope is a string the caller picks, the
/// request is bytes the caller canonicalises, and the stored result is opaque.
///
/// **The claim is one `insert ... on conflict do nothing returning`**, because
/// select-then-insert reopens the race it exists to close.
///
/// Receipts that do not outlive a restart are not receipts, so the database
/// is a precondition -- checked against the model rather than the build file,
/// for the reason `auth` is.
const IDEMPOTENCY: Recipe<Component> = recipe(
    &[Key::Table("_receipts")],
    &[Need {
        want: Want::Database,
        why: "needs PostgreSQL/JDBC to keep receipts across restarts",
    }],
    &[
        // The receipt is managed ABI: the port, the store and the guard all
        // name it, and every file below imports it and the port by role.
        main(
            "record",
            crate::template!("spring/idempotency_record_java.java"),
            Package::Domain,
            Naming::Suffix("Receipt"),
            false,
        ),
        JavaFile {
            imports: &[Import::Role("record"), Import::Role("port")],
            ..main(
                "port",
                crate::template!("spring/idempotency_port_java.java"),
                Package::Application,
                Naming::Suffix("Receipts"),
                false,
            )
        },
        JavaFile {
            imports: &[Import::Role("record"), Import::Role("port")],
            ..main(
                "store",
                crate::template!("spring/idempotency_store_java.java"),
                Package::AdaptersJdbc,
                Naming::Wrap("Jdbc", "Receipts"),
                true,
            )
        },
        JavaFile {
            imports: &[Import::Role("record"), Import::Role("port")],
            ..main(
                "guard",
                crate::template!("spring/idempotency_guard_java.java"),
                Package::Service,
                Naming::Suffix("Guard"),
                true,
            )
        },
        JavaFile {
            imports: &[Import::Role("record"), Import::Role("port")],
            ..test(
                "test",
                crate::template!("spring/idempotency_test_java.java"),
                Package::Service,
                Naming::Suffix("GuardTest"),
            )
        },
    ],
    NO_DEPENDENCIES,
    NO_PROPERTIES,
);

/// `component http-workflow <Name>`: a durable traversal over a bounded
/// fetcher.
///
/// Four artifacts and no more: the workflow, an HTTP control plane to start
/// and cancel runs, an integration test that drives a real traversal, and the
/// three tables. The scheduling config is shared, so it comes from
/// [`job::scheduling`] like every other scheduled bean's. Micrometer for the
/// traversal counters; the JDBC half comes from `storage`.
const HTTP_WORKFLOW: Recipe<Component> = recipe(
    &[
        Key::Fetcher,
        Key::Property,
        Key::Table(""),
        Key::Layer("clients", Package::Clients),
        Key::Layer("jobs", Package::Jobs),
    ],
    &[Need {
        want: Want::Database,
        why: "keeps its whole frontier in PostgreSQL",
    }],
    &[
        main(
            "workflow",
            crate::template!("spring/http_workflow_java.java"),
            Package::Jobs,
            Naming::Suffix("Workflow"),
            true,
        ),
        main(
            "controller",
            crate::template!("spring/http_workflow_controller_java.java"),
            Package::Web,
            Naming::Suffix("WorkflowController"),
            true,
        ),
        JavaFile {
            source_set: SourceSet::IntegrationTest,
            ..main(
                "test",
                crate::template!("spring/http_workflow_it_java.java"),
                Package::Jobs,
                Naming::Suffix("WorkflowIT"),
                true,
            )
        },
    ],
    &[DependencySpec::managed(
        "org.springframework.boot",
        "spring-boot-starter-actuator",
    )],
    NO_PROPERTIES,
);

/// The recipe registry for components: one row per kind that is data.
///
/// `http_sink` and `durable_job` are the two kinds that stay functions, and
/// the eight unit-shaped kinds render through `emit_unit`.
pub(crate) fn recipe_for(kind: ComponentKind) -> Option<&'static Recipe<Component>> {
    match kind {
        ComponentKind::Fetcher => Some(&FETCHER),
        ComponentKind::Auth => Some(&AUTH),
        ComponentKind::Client => Some(&CLIENT),
        ComponentKind::Job => Some(&JOB),
        ComponentKind::Handler => Some(&HANDLER),
        ComponentKind::Socket => Some(&SOCKET),
        ComponentKind::Webhook => Some(&WEBHOOK),
        ComponentKind::Command => Some(&COMMAND),
        ComponentKind::Cli => Some(&CLI),
        ComponentKind::Presence => Some(&PRESENCE),
        ComponentKind::Idempotency => Some(&IDEMPOTENCY),
        ComponentKind::HttpWorkflow => Some(&HTTP_WORKFLOW),
        ComponentKind::HttpSink
        | ComponentKind::DurableJob
        | ComponentKind::Class
        | ComponentKind::Interface
        | ComponentKind::Service
        | ComponentKind::Controller
        | ComponentKind::Sealed
        | ComponentKind::Strategy
        | ComponentKind::Cases
        | ComponentKind::Test
        | ComponentKind::IntegrationTest => None,
    }
}

pub(crate) fn emit(
    model: &AppModel,
    output: &mut RenderedTree,
    snapshot: &jails_contracts::WorkspaceSnapshot,
) -> Result<(), CompileError> {
    let templates = &snapshot.template_overrides;
    for component in model.components.values() {
        if let Some(recipe) = recipe_for(component.kind) {
            crate::recipe::render(model, component, recipe, snapshot, output)?;
            continue;
        }
        let files = match component.kind {
            ComponentKind::HttpSink => http_sink::files(model, component, templates)?,
            ComponentKind::DurableJob => durable_job::files(model, component, templates)?,
            _ => continue,
        };
        for file in files {
            output
                .insert(file.path, file.file)
                .map_err(CompileError::new)?;
        }
    }
    // Emitted after the loop and once: `SchedulingConfig` belongs to every job
    // in the model rather than to one, and a managed tree refuses two units
    // writing the same path.
    if let Some(shared) = job::scheduling(model, templates)? {
        output
            .insert(shared.path, shared.file)
            .map_err(CompileError::new)?;
    }
    for shared in handler::envelope(model, templates)? {
        output
            .insert(shared.path, shared.file)
            .map_err(CompileError::new)?;
    }
    Ok(())
}

/// The forward migrations this model's components need.
///
/// Takes the accepted model because a migration is an irreproducible
/// operation: what matters is which components are *new*, not which exist.
pub(crate) fn migrations(
    accepted: Option<&AppModel>,
    next: &AppModel,
) -> Vec<jails_contracts::RenderedMigration> {
    let mut migrations = idempotency::migrations(accepted, next);
    migrations.extend(presence::migrations(accepted, next));
    migrations.extend(http_workflow::migrations(accepted, next));
    migrations.extend(durable_job::migrations(accepted, next));
    migrations
}

/// The entry point a `cli` component may claim, if jails may claim one.
///
/// `model` is the *intended* model, not `snapshot.model.model`: the command
/// that declares a `cli` is the one whose pre-patch model has none, so reading
/// the snapshot's means `jails g cli Admin` never retargets `<mainClass>` and
/// some later, unrelated command does it instead. The snapshot is still what
/// says whether jails *may* claim the entry point -- that answer is about the
/// pom on disk.
pub(crate) fn entry_point(
    snapshot: &jails_contracts::WorkspaceSnapshot,
    model: &AppModel,
) -> Option<String> {
    cli::entry_point(snapshot, model)
}

/// The build dependencies this model's components need.
///
/// Every one is versionless, which is correct under
/// `spring-boot-starter-parent` and required rather than merely tidy: a
/// `<version>` invented here would pin a starter against the reader's Boot.
pub(crate) fn dependencies(model: &AppModel) -> Vec<BuildDependency> {
    let mut dependencies = model
        .components
        .values()
        .filter_map(|component| recipe_for(component.kind))
        .flat_map(|recipe| crate::recipe::dependencies(recipe, None, true))
        .collect::<BTreeSet<_>>();
    if model
        .components
        .values()
        .any(|component| component.kind == ComponentKind::HttpSink)
    {
        dependencies.extend(http_sink::DEPENDENCIES.iter().map(|(group, artifact)| {
            BuildDependency {
                group: (*group).to_string(),
                artifact: (*artifact).to_string(),
                version: None,
                scope: jails_model::DependencyScope::Compile,
                optional: false,
            }
        }));
    }
    dependencies.into_iter().collect()
}

/// The `application.properties` entries this model's components need.
pub(crate) fn properties(
    model: &AppModel,
    target: SettingTarget,
) -> Result<Vec<PropertyEntry>, CompileError> {
    if target != SettingTarget::Main {
        return Ok(Vec::new());
    }
    let mut properties = Vec::new();
    for component in model.components.values() {
        if let Some(recipe) = recipe_for(component.kind) {
            properties.extend(crate::recipe::properties(model, component, recipe, target)?);
        } else if component.kind == ComponentKind::HttpSink {
            properties.extend(http_sink::properties(model, component)?);
        }
    }
    Ok(properties)
}

/// One rendered file and where it goes.
struct Emitted {
    path: ProjectPath,
    file: RenderedFile,
}

/// A managed Java file for one component, identified by that component and a
/// suffix rather than by its path -- the same identity the recipe loop gives
/// its files, for the two kinds that are still functions.
fn java(
    component: &Component,
    suffix: &str,
    package: &str,
    type_name: &str,
    test: bool,
    ejectable: bool,
    unit: impl Into<crate::emit_java::JavaUnit>,
) -> Result<Emitted, CompileError> {
    let artifact = format!("art_{}_{}", component.id.as_str(), suffix);
    let root = if test { TEST_ROOT } else { MAIN_ROOT };
    let path = ProjectPath::parse(format!(
        "{root}/{}/{type_name}.java",
        package.replace('.', "/")
    ))
    .map_err(CompileError::new)?;
    Ok(Emitted {
        path,
        file: RenderedFile {
            bytes: unit.into().render(&artifact).into_bytes(),
            kind: if test {
                FileKind::JavaTest
            } else {
                FileKind::JavaMain
            },
            mode: FileMode::Regular,
            provenance: component.provenance(artifact, ejectable, "components"),
        },
    })
}

/// Where a component's Java goes.
fn package(model: &AppModel, package: Package) -> String {
    model.project.package_for(package)
}

/// Whether SQL is reachable from this project, however it got there.
fn has_database(model: &AppModel) -> bool {
    crate::recipe::has_database(model)
}
