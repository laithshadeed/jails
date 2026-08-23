//! Spring-only capabilities: the slices that only make sense inside a Spring
//! Boot application.
//!
//! These live apart from `add.rs` for two reasons. `add.rs` is already the
//! biggest file in the project, and everything here shares one precondition
//! -- a Spring Boot parent -- which is checked in one place rather than
//! re-derived per capability.
//!
//! Every template here was written against the sources under `deps/`
//! (Spring Boot 4.x, Spring Framework 7.x), not from memory. Where an API
//! moved or was replaced, the comment says which one and why, because the
//! failure mode for generated code is silent: it compiles against the
//! version you had and breaks on the version the reader has.

use std::path::Path;

use crate::model::{Artifact, Change, Layer, Slice};
use jails_support::Result;

mod auth;
mod containers;
mod dto;
mod durable;
mod http;
mod mail;
mod messaging;
mod outbox;
mod query;
mod schema;
mod search;
mod security;
mod sse;
mod transition;
mod webhook;
mod workflow;
pub use auth::*;
pub use containers::*;
pub use dto::*;
pub use durable::*;
pub use http::*;
pub use mail::*;
pub use messaging::*;
pub use outbox::*;
pub use query::*;
pub use schema::*;
pub use search::*;
pub use security::*;
pub use sse::*;
pub use transition::*;
pub use webhook::*;
pub use workflow::*;
// Production code here reaches the project through `Slice`; only the renderer
// fixtures build one directly.
#[cfg(test)]
use crate::model::Project;
use crate::pom::{Dependency, Flavor};

fn artifact(path: std::path::PathBuf, contents: String) -> Artifact {
    Artifact::rendered(path, contents)
}

pub const VALIDATION_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-validation",
    version: None,
    scope: None,
    optional: false,
};

/// The annotations themselves, for a project with no Spring Boot BOM.
///
/// `g scaffold` and `g dto` emit `jakarta.validation.constraints.*` and
/// `@Valid`, and on Spring the starter is how you get them *plus* an
/// implementation. On plain Maven the starter is two mistakes at once: it is
/// versionless, which is a pom Maven refuses to read (plan.md §8.1), and it
/// drags Boot into a project that deliberately has none. The API jar is the
/// artifact the generated code actually imports.
pub const JAKARTA_VALIDATION_API: Dependency = Dependency {
    group_id: "jakarta.validation",
    artifact_id: "jakarta.validation-api",
    version: Some("3.1.1"),
    scope: None,
    optional: false,
};

/// Whichever of the two the project can actually resolve.
pub fn validation_dependency(flavor: crate::pom::Flavor) -> &'static Dependency {
    match flavor {
        crate::pom::Flavor::SpringBoot => &VALIDATION_STARTER,
        crate::pom::Flavor::PlainMaven => &JAKARTA_VALIDATION_API,
    }
}

pub const ACTUATOR_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-actuator",
    version: None,
    scope: None,
    optional: false,
};

pub const CACHE_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-cache",
    version: None,
    scope: None,
    optional: false,
};

/// What actually applies `spring.http.serviceclient.<group>.*` to a
/// declarative client.
///
/// Easy to miss, and the failure is confusing rather than loud: without this
/// module `@ImportHttpServices` still builds the proxies (that part is
/// Framework, not Boot), but nothing binds the group's base URL, so the first
/// call fails with "URI with undefined scheme" -- a message that says nothing
/// about a missing dependency. `spring-boot-starter-webmvc` does not bring it;
/// serving HTTP and calling it are separate concerns.
pub const RESTCLIENT_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-restclient",
    version: None,
    scope: None,
    optional: false,
};

/// Apache HttpClient is used by the safe fetcher because its DNS resolver can
/// be pinned to the addresses that passed policy validation. JDK HttpClient
/// does not expose that boundary, leaving a DNS-rebinding window between
/// validation and connection.
pub const APACHE_HTTPCLIENT: Dependency = Dependency {
    group_id: "org.apache.httpcomponents.client5",
    artifact_id: "httpclient5",
    version: None,
    scope: None,
    optional: false,
};

/// Caffeine is the cache Spring Boot picks up automatically when it is on
/// the classpath and nothing else claims the slot. Version managed by the
/// Boot parent so it moves with the platform.
pub const CAFFEINE: Dependency = Dependency {
    group_id: "com.github.ben-manes.caffeine",
    artifact_id: "caffeine",
    version: None,
    scope: None,
    optional: false,
};

/// Refuse politely rather than generating Spring code into a plain Maven
/// project, where it would not compile and the reason would not be obvious.
pub fn require_spring(flavor: Flavor, capability: &str) -> Result<()> {
    match flavor {
        Flavor::SpringBoot => Ok(()),
        Flavor::PlainMaven => Err(format!(
            "`{capability}` is a Spring Boot capability, and this is a plain Maven project.\n       \
             `jails new <name>` creates a Spring project; `jails add http` is the framework-free \
             HTTP option."
        )),
    }
}

// ---------------------------------------------------------------------------
// Shared helpers -- used by more than one kind, so they live above all of them.
// ---------------------------------------------------------------------------

fn usecase_normalized_type(java_type: &str) -> &str {
    match java_type {
        "Integer" => "int",
        "Long" => "long",
        "Double" => "double",
        "Float" => "float",
        "Boolean" => "boolean",
        "Short" => "short",
        "Byte" => "byte",
        "Character" => "char",
        other => other,
    }
}

fn usecase_field_type(field: &crate::generate::Field) -> String {
    if field.optionality == crate::generate::Optionality::Nullable {
        format!("Optional<{}>", field.java_type)
    } else {
        field.java_type.clone()
    }
}

fn java_literal_imports(fields: &[crate::generate::Field], domain: &str) -> Vec<String> {
    let mut imports = fields
        .iter()
        .flat_map(|field| field.imports.iter().map(|import| (*import).to_string()))
        .collect::<Vec<_>>();
    imports.extend(
        fields
            .iter()
            .filter(|field| field.owned)
            .map(|field| format!("{domain}.{}", field.java_type)),
    );
    if fields
        .iter()
        .any(|field| field.optionality == crate::generate::Optionality::Nullable)
    {
        imports.push("java.util.Optional".to_string());
    }
    imports.sort();
    imports.dedup();
    imports
}

fn scope_controller_parts(
    security: &str,
    web: &str,
    fields: &[crate::generate::Field],
    request: &str,
) -> (String, String, String, String, String, String) {
    let scoped = fields
        .iter()
        .filter(|field| field.constraints.scoped)
        .collect::<Vec<_>>();
    if scoped.is_empty() {
        return (
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        );
    }
    let import = format!(
        "{}import org.springframework.security.core.Authentication;\n",
        crate::generate::import_of(web, security, "ScopeAuthorizer")
    );
    let checks = scoped
        .iter()
        .map(|field| {
            format!(
                "        scopeAuthorizer.require(authentication, \"{}\", {request}.{}());",
                field.name, field.name
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    (
        import,
        "    private final ScopeAuthorizer scopeAuthorizer;\n".to_string(),
        ", ScopeAuthorizer scopeAuthorizer".to_string(),
        "        this.scopeAuthorizer = scopeAuthorizer;".to_string(),
        ",\n            Authentication authentication".to_string(),
        checks,
    )
}

// ---------------------------------------------------------------------------
// `add api` -- one place where failures become HTTP responses.
// ---------------------------------------------------------------------------

/// The error-handling slice.
///
/// This is the single largest piece of boilerplate in a Spring web service,
/// and the one most often written slightly differently in every project: a
/// controller that catches its own exceptions, a `Map<String, String>` error
/// body invented per endpoint, validation failures surfacing as a 500
/// because nothing handled them.
///
/// What replaces it is RFC 9457 (`application/problem+json`), which Spring
/// models as `ProblemDetail` and already produces for its own exceptions.
/// The generated advice extends `ResponseEntityExceptionHandler` -- Spring's
/// own base class, so every framework exception keeps its correct status and
/// only the project's own exceptions need mapping.
pub fn api_slice(slice: &Slice) -> Change {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.placed(Layer::Api);
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    Change {
        deps: vec![VALIDATION_STARTER],
        files: vec![
            artifact(main.join("ApiException.java"), api_exception_java(pkg)),
            artifact(
                main.join("ApiExceptionHandler.java"),
                api_exception_handler_java(pkg),
            ),
            artifact(
                test.join("ApiExceptionHandlerTest.java"),
                api_exception_handler_test_java(pkg),
            ),
        ],
        properties: Vec::new(),
        ..Change::default()
    }
}

/// The project's own failures, as a sealed set.
///
/// Sealed rather than an open hierarchy: the handler switches over these, and
/// a `default` branch is what lets a new failure type silently become a 500.
/// With `permits` spelled out, adding one breaks the build at the switch --
/// which is where the decision about its status code belongs.
fn api_exception_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/api_exception_java.java"),
        &[("pkg", pkg)],
    )
}

fn api_exception_handler_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/api_exception_handler_java.java"),
        &[("pkg", pkg)],
    )
}

fn api_exception_handler_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/api_exception_handler_test_java.java"),
        &[("pkg", pkg)],
    )
}

// ---------------------------------------------------------------------------
// `add actuator` -- health, metrics and info, without inventing endpoints.
// ---------------------------------------------------------------------------

/// The actuator exposure list, unioned with whatever is already set.
///
/// Two capabilities own this one key -- `actuator` and `observability` -- and
/// each installs its properties as its own marked block. Properties are
/// last-wins, so whichever was added second would otherwise silently narrow
/// the other: `add observability` then `add actuator` leaves `prometheus`
/// unexposed and the scrape returns 404 with nothing in the logs to say why.
/// Reading the current value and unioning makes the order stop mattering.
fn exposure_include(slice: &Slice, wanted: &[&str]) -> String {
    let root: &Path = slice.project().root();
    let mut names: Vec<String> = Vec::new();
    let path = root.join("src/main/resources/application.properties");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        for line in existing.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("management.endpoints.web.exposure.include=") {
                for name in value.split(',') {
                    let name = name.trim();
                    // `*` is a wildcard, not a name: keep it and stop, rather
                    // than expanding a user's deliberate choice into a list.
                    if !name.is_empty() && !names.iter().any(|n| n == name) {
                        names.push(name.to_string());
                    }
                }
            }
        }
    }
    for name in wanted {
        if !names.iter().any(|n| n == name) {
            names.push((*name).to_string());
        }
    }
    format!(
        "management.endpoints.web.exposure.include={}",
        names.join(",")
    )
}

pub fn actuator_slice(slice: &Slice) -> Change {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.root_package();
    let test = crate::generate::test_dir(root, pkg);
    Change {
        deps: vec![ACTUATOR_STARTER],
        files: vec![artifact(
            test.join("ActuatorEndpointsTest.java"),
            actuator_test_java(pkg),
        )],
        // Exposed deliberately and narrowly. The default over HTTP is health
        // alone; `*` is the shape that leaks heap dumps and environment
        // variables to anything that can reach the port.
        properties: vec![
            exposure_include(slice, &["health", "info", "prometheus", "threaddump"]),
            "management.server.port=8081".to_string(),
            "management.endpoints.web.base-path=/management".to_string(),
            "management.endpoint.health.cache.time-to-live=5s".to_string(),
            // A dependency outage must make a pod unready, never kill it.
            // Liveness therefore stays process-only; readiness is widened by
            // capabilities that own a real dependency.
            "management.endpoint.health.group.liveness.include=ping".to_string(),
            "management.endpoint.health.group.readiness.include=ping".to_string(),
            "management.endpoint.health.show-details=when-authorized".to_string(),
            "info.app.name=@project.name@".to_string(),
            "info.app.version=@project.version@".to_string(),
            "info.app.description=@project.description@".to_string(),
        ],
        ..Change::default()
    }
}

fn actuator_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/actuator_test_java.java"),
        &[("pkg", pkg)],
    )
}

// ---------------------------------------------------------------------------
// `add cache` -- caching that is switched on and provably working.
// ---------------------------------------------------------------------------

pub fn cache_slice(slice: &Slice) -> Change {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.root_package();
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    Change {
        deps: vec![CACHE_STARTER, CAFFEINE],
        files: vec![
            artifact(main.join("CacheConfig.java"), cache_config_java(pkg)),
            artifact(test.join("CacheConfigTest.java"), cache_test_java(pkg)),
        ],
        properties: vec![
            "spring.cache.type=caffeine".to_string(),
            // A cache with no bound is a memory leak with a friendly name.
            "spring.cache.caffeine.spec=maximumSize=1000,expireAfterWrite=60s".to_string(),
        ],
        ..Change::default()
    }
}

fn cache_config_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/cache_config_java.java"),
        &[("pkg", pkg)],
    )
}

fn cache_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/cache_test_java.java"),
        &[("pkg", pkg)],
    )
}

#[cfg(test)]
mod event_tests {
    use super::*;

    #[test]
    fn typed_events_share_the_field_model_and_keep_a_string_kafka_key() {
        let fields = crate::generate::parse_fields(&[
            "id:uuid".to_string(),
            "url:uri".to_string(),
            "occurredAt:instant".to_string(),
        ])
        .unwrap();
        let (_root, project) = scratch_jdbc_project("event-field");
        let files = event_files(&Slice::new(&project, None), "PageDiscovered", &fields).unwrap();

        let event = &files[0].contents;
        assert!(event.contains("record PageDiscoveredEvent(UUID id, URI url, Instant occurredAt)"));
        let publisher = &files[1].contents;
        assert!(publisher.contains("kafka.send(topic, String.valueOf(event.id()), event)"));
        let integration_test = &files[3].contents;
        assert!(
            integration_test.contains("UUID.fromString"),
            "{integration_test}"
        );
        assert!(
            integration_test.contains("URI.create"),
            "{integration_test}"
        );
        assert!(
            integration_test.contains("Instant.parse"),
            "{integration_test}"
        );
        assert!(
            integration_test.contains("isEqualTo(UUID.fromString"),
            "{integration_test}"
        );
    }

    #[test]
    fn typed_events_refuse_to_invent_a_durable_identity() {
        let fields = crate::generate::parse_fields(&["occurredAt:instant".to_string()]).unwrap();
        let (_root, project) = scratch_jdbc_project("event-no-id");
        let error =
            event_files(&Slice::new(&project, None), "PageDiscovered", &fields).unwrap_err();
        assert!(error.contains("stable `id`"), "{error}");
    }
}
// ---------------------------------------------------------------------------
// `add security` -- an explicit filter chain, rather than the default one.
// ---------------------------------------------------------------------------

pub const SECURITY_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-security",
    version: None,
    scope: None,
    optional: false,
};

pub const SECURITY_TEST: Dependency = Dependency {
    group_id: "org.springframework.security",
    artifact_id: "spring-security-test",
    version: None,
    scope: Some("test"),
    optional: false,
};

pub const OAUTH2_RESOURCE_SERVER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-oauth2-resource-server",
    version: None,
    scope: None,
    optional: false,
};

// ---------------------------------------------------------------------------
// The scaffold's service and controller -- working CRUD rather than stubs.
// ---------------------------------------------------------------------------

/// The application service a scaffolded resource gets.
///
/// Thin on purpose: it delegates to the port and returns domain types. What
/// it buys is a seam -- the controller depends on this rather than on a
/// repository, so the day one of these operations grows a rule (a permission
/// check, an event to publish) there is somewhere for it to go that is not a
/// controller method.
pub fn resource_service_java(pkg: &str, name: &str, extra: &str) -> String {
    let var = crate::generate::lower_first(name);
    crate::template::render(
        crate::template_here!("spring/resource_service_java.java"),
        &[
            ("pkg", pkg),
            ("extra", extra),
            ("name", name),
            ("var", &*var),
        ],
    )
}

/// A REST resource with the four operations that actually exist, wired to
/// the service and speaking in DTOs.
///
/// The status codes are the ones the situations mean, which is most of what
/// distinguishes a REST API from a set of methods reachable over HTTP: 201
/// with a `Location` for a creation, 204 for a delete that removed
/// something, 404 for one that did not, and 404 rather than an empty 200 for
/// a missing item.
pub fn resource_controller_java(
    slice: &Slice,
    name: &str,
    extra: &str,
    has_id: bool,
    fields: &[crate::generate::Field],
) -> String {
    let pkg: &str = &slice.placed(Layer::Web);
    if fields.iter().any(|field| field.constraints.scoped) {
        return scoped_resource_controller_java(slice, name, extra, has_id, fields);
    }
    let path = format!("/{}", crate::sql::table_name(name).replace('_', "-"));
    // A `Location` header needs something to point at. Without an `id`
    // component there is no per-item URL to build, and inventing one would
    // be worse than omitting the header.
    let (location_import, created) = if has_id {
        (
            "import java.net.URI;\n",
            format!(
                "        return ResponseEntity.created(URI.create(PATH + \"/\" + created.id()))\n\
                 \x20               .body({name}Response.from(created));"
            ),
        )
    } else {
        (
            "",
            format!(
                "        // No `id` component, so there is no per-item URL to\n\
                 \x20       // advertise in a Location header.\n\
                 \x20       return ResponseEntity.status(HttpStatus.CREATED).body({name}Response.from(created));"
            ),
        )
    };
    let status_import = if has_id {
        ""
    } else {
        "import org.springframework.http.HttpStatus;\n"
    };
    crate::template::render(
        crate::template_here!("spring/resource_controller_java.java"),
        &[
            ("pkg", pkg),
            ("extra", extra),
            ("location_import", location_import),
            ("status_import", status_import),
            ("name", name),
            ("path", &*path),
            ("created", &*created),
        ],
    )
}

fn scoped_resource_controller_java(
    slice: &Slice,
    name: &str,
    extra: &str,
    has_id: bool,
    fields: &[crate::generate::Field],
) -> String {
    let security: &str = slice.base();
    let pkg: &str = &slice.placed(Layer::Web);
    let path = format!("/{}", crate::sql::table_name(name).replace('_', "-"));
    let (
        scope_import,
        scope_field,
        scope_constructor,
        scope_assignment,
        scope_parameter,
        scope_checks,
    ) = scope_controller_parts(security, pkg, fields, "request");
    let (location_import, created) = if has_id {
        (
            "import java.net.URI;\n",
            format!(
                "        return ResponseEntity.created(URI.create(PATH + \"/\" + created.id()))\n                 .body({name}Response.from(created));"
            ),
        )
    } else {
        (
            "",
            format!(
                "        return ResponseEntity.status(HttpStatus.CREATED).body({name}Response.from(created));"
            ),
        )
    };
    let status_import = if has_id {
        ""
    } else {
        "import org.springframework.http.HttpStatus;\n"
    };
    crate::template::render(
        crate::template_here!("spring/scoped_resource_controller_java.java"),
        &[
            ("pkg", pkg),
            ("extra", extra),
            ("scope_import", &*scope_import),
            ("location_import", location_import),
            ("status_import", status_import),
            ("name", name),
            ("path", &*path),
            ("scope_field", &*scope_field),
            ("scope_constructor", &*scope_constructor),
            ("scope_assignment", &*scope_assignment),
            ("scope_parameter", &*scope_parameter),
            ("scope_checks", &*scope_checks),
            ("created", &*created),
        ],
    )
}

/// The controller's test: a standalone MVC harness with the service replaced.
///
/// `MockMvcTester.of` installs the real Spring MVC mappings, argument
/// conversion, response conversion, and exception handling around this exact
/// controller without starting a Boot application context per generated
/// resource. Security configuration and full-context behavior retain their
/// dedicated Spring tests; this companion test stays focused on HTTP and uses
/// a fresh Mockito service for every method.
pub fn resource_controller_test_java(
    slice: &Slice,
    name: &str,
    extra: &str,
    fields: &[crate::generate::Field],
) -> String {
    let security: &str = slice.base();
    let pkg: &str = &slice.placed(Layer::Web);
    if fields.iter().any(|field| field.constraints.scoped) {
        let guard_import = crate::generate::import_of(pkg, security, "ScopeAuthorizer");
        return crate::template::render(
            crate::template_here!("spring/resource_controller_test_scoped_java.java"),
            &[
                ("pkg", pkg),
                ("extra", extra),
                ("guard_import", &*guard_import),
                ("name", name),
            ],
        );
    }
    crate::template::render(
        crate::template_here!("spring/resource_controller_test_java.java"),
        &[("pkg", pkg), ("extra", extra), ("name", name)],
    )
}

/// The scaffolded service's test.
///
/// The repository is a Mockito mock rather than a hand-written fake, for one
/// reason: a fake has to key items by something, and jails cannot know which
/// component of an arbitrary record is its identity. A mock needs no such
/// knowledge, so this test compiles for every field spec.
///
/// What it pins is delegation and the two boolean-ish outcomes that are easy
/// to get backwards -- an absent item is `Optional.empty()`, and a delete
/// reports whether anything was actually removed.
pub fn resource_service_test_java(pkg: &str, name: &str, extra: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/resource_service_test_java.java"),
        &[("pkg", pkg), ("extra", extra), ("name", name)],
    )
}

/// An in-memory adapter, so a freshly scaffolded application starts and
/// serves requests before anyone has wired a database.
///
/// This is the piece that makes `jails g scaffold` produce something you can
/// actually run: the JDBC adapter is deliberately not a bean (it takes a
/// `Connection`, which the caller owns), so without this the context fails to
/// start with "no qualifying bean of type ...Repository" -- a scaffold that
/// compiles and cannot run.
///
/// It is also the honest default for the stage a scaffold is generated at.
/// Swap the `@Component` annotation onto the JDBC adapter when there is a
/// real `DataSource`; keeping both annotated would make two beans qualify for
/// one injection point, which Spring refuses to choose between (`jails
/// beans` reports exactly that).
/// The in-memory adapter.
///
/// `is_bean` decides whether it carries `@Component`, and exactly one of
/// this and the JDBC adapter may -- see `generate::RepositoryWiring`. Two
/// annotated adapters make two beans qualify for one injection point, and the
/// scaffold then compiles and refuses to start.
pub fn in_memory_repository_java(
    pkg: &str,
    name: &str,
    extra: &str,
    id_accessor: Option<&str>,
    is_bean: bool,
) -> String {
    let var = crate::generate::lower_first(name);
    let (find_by_id, delete_by_id, save_body, note) = match id_accessor {
        Some(accessor) => (
            "        return Optional.ofNullable(items.get(id));".to_string(),
            "        return items.remove(id) != null;".to_string(),
            format!("        items.put(String.valueOf({var}.{accessor}()), {var});"),
            " * <p>Keyed on the record's own {@code id} component.\n",
        ),
        None => (
            "        // TODO: this type has no `id` component, so jails cannot\n\
             \x20       // tell which part of it is the identity. Pick one and key\n\
             \x20       // `items` on it.\n\
             \x20       return Optional.empty();"
                .to_string(),
            "        return items.remove(id) != null;".to_string(),
            format!("        items.put(String.valueOf(items.size()), {var});"),
            " * <p>This type declares no {@code id} component, so lookups by id are\n\
             \x20* left unimplemented -- see the TODO in {@code findById}.\n",
        ),
    };
    // Exactly one adapter is the bean. When the JDBC one is, this is a fake
    // for tests and says so rather than pretending to be a stand-in for a
    // database that now exists.
    let repository_annotation = if is_bean { "@Component\n" } else { "" };
    let repository_import = if is_bean {
        "import org.springframework.stereotype.Component;\n"
    } else {
        ""
    };
    let role_note = if is_bean {
        " * <p>When a real {@code DataSource} arrives, `jails add db` makes\n * {@code Jdbc"
            .to_string()
            + name
            + "Repository} the bean and drops the annotation here. Annotating\n * both makes two beans qualify for one injection point, which Spring\n * refuses to choose between.\n"
    } else {
        " * <p>Not a bean: this project has a {@code DataSource}, so {@code Jdbc".to_string()
            + name
            + "Repository}\n * is the {@code @Component}. This stays as a fake for tests that want a\n * repository without a container -- construct it directly.\n"
    };
    crate::template::render(
        crate::template_here!("spring/in_memory_repository_java.java"),
        &[
            ("pkg", pkg),
            ("extra", extra),
            ("repository_import", repository_import),
            ("name", name),
            ("note", note),
            ("role_note", &*role_note),
            ("repository_annotation", repository_annotation),
            ("find_by_id", &*find_by_id),
            ("var", &*var),
            ("save_body", &*save_body),
            ("delete_by_id", &*delete_by_id),
        ],
    )
}

/// The Failsafe plugin, without which every generated `*IT` is dead code.
///
/// Surefire runs `*Test`; `*IT` belongs to Failsafe, and Failsafe is *not*
/// part of the Spring Boot parent's default build. So a project that has
/// never added it runs `mvn verify` to completion, reports success, and
/// executes none of the integration tests -- which is worse than not having
/// them, because the green build says they passed.
///
/// `integration-test` and `verify` are both bound: the first runs the tests,
/// the second is what makes a failure fail the build. Binding only the first
/// runs them and ignores the result.
pub const FAILSAFE_ARTIFACT: &str = "maven-failsafe-plugin";

/// The Failsafe plugin, versioned for the project it is going into.
///
/// Versionless is correct under `spring-boot-starter-parent`, which manages
/// it, and a trap without one: Maven only *warns* about a versionless plugin
/// rather than refusing the pom, so it resolves whatever the running Maven
/// defaults to, which is not a decision jails should be making silently.
/// plan.md §8.1 names it as the quiet half of that defect.
pub fn failsafe_plugin(flavor: crate::pom::Flavor) -> &'static str {
    match flavor {
        crate::pom::Flavor::SpringBoot => FAILSAFE_PLUGIN,
        crate::pom::Flavor::PlainMaven => FAILSAFE_PLUGIN_PINNED,
    }
}

pub const FAILSAFE_PLUGIN_PINNED: &str = r#"<plugin>
    <groupId>org.apache.maven.plugins</groupId>
    <artifactId>maven-failsafe-plugin</artifactId>
    <version>3.5.6</version>
    <executions>
        <execution>
            <goals>
                <!-- `verify` as well as `integration-test`: without it the
                     tests run and their failures are ignored. -->
                <goal>integration-test</goal>
                <goal>verify</goal>
            </goals>
        </execution>
    </executions>
</plugin>"#;

pub const FAILSAFE_PLUGIN: &str = r#"<plugin>
    <groupId>org.apache.maven.plugins</groupId>
    <artifactId>maven-failsafe-plugin</artifactId>
    <executions>
        <execution>
            <goals>
                <!-- `verify` as well as `integration-test`: without it the
                     tests run and their failures are ignored. -->
                <goal>integration-test</goal>
                <goal>verify</goal>
            </goals>
        </execution>
    </executions>
</plugin>"#;

// ---------------------------------------------------------------------------
// `add redis` -- a key/value store with a compose service and a real test.
// ---------------------------------------------------------------------------

pub const REDIS_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-data-redis",
    version: None,
    scope: None,
    optional: false,
};

pub const REDIS_IMAGE: &str = "redis:7-alpine";

pub fn redis_slice(slice: &Slice) -> Change {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.placed(Layer::Adapters);
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    Change {
        deps: vec![REDIS_STARTER, TESTCONTAINERS_CORE, SPRING_TESTCONTAINERS],
        files: vec![
            artifact(main.join("KeyValueStore.java"), key_value_store_java(pkg)),
            artifact(
                test.join("KeyValueStoreIT.java"),
                key_value_store_it_java(pkg),
            ),
        ],
        properties: vec![
            "spring.data.redis.host=localhost".to_string(),
            "spring.data.redis.port=6379".to_string(),
            // A key/value store is a cache, not a database, and a cache
            // without expiry is a memory leak that survives restarts. This is
            // the default the wrapper applies when no TTL is given.
            "app.redis.default-ttl=PT10M".to_string(),
        ],
        ..Change::default()
    }
}

fn key_value_store_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/key_value_store_java.java"),
        &[("pkg", pkg)],
    )
}

fn key_value_store_it_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/key_value_store_it_java.java"),
        &[("pkg", pkg), ("REDIS_IMAGE", REDIS_IMAGE)],
    )
}

// ---------------------------------------------------------------------------
// `add observability` -- metrics that are attributable and scrapeable.
// ---------------------------------------------------------------------------

/// Version managed by the Spring Boot parent, which imports the Micrometer
/// BOM (verified in spring-boot-dependencies, not assumed).
pub const PROMETHEUS_REGISTRY: Dependency = Dependency {
    group_id: "io.micrometer",
    artifact_id: "micrometer-registry-prometheus",
    version: None,
    scope: None,
    optional: false,
};

/// Metrics, exposed for scraping and tagged so they can be told apart.
///
/// The boilerplate this removes is not the dependency -- it is the two
/// conventions people rediscover per project. Metrics with no common tag are
/// unattributable the moment a second service reports to the same Prometheus,
/// and a counter created inline per call site gets a slightly different name
/// each time (`orders.created`, `order_created`, `ordersCreated`) until the
/// dashboards stop agreeing.
pub fn observability_slice(slice: &Slice) -> Change {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.root_package();
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    Change {
        deps: vec![ACTUATOR_STARTER, PROMETHEUS_REGISTRY],
        files: vec![
            artifact(
                main.join("MetricsConfig.java"),
                metrics_config_java(pkg, meter_registry_customizer_import(slice)),
            ),
            artifact(main.join("AppMetrics.java"), app_metrics_java(pkg)),
            artifact(test.join("AppMetricsTest.java"), app_metrics_test_java(pkg)),
            artifact(
                test.join("PrometheusScrapeTest.java"),
                prometheus_scrape_test_java(pkg),
            ),
        ],
        properties: vec![
            // `prometheus` in addition to the actuator defaults. Still named
            // individually rather than `*`, which would publish heapdump and
            // the resolved environment.
            exposure_include(slice, &["health", "info", "prometheus", "threaddump"]),
            // Observability owns the scrape endpoint, so it must also make the
            // management connector private-by-default when actuator was not
            // added separately. Both capabilities converge on the same
            // values, which keeps their application order irrelevant.
            "management.server.port=8081".to_string(),
            "management.endpoints.web.base-path=/management".to_string(),
            "management.endpoint.health.cache.time-to-live=5s".to_string(),
            "management.endpoint.health.group.liveness.include=ping".to_string(),
            "management.endpoint.health.group.readiness.include=ping".to_string(),
            "management.endpoint.health.show-details=when-authorized".to_string(),
            // Explicit SLOs produce a bounded, useful histogram. Enabling the
            // default percentile histogram creates roughly seventy buckets
            // for every endpoint/status pair.
            "management.metrics.distribution.slo.http.server.requests=100ms,250ms,500ms,1s,2s,5s,10s"
                .to_string(),
            "management.metrics.distribution.percentiles-histogram.http.server.requests=false"
                .to_string(),
            "management.metrics.distribution.percentiles.http.server.requests=0.5,0.9,0.95,0.99"
                .to_string(),
            "management.metrics.distribution.minimum-expected-value.http.server.requests=1ms"
                .to_string(),
            "management.metrics.distribution.maximum-expected-value.http.server.requests=10s"
                .to_string(),
            "management.tracing.propagation.type=w3c".to_string(),
            "management.tracing.sampling.probability=0.1".to_string(),
            "management.tracing.baggage.correlation.fields=request-id".to_string(),
            "management.tracing.baggage.tag-fields=request-id".to_string(),
            // Internal correlation is useful in logs but must not leak to a
            // third party as propagated baggage.
            "management.tracing.baggage.local-fields=request-id".to_string(),
            // Write access logs to the container's stdout device, with no
            // date suffix or buffering. The management server has its own
            // prefix default (`management_`); overriding it is essential or
            // Tomcat tries to create /dev/management_stdout as a non-root
            // user instead of opening /dev/stdout.
            "server.tomcat.accesslog.enabled=true".to_string(),
            "server.tomcat.accesslog.directory=/dev".to_string(),
            "server.tomcat.accesslog.prefix=stdout".to_string(),
            "server.tomcat.accesslog.suffix=".to_string(),
            "server.tomcat.accesslog.file-date-format=".to_string(),
            "server.tomcat.accesslog.buffered=false".to_string(),
            "management.server.tomcat.accesslog.prefix=stdout".to_string(),
        ],
        ..Change::default()
    }
}

fn app_metrics_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/app_metrics_java.java"),
        &[("pkg", pkg)],
    )
}

fn app_metrics_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/app_metrics_test_java.java"),
        &[("pkg", pkg)],
    )
}

#[cfg(test)]
mod observability_tests {
    use super::*;

    fn scratch(tag: &str) -> (std::path::PathBuf, Project) {
        scratch_project(&format!("observability-{tag}"), "<project></project>")
    }

    fn fixture(tag: &str, properties: &str) -> (std::path::PathBuf, Project) {
        let (dir, project) = scratch(tag);
        let resources = dir.join("src/main/resources");
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::write(resources.join("application.properties"), properties).unwrap();
        (dir, project)
    }

    #[test]
    fn the_exposure_list_unions_rather_than_replaces() {
        let (_dir, project) = fixture(
            "union",
            "management.endpoints.web.exposure.include=health,info,metrics\n",
        );
        assert_eq!(
            exposure_include(
                &Slice::new(&project, None),
                &["health", "info", "metrics", "prometheus"]
            ),
            "management.endpoints.web.exposure.include=health,info,metrics,prometheus"
        );
    }

    #[test]
    fn a_narrower_capability_does_not_drop_what_a_wider_one_exposed() {
        // `add observability` then `add actuator`: actuator's own list is a
        // subset, and appending it verbatim would win and hide the scrape.
        let (_dir, project) = fixture(
            "narrower",
            "management.endpoints.web.exposure.include=health,info,metrics,prometheus\n",
        );
        assert!(
            exposure_include(&Slice::new(&project, None), &["health", "info", "metrics"])
                .ends_with("prometheus")
        );
    }

    #[test]
    fn a_hand_widened_list_is_preserved_not_rewritten() {
        let (_dir, project) = fixture(
            "hand-widened",
            "management.endpoints.web.exposure.include=health,loggers\n",
        );
        let line = exposure_include(&Slice::new(&project, None), &["health", "info", "metrics"]);
        assert!(line.contains("loggers"), "{line}");
    }

    #[test]
    fn no_properties_file_yields_just_the_wanted_names() {
        let (_dir, project) = scratch("absent");
        assert_eq!(
            exposure_include(&Slice::new(&project, None), &["health", "prometheus"]),
            "management.endpoints.web.exposure.include=health,prometheus"
        );
    }

    #[test]
    fn actuator_and_prometheus_share_the_same_spring_context_configuration() {
        fn spring_boot_test_arguments(source: &str) -> &str {
            source
                .split_once("@SpringBootTest(")
                .unwrap()
                .1
                .split_once(")\nclass ")
                .unwrap()
                .0
        }

        let actuator = actuator_test_java("com.example.demo");
        let prometheus = prometheus_scrape_test_java("com.example.demo");
        assert_eq!(
            spring_boot_test_arguments(&actuator),
            spring_boot_test_arguments(&prometheus)
        );
    }
}

fn prometheus_scrape_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/prometheus_scrape_test_java.java"),
        &[("pkg", pkg)],
    )
}

/// Boot 4 moved `MeterRegistryCustomizer` out of `actuate.autoconfigure`, with
/// no shim -- the same class of break as `@AutoConfigureMockMvc`.
fn meter_registry_customizer_import(slice: &Slice) -> &'static str {
    const LEGACY: &str =
        "org.springframework.boot.actuate.autoconfigure.metrics.MeterRegistryCustomizer";
    const CURRENT: &str =
        "org.springframework.boot.micrometer.metrics.autoconfigure.MeterRegistryCustomizer";
    if slice.project().boot_major() >= 4 {
        CURRENT
    } else {
        LEGACY
    }
}

fn metrics_config_java(pkg: &str, customizer_import: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/metrics_config_java.java"),
        &[("pkg", pkg), ("customizer_import", customizer_import)],
    )
}
