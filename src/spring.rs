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

use crate::Result;
use crate::model::{Artifact, Change, Layer, Slice};

mod auth;
mod durable;
mod http;
mod query;
mod schema;
mod sse;
mod transition;
mod webhook;
mod workflow;
pub(crate) use auth::*;
pub(crate) use durable::*;
pub(crate) use http::*;
pub(crate) use query::*;
pub(crate) use schema::*;
pub(crate) use sse::*;
pub(crate) use transition::*;
pub(crate) use webhook::*;
pub(crate) use workflow::*;
// Production code here reaches the project through `Slice`; only the renderer
// fixtures build one directly.
#[cfg(test)]
use crate::model::Project;
use crate::pom::{Dependency, Flavor};

fn artifact(path: std::path::PathBuf, contents: String) -> Artifact {
    Artifact::rendered(path, contents)
}

pub(crate) const VALIDATION_STARTER: Dependency = Dependency {
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
pub(crate) const JAKARTA_VALIDATION_API: Dependency = Dependency {
    group_id: "jakarta.validation",
    artifact_id: "jakarta.validation-api",
    version: Some("3.1.1"),
    scope: None,
    optional: false,
};

/// Whichever of the two the project can actually resolve.
pub(crate) fn validation_dependency(flavor: crate::pom::Flavor) -> &'static Dependency {
    match flavor {
        crate::pom::Flavor::SpringBoot => &VALIDATION_STARTER,
        crate::pom::Flavor::PlainMaven => &JAKARTA_VALIDATION_API,
    }
}

pub(crate) const ACTUATOR_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-actuator",
    version: None,
    scope: None,
    optional: false,
};

pub(crate) const CACHE_STARTER: Dependency = Dependency {
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
pub(crate) const RESTCLIENT_STARTER: Dependency = Dependency {
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
pub(crate) const APACHE_HTTPCLIENT: Dependency = Dependency {
    group_id: "org.apache.httpcomponents.client5",
    artifact_id: "httpclient5",
    version: None,
    scope: None,
    optional: false,
};

/// Caffeine is the cache Spring Boot picks up automatically when it is on
/// the classpath and nothing else claims the slot. Version managed by the
/// Boot parent so it moves with the platform.
pub(crate) const CAFFEINE: Dependency = Dependency {
    group_id: "com.github.ben-manes.caffeine",
    artifact_id: "caffeine",
    version: None,
    scope: None,
    optional: false,
};

/// Refuse politely rather than generating Spring code into a plain Maven
/// project, where it would not compile and the reason would not be obvious.
pub(crate) fn require_spring(flavor: Flavor, capability: &str) -> Result<()> {
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
pub(crate) fn api_slice(slice: &Slice) -> Change {
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
        crate::template::template!("spring/api_exception_java.java"),
        &[("pkg", pkg)],
    )
}

fn api_exception_handler_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/api_exception_handler_java.java"),
        &[("pkg", pkg)],
    )
}

fn api_exception_handler_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/api_exception_handler_test_java.java"),
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

pub(crate) fn actuator_slice(slice: &Slice) -> Change {
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
        crate::template::template!("spring/actuator_test_java.java"),
        &[("pkg", pkg)],
    )
}

// ---------------------------------------------------------------------------
// `add cache` -- caching that is switched on and provably working.
// ---------------------------------------------------------------------------

pub(crate) fn cache_slice(slice: &Slice) -> Change {
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
        crate::template::template!("spring/cache_config_java.java"),
        &[("pkg", pkg)],
    )
}

fn cache_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/cache_test_java.java"),
        &[("pkg", pkg)],
    )
}

// ---------------------------------------------------------------------------
// `generate dto` -- the request and response shapes for a domain type.
// ---------------------------------------------------------------------------

/// Request/response records for a domain type, plus the mapping between them.
///
/// This is the most-typed, least-thought-about code in a Spring service, and
/// skipping it is worse than writing it: exposing a domain record directly as
/// the API contract means every internal rename is a breaking change for
/// clients, and every new field is published whether or not anyone meant to.
///
/// The request carries bean-validation annotations derived from the field
/// spec jails already has -- a non-null component becomes `@NotNull`, a
/// non-blank one `@NotBlank` -- so a malformed request is rejected at the
/// edge and reported by `add api`'s handler as a 400 naming the field.
pub(crate) fn dto_files(
    slice: &Slice,
    name: &str,
    fields: &[crate::generate::Field],
) -> Vec<Artifact> {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.placed(Layer::Web);
    let domain: &str = &slice.placed(Layer::Domain);
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    let domain_import = crate::generate::import_of(pkg, domain, name);
    vec![
        Artifact {
            kind: "request",
            path: main.join(format!("{name}Request.java")),
            contents: request_java_for(pkg, name, fields, &domain_import, domain),
        },
        Artifact {
            kind: "response",
            path: main.join(format!("{name}Response.java")),
            contents: response_java_for(pkg, name, fields, &domain_import, domain),
        },
        Artifact {
            kind: "dto test",
            path: test.join(format!("{name}DtoTest.java")),
            contents: dto_test_java(slice, name, fields, &domain_import),
        },
    ]
}

/// Which validation annotation a component earns, from the optionality jails
/// already parsed. Returns the annotation and the import it needs.
fn validation_for(field: &crate::generate::Field) -> Option<(&'static str, &'static str)> {
    use crate::generate::Optionality;
    // A primitive cannot be null, so @NotNull on one is noise at best -- and
    // Hibernate Validator rejects some constraint/type pairings outright.
    if is_primitive(&field.java_type) {
        return None;
    }
    match field.optionality {
        // `!` means non-blank, which only applies to text -- and @NotBlank
        // implies @NotNull, so one annotation covers both.
        Optionality::NonBlank => Some(("@NotBlank", "jakarta.validation.constraints.NotBlank")),
        Optionality::Required => Some(("@NotNull", "jakarta.validation.constraints.NotNull")),
        // `?` is explicitly optional: constraining it would contradict the
        // field spec.
        Optionality::Nullable => None,
    }
}

fn is_primitive(java_type: &str) -> bool {
    matches!(
        java_type,
        "int" | "long" | "double" | "float" | "boolean" | "char" | "byte" | "short"
    )
}

/// The DTO's own component type. An `Optional<T>` domain component becomes a
/// plain nullable `T` on the wire: JSON has `null` and no notion of an
/// absent-vs-null-valued Optional, and Jackson serialising an `Optional`
/// without the JDK8 module produces `{"present":true}` rather than the value.
fn wire_type(field: &crate::generate::Field) -> String {
    // `java_type` is always the inner type; `optionality` says whether the
    // record wraps it. The wire type is the inner one either way.
    field.java_type.clone()
}

/// Imports for the DTO's own components.
///
/// `owner`/`user` are the domain package and the DTO's package: a component
/// whose type the project declares (an enum, most often) needs importing from
/// wherever the domain lives, and `field.imports` cannot carry that because
/// jails only knows the built-in types' packages. Missing it produces a
/// record that names a type it cannot see, which javac catches and no
/// template review does.
fn dto_imports(
    fields: &[crate::generate::Field],
    with_validation: bool,
    owner: &str,
    user: &str,
) -> String {
    let mut imports: Vec<String> = Vec::new();
    for field in fields {
        if field.owned {
            let import = crate::generate::import_of(user, owner, &field.java_type);
            if !import.is_empty() {
                imports.push(
                    import
                        .trim()
                        .trim_start_matches("import ")
                        .trim_end_matches(';')
                        .to_string(),
                );
            }
        }
        for import in &field.imports {
            // Optional itself never reaches the wire type, so its import
            // would be unused -- and an unused import fails `jails check`
            // under a strict formatter.
            if *import == "java.util.Optional" {
                continue;
            }
            imports.push((*import).to_string());
        }
        if with_validation && let Some((_, import)) = validation_for(field) {
            imports.push(import.to_string());
        }
    }
    imports.sort();
    imports.dedup();
    imports
        .into_iter()
        .map(|i| format!("import {i};\n"))
        .collect()
}

fn components(fields: &[crate::generate::Field], with_validation: bool) -> String {
    fields
        .iter()
        .map(|field| {
            let annotation = if with_validation {
                validation_for(field)
                    .map(|(a, _)| format!("{a} "))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            format!("        {annotation}{} {}", wire_type(field), field.name)
        })
        .collect::<Vec<_>>()
        .join(",\n")
}

/// `x.name()` for a plain component, `x.name().orElse(null)` for an Optional
/// one -- the wire type is nullable, so the Optional is unwrapped exactly
/// once, here, rather than at every call site.
fn read_from_domain(field: &crate::generate::Field, receiver: &str) -> String {
    let accessor = format!("{receiver}.{}()", field.name);
    if is_optional(field) {
        format!("{accessor}.orElse(null)")
    } else {
        accessor
    }
}

fn is_optional(field: &crate::generate::Field) -> bool {
    field.optionality == crate::generate::Optionality::Nullable
}

/// The reverse: a nullable wire component becomes an `Optional` again. The
/// generated record's compact constructor normalises a null Optional, so
/// `ofNullable` is enough.
fn write_to_domain(field: &crate::generate::Field) -> String {
    if is_optional(field) {
        format!("Optional.ofNullable({})", field.name)
    } else {
        field.name.clone()
    }
}

fn needs_optional(fields: &[crate::generate::Field]) -> bool {
    fields.iter().any(is_optional)
}

pub(crate) fn request_java_for(
    pkg: &str,
    name: &str,
    fields: &[crate::generate::Field],
    domain_import: &str,
    domain: &str,
) -> String {
    let imports = dto_imports(fields, true, domain, pkg);
    let optional_import = if needs_optional(fields) {
        "import java.util.Optional;\n"
    } else {
        ""
    };
    let components = components(fields, true);
    let arguments = fields
        .iter()
        .map(write_to_domain)
        .map(|a| format!("                {a}"))
        .collect::<Vec<_>>()
        .join(",\n");
    crate::template::render(
        crate::template::template!("spring/request_java_for.java"),
        &[
            ("pkg", pkg),
            ("domain_import", domain_import),
            ("optional_import", optional_import),
            ("imports", &*imports),
            ("name", name),
            ("components", &*components),
            ("arguments", &*arguments),
        ],
    )
}

pub(crate) fn response_java_for(
    pkg: &str,
    name: &str,
    fields: &[crate::generate::Field],
    domain_import: &str,
    domain: &str,
) -> String {
    let imports = dto_imports(fields, false, domain, pkg);
    let components = components(fields, false);
    let arguments = fields
        .iter()
        .map(|field| read_from_domain(field, &crate::generate::lower_first(name)))
        .map(|a| format!("                {a}"))
        .collect::<Vec<_>>()
        .join(",\n");
    let var = crate::generate::lower_first(name);
    crate::template::render(
        crate::template::template!("spring/response_java_for.java"),
        &[
            ("pkg", pkg),
            ("domain_import", domain_import),
            ("imports", &*imports),
            ("name", name),
            ("components", &*components),
            ("var", &*var),
            ("arguments", &*arguments),
        ],
    )
}

/// The round-trip test.
///
/// jails follows one rule for a test it cannot fully write: emit it whole and
/// `@Disabled`, naming what is missing. Emitting a guess would produce a test
/// that does not compile; emitting nothing would drop the coverage silently.
/// Here the guess would be a sample value for a component whose type jails
/// has no model of, so the sample is attempted per component and the whole
/// test is disabled only if some component defeats it.
fn dto_test_java(
    slice: &Slice,
    name: &str,
    fields: &[crate::generate::Field],
    domain_import: &str,
) -> String {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.placed(Layer::Web);
    let domain: &str = &slice.placed(Layer::Domain);
    let var = crate::generate::lower_first(name);
    // A request component is the *wire* type: an Optional domain component is
    // a plain nullable field here, so `Optional.empty()` would not compile as
    // its sample. `null` is the honest wire-level equivalent.
    let samples: Vec<Option<String>> = fields
        .iter()
        .map(|field| {
            if is_optional(field) {
                Some("null".to_string())
            } else {
                crate::generate::sample_value(field, root, domain)
            }
        })
        .collect();
    let unsampleable: Vec<&str> = fields
        .iter()
        .zip(&samples)
        .filter(|(_, sample)| sample.is_none())
        .map(|(field, _)| field.java_type.as_str())
        .collect();

    let disabled = if unsampleable.is_empty() {
        String::new()
    } else {
        format!(
            "@Disabled(\"todo: supply a sample for {} -- jails cannot know how to build one\")\n",
            unsampleable.join(", ")
        )
    };
    let disabled_import = if unsampleable.is_empty() {
        String::new()
    } else {
        "import org.junit.jupiter.api.Disabled;\n".to_string()
    };
    let arguments = fields
        .iter()
        .zip(&samples)
        .map(|(field, sample)| {
            format!(
                "                {}",
                sample
                    .clone()
                    .unwrap_or_else(|| format!("null /* {} */", field.java_type))
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    // The sample literals need the same imports the wire types do
    // (`UUID.fromString`, `Instant.parse`, ...), and `dto_imports` already
    // computes exactly that set with Optional filtered out.
    let sample_imports = dto_imports(fields, false, domain, pkg);

    crate::template::render(
        crate::template::template!("spring/dto_test_java.java"),
        &[
            ("pkg", pkg),
            ("domain_import", domain_import),
            ("sample_imports", &*sample_imports),
            ("disabled_import", &*disabled_import),
            ("disabled", &*disabled),
            ("name", name),
            ("var", &*var),
            ("arguments", &*arguments),
        ],
    )
}

// ---------------------------------------------------------------------------
// `generate event` -- a Kafka publisher, listener and payload, as one slice.
// ---------------------------------------------------------------------------

/// Boot's Testcontainers integration, needed for `@ServiceConnection`.
pub(crate) const SPRING_TESTCONTAINERS: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-testcontainers",
    version: None,
    scope: Some("test"),
    optional: false,
};

/// Testcontainers' Kafka module. Named the 2.x way (`testcontainers-kafka`),
/// matching the postgres module `add db` already pins.
pub(crate) const TESTCONTAINERS_KAFKA: Dependency = Dependency {
    group_id: "org.testcontainers",
    artifact_id: "testcontainers-kafka",
    version: Some("2.0.5"),
    scope: Some("test"),
    optional: false,
};

/// The Spring Kafka properties that make publish-and-consume actually work.
///
/// Every one of these is a thing people discover by losing an afternoon:
///
/// - `auto-offset-reset=earliest`: a consumer joining a group for the first
///   time otherwise starts at the *end* of the topic, so anything published
///   before it joined is invisible. This is the single most common reason a
///   Kafka integration test hangs and then fails with nothing consumed.
/// - `JacksonJsonSerializer`/`JacksonJsonDeserializer`: the defaults are
///   `StringSerializer`, so a record payload arrives as its `toString()` and
///   comes back unparseable. Note the `Jackson` prefix -- the older
///   `JsonSerializer`/`JsonDeserializer` pair is deprecated for removal since
///   Spring Kafka 4.0, which moved to Jackson 3.
/// - `spring.json.trusted.packages`: the deserializer refuses to instantiate
///   a type outside the trusted list, and reports it as a deserialization
///   failure rather than a configuration one.
pub(crate) fn kafka_properties(base: &str, group: &str) -> Vec<String> {
    vec![
        "spring.kafka.bootstrap-servers=localhost:9092".to_string(),
        format!("spring.kafka.consumer.group-id={group}"),
        "spring.kafka.consumer.auto-offset-reset=earliest".to_string(),
        "spring.kafka.producer.value-serializer=org.springframework.kafka.support.serializer.JacksonJsonSerializer".to_string(),
        // Both the base package *and* a wildcard for everything under it.
        // The check is `PatternMatchUtils.simpleMatch` against the class's
        // package name, so it is neither a prefix match nor recursive:
        // `com.example.app` alone rejects `com.example.app.messaging` --
        // where `jails g event` puts the payload -- and the failure surfaces
        // as a SerializationException reading "is not in the trusted
        // packages", which sounds like a security setting rather than a
        // missing dot-star. The wildcard alone would not match the base
        // package itself, hence both.
        format!("spring.kafka.consumer.properties.spring.json.trusted.packages={base},{base}.*"),
        // KIP-848. The broker default since Kafka 4.0, but the *client*
        // default is still `classic`, so a project that does not opt in keeps
        // the stop-the-world rebalance -- every consumer in the group stops
        // while one joins. Nothing reports this; it just stays slow.
        "spring.kafka.consumer.properties.group.protocol=consumer".to_string(),
        // Durability over throughput. `acks=all` waits for the in-sync
        // replicas, and idempotence stops a producer retry from writing the
        // record twice. Both are stated rather than inherited because the
        // defaults have moved between client versions.
        "spring.kafka.producer.acks=all".to_string(),
        "spring.kafka.producer.properties.enable.idempotence=true".to_string(),
    ]
    .into_iter()
    .chain(kafka_deserializer_properties())
    .collect()
}

/// The deserializer half, kept apart because it is what makes a poison
/// message survivable.
///
/// A record that will not deserialize will not deserialize on the next
/// attempt either. Left as a plain `JacksonJsonDeserializer`, the failure is
/// thrown *inside* the consumer before any error handler can see it as a
/// record, so the container retries the same bad offset forever and the
/// partition stops. `ErrorHandlingDeserializer` catches it and hands the
/// error along as the record's value, which is the only shape
/// `DefaultErrorHandler` can route to a dead-letter topic.
///
/// Separate from [`kafka_properties`] only for readability -- `add kafka`
/// writes both, and one without the other is the bug.
fn kafka_deserializer_properties() -> Vec<String> {
    vec![
        "spring.kafka.consumer.value-deserializer=org.springframework.kafka.support.serializer.ErrorHandlingDeserializer".to_string(),
        "spring.kafka.consumer.properties.spring.deserializer.value.delegate.class=org.springframework.kafka.support.serializer.JacksonJsonDeserializer".to_string(),
    ]
}

/// What happens to a record that does not process cleanly.
///
/// This is the half of a Kafka integration that nobody writes on day one and
/// everybody needs on day two. Spring Kafka's default is to retry a failing
/// record ten times and then *log and move on*, and the shape of the failure
/// decides which half of that is wrong:
///
/// - A record that will not deserialize, or that names an enum constant this
///   service does not have, fails identically on every attempt. Retrying it
///   is a loop that costs the whole partition, and the only symptom is
///   consumer lag with no new errors after the first.
/// - A database that is briefly unavailable is the opposite case, and the one
///   the backoff exists for.
///
/// So the classification is the load-bearing part, not the backoff.
///
/// It is expressed as *one* marker exception rather than a list of JDK types,
/// for two reasons that come out of
/// `deps/spring-kafka/.../listener/ExceptionClassifier.java`:
///
/// - The framework already treats `DeserializationException`,
///   `MessageConversionException`, `ConversionException`,
///   `MethodArgumentResolutionException` and `ClassCastException` as fatal
///   (`defaultFatalExceptionsList`). Re-listing one of those reads as if the
///   generated list were the whole policy, and hides the other four.
/// - Naming `NullPointerException` there is worse than redundant. An NPE is a
///   bug in the listener, not a bad record; classifying it permanent commits
///   the offset and destroys the repeating failure that would have surfaced
///   it. Only the domain knows what is genuinely unprocessable, so only the
///   domain gets to say so -- see [`non_retryable_exception_java`].
///
/// No `NewTopic` beans here: `add kafka` does not know what this service's
/// topics are called. `jails g event <Name>` declares them, because it does.
fn kafka_config_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/kafka_config_java.java"),
        &[("pkg", pkg)],
    )
}

/// The domain's own "no retry will ever fix this".
///
/// Deliberately unlike [`api_exception_java`], which is sealed, abstract and
/// stack-trace-free. This one is open -- callers throw and subclass it -- and it
/// keeps its stack trace, because it wraps a real cause and that cause is what
/// a human reads out of the dead-letter headers.
fn non_retryable_exception_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/non_retryable_exception_java.java"),
        &[("pkg", pkg)],
    )
}

/// A test that the poison-message path is actually wired, without a broker.
///
/// The container-backed version belongs to `g event`; this one exists so that
/// `add kafka` keeps the promise `jails add --help` makes -- a dependency,
/// the code that uses it, *and a test that proves it works*.
fn kafka_config_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/kafka_config_test_java.java"),
        &[("pkg", pkg)],
    )
}

/// The files `add kafka` writes on a Spring project.
pub(crate) fn kafka_files(root: &Path, pkg: &str) -> Vec<Artifact> {
    vec![
        Artifact {
            kind: "kafka config",
            path: crate::generate::main_dir(root, pkg).join("KafkaConfig.java"),
            contents: kafka_config_java(pkg),
        },
        Artifact {
            kind: "non-retryable exception",
            path: crate::generate::main_dir(root, pkg).join("NonRetryableException.java"),
            contents: non_retryable_exception_java(pkg),
        },
        Artifact {
            kind: "kafka config test",
            path: crate::generate::test_dir(root, pkg).join("KafkaConfigTest.java"),
            contents: kafka_config_test_java(pkg),
        },
    ]
}

pub(crate) fn event_files(
    slice: &Slice,
    name: &str,
    fields: &[crate::generate::Field],
) -> Result<Vec<Artifact>> {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.placed(Layer::Messaging);
    let domain: &str = &slice.placed(Layer::Domain);
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    let topic = crate::sql::snake_case(name).replace('_', "-");
    let id = fields.iter().find(|field| field.name == "id");
    if !fields.is_empty() && id.is_none() {
        return Err(format!(
            "an event payload needs a stable `id` field for deduplication and Kafka partitioning.\n       \
             Add `id:string!` or `id:uuid` to `jails g event {name} ...`."
        ));
    }
    if id.is_some_and(|field| field.optionality == crate::generate::Optionality::Nullable) {
        return Err(
            "an event `id` cannot be optional: a null key loses per-entity ordering".to_string(),
        );
    }
    let key = id
        .filter(|field| field.java_type != "String")
        .map(|_| "String.valueOf(event.id())")
        .unwrap_or("event.id()");
    Ok(vec![
        Artifact {
            kind: "event",
            path: main.join(format!("{name}Event.java")),
            contents: event_java(pkg, domain, name, fields),
        },
        Artifact {
            kind: "publisher",
            path: main.join(format!("{name}Publisher.java")),
            contents: publisher_java(pkg, name, &topic, key),
        },
        Artifact {
            kind: "listener",
            path: main.join(format!("{name}Listener.java")),
            contents: listener_java(pkg, name, &topic),
        },
        Artifact {
            kind: "messaging integration test",
            path: test.join(format!("{name}MessagingIT.java")),
            contents: messaging_it_java(slice, name, &topic, fields),
        },
    ])
}

fn event_java(pkg: &str, domain: &str, name: &str, fields: &[crate::generate::Field]) -> String {
    if fields.is_empty() {
        return crate::template::render(
            crate::template::template!("spring/event_java.java"),
            &[("pkg", pkg), ("name", name)],
        );
    }

    let event = format!("{name}Event");
    let mut source = crate::generate::record_java(pkg, &event, fields);
    let mut imports = fields
        .iter()
        .filter(|field| field.owned && domain != pkg)
        .map(|field| format!("import {domain}.{};", field.java_type))
        .collect::<Vec<_>>();
    imports.sort();
    imports.dedup();
    if !imports.is_empty() {
        let package = format!("package {pkg};\n");
        let replacement = format!("{package}\n{}\n", imports.join("\n"));
        source = source.replacen(&package, &replacement, 1);
        source = crate::generate::normalize_imports(&source);
    }
    source.replace(
        &format!(" * An immutable {event} value."),
        &format!(" * Immutable payload published as {event}."),
    )
}

fn publisher_java(pkg: &str, name: &str, topic: &str, key: &str) -> String {
    let source = crate::template::render(
        crate::template::template!("spring/publisher_java.java"),
        &[("pkg", pkg), ("name", name), ("topic", topic)],
    );
    source.replace(
        "kafka.send(topic, event.id(), event)",
        &format!("kafka.send(topic, {key}, event)"),
    )
}

fn listener_java(pkg: &str, name: &str, topic: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/listener_java.java"),
        &[("pkg", pkg), ("name", name), ("topic", topic)],
    )
}

fn messaging_it_java(
    slice: &Slice,
    name: &str,
    topic: &str,
    fields: &[crate::generate::Field],
) -> String {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.placed(Layer::Messaging);
    let domain: &str = &slice.placed(Layer::Domain);
    let (event_imports, disabled_import, disabled, event_args, expected_id) = if fields.is_empty() {
        (
            "import java.time.Instant;\n".to_string(),
            String::new(),
            String::new(),
            "\"probe-1\", Instant.parse(\"2024-01-01T00:00:00Z\")".to_string(),
            "\"probe-1\"".to_string(),
        )
    } else {
        let samples = fields
            .iter()
            .map(|field| crate::generate::sample_value(field, root, domain))
            .collect::<Vec<_>>();
        let missing = fields
            .iter()
            .zip(&samples)
            .filter(|(_, sample)| sample.is_none())
            .map(|(field, _)| field.name.as_str())
            .collect::<Vec<_>>();
        let expected_id = fields
            .iter()
            .zip(&samples)
            .find(|(field, _)| field.name == "id")
            .and_then(|(_, sample)| sample.clone())
            .unwrap_or_else(|| "null /* TODO: an event id sample */".to_string());
        let event_args = fields
            .iter()
            .zip(samples)
            .map(|(field, sample)| {
                sample.unwrap_or_else(|| format!("null /* TODO: a {} */", field.java_type))
            })
            .collect::<Vec<_>>()
            .join(",\n                ");
        let mut imports = fields
            .iter()
            .flat_map(|field| field.imports.iter().copied().map(str::to_string))
            .collect::<Vec<_>>();
        imports.extend(
            fields
                .iter()
                .filter(|field| field.owned && domain != pkg)
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
        let imports = imports
            .into_iter()
            .map(|import| format!("import {import};\n"))
            .collect::<String>();
        if missing.is_empty() {
            (
                imports,
                String::new(),
                String::new(),
                event_args,
                expected_id,
            )
        } else {
            (
                imports,
                "import org.junit.jupiter.api.Disabled;\n".to_string(),
                format!(
                    "@Disabled(\"todo: supply a sample for {} -- jails cannot build the full event\")\n",
                    missing.join(", ")
                ),
                event_args,
                expected_id,
            )
        }
    };
    crate::template::render(
        crate::template::template!("spring/messaging_it_java.java"),
        &[
            ("pkg", pkg),
            ("name", name),
            ("topic", topic),
            ("event_imports", &event_imports),
            ("disabled_import", &disabled_import),
            ("disabled", &disabled),
            ("event_args", &event_args),
            ("expected_id", &expected_id),
        ],
    )
}

#[cfg(test)]
mod event_tests {
    use super::*;

    #[test]
    fn typed_events_share_the_field_model_and_keep_a_string_kafka_key() {
        let fields = crate::generate::parse_fields_for_test(&[
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
        let fields =
            crate::generate::parse_fields_for_test(&["occurredAt:instant".to_string()]).unwrap();
        let (_root, project) = scratch_jdbc_project("event-no-id");
        let error =
            event_files(&Slice::new(&project, None), "PageDiscovered", &fields).unwrap_err();
        assert!(error.contains("stable `id`"), "{error}");
    }
}
// ---------------------------------------------------------------------------
// `add security` -- an explicit filter chain, rather than the default one.
// ---------------------------------------------------------------------------

pub(crate) const SECURITY_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-security",
    version: None,
    scope: None,
    optional: false,
};

pub(crate) const SECURITY_TEST: Dependency = Dependency {
    group_id: "org.springframework.security",
    artifact_id: "spring-security-test",
    version: None,
    scope: Some("test"),
    optional: false,
};

pub(crate) const OAUTH2_RESOURCE_SERVER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-oauth2-resource-server",
    version: None,
    scope: None,
    optional: false,
};

/// The security slice.
///
/// Adding `spring-boot-starter-security` on its own changes the application
/// immediately and drastically: every endpoint requires authentication, a
/// login form appears, and a generated password is printed to the log once at
/// startup. That is a safe default and a bewildering one -- the usual first
/// reaction is to search for how to turn it off, which is how applications
/// end up with `permitAll()` on everything.
///
/// So this writes the filter chain explicitly. An explicit chain is readable,
/// reviewable, and testable, and the generated test asserts both directions:
/// anonymous requests are rejected, authenticated ones are not.
pub(crate) fn security_slice(slice: &Slice) -> Change {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.root_package();
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    Change {
        deps: vec![SECURITY_STARTER, OAUTH2_RESOURCE_SERVER, SECURITY_TEST],
        files: vec![
            artifact(main.join("SecurityConfig.java"), security_config_java(pkg)),
            artifact(
                main.join("ProductionSecurityConfig.java"),
                production_security_config_java(pkg),
            ),
            artifact(
                main.join("ScopeAuthorizer.java"),
                scope_authorizer_java(pkg),
            ),
            artifact(
                test.join("SecurityConfigTest.java"),
                security_test_java(pkg),
            ),
            artifact(
                test.join("ScopeAuthorizerTest.java"),
                scope_authorizer_test_java(pkg),
            ),
        ],
        properties: Vec::new(),
        ..Change::default()
    }
}

pub(crate) fn cors_slice(slice: &Slice) -> Change {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.root_package();
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    Change {
        deps: Vec::new(),
        files: vec![
            artifact(main.join("CorsConfig.java"), cors_config_java(pkg)),
            artifact(test.join("CorsConfigTest.java"), cors_config_test_java(pkg)),
        ],
        properties: vec![
            "# Exact browser origins; never use `*` together with credentials.".to_string(),
            "app.cors.allowed-origins=http://localhost:3000".to_string(),
        ],
        ..Change::default()
    }
}

fn cors_config_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/cors_config_java.java"),
        &[("pkg", pkg)],
    )
}

fn cors_config_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/cors_config_test_java.java"),
        &[("pkg", pkg)],
    )
}

fn security_config_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/security_config_java.java"),
        &[("pkg", pkg)],
    )
}

fn production_security_config_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/production_security_config_java.java"),
        &[("pkg", pkg)],
    )
}

fn security_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/security_test_java.java"),
        &[("pkg", pkg)],
    )
}

fn scope_authorizer_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/scope_authorizer_java.java"),
        &[("pkg", pkg)],
    )
}

fn scope_authorizer_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/scope_authorizer_test_java.java"),
        &[("pkg", pkg)],
    )
}

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
pub(crate) fn resource_service_java(pkg: &str, name: &str, extra: &str) -> String {
    let var = crate::generate::lower_first(name);
    crate::template::render(
        crate::template::template!("spring/resource_service_java.java"),
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
pub(crate) fn resource_controller_java(
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
        crate::template::template!("spring/resource_controller_java.java"),
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
        crate::template::template!("spring/scoped_resource_controller_java.java"),
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

/// The controller's test: a web-layer slice with the service replaced.
///
/// `@WebMvcTest` starts the web layer and nothing else -- no database, no
/// component scan of the whole application -- so it runs in a fraction of
/// the time a `@SpringBootTest` takes and fails for reasons that are about
/// HTTP. The service is a `@MockitoBean`, which is the current spelling:
/// `@MockBean` no longer exists in Spring Boot 4.
pub(crate) fn resource_controller_test_java(
    slice: &Slice,
    name: &str,
    extra: &str,
    fields: &[crate::generate::Field],
) -> String {
    let security: &str = slice.base();
    let pkg: &str = &slice.placed(Layer::Web);
    let webmvc_test_import: &str = slice.project().webmvc_test_import();
    if fields.iter().any(|field| field.constraints.scoped) {
        let guard_import = crate::generate::import_of(pkg, security, "ScopeAuthorizer");
        return crate::template::render(
            crate::template::template!("spring/resource_controller_test_scoped_java.java"),
            &[
                ("pkg", pkg),
                ("extra", extra),
                ("guard_import", &*guard_import),
                ("webmvc_test_import", webmvc_test_import),
                ("name", name),
            ],
        );
    }
    crate::template::render(
        crate::template::template!("spring/resource_controller_test_java.java"),
        &[
            ("pkg", pkg),
            ("extra", extra),
            ("webmvc_test_import", webmvc_test_import),
            ("name", name),
        ],
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
pub(crate) fn resource_service_test_java(pkg: &str, name: &str, extra: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/resource_service_test_java.java"),
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
pub(crate) fn in_memory_repository_java(
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
        crate::template::template!("spring/in_memory_repository_java.java"),
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
pub(crate) const FAILSAFE_ARTIFACT: &str = "maven-failsafe-plugin";

/// The Failsafe plugin, versioned for the project it is going into.
///
/// Versionless is correct under `spring-boot-starter-parent`, which manages
/// it, and a trap without one: Maven only *warns* about a versionless plugin
/// rather than refusing the pom, so it resolves whatever the running Maven
/// defaults to, which is not a decision jails should be making silently.
/// plan.md §8.1 names it as the quiet half of that defect.
pub(crate) fn failsafe_plugin(flavor: crate::pom::Flavor) -> &'static str {
    match flavor {
        crate::pom::Flavor::SpringBoot => FAILSAFE_PLUGIN,
        crate::pom::Flavor::PlainMaven => FAILSAFE_PLUGIN_PINNED,
    }
}

pub(crate) const FAILSAFE_PLUGIN_PINNED: &str = r#"<plugin>
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

pub(crate) const FAILSAFE_PLUGIN: &str = r#"<plugin>
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

pub(crate) const REDIS_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-data-redis",
    version: None,
    scope: None,
    optional: false,
};

/// Testcontainers' generic container, which is what Boot's Redis
/// `@ServiceConnection` factory matches on: it accepts any
/// `GenericContainer` whose image is one of the Redis images, rather than a
/// dedicated Redis container type.
pub(crate) const TESTCONTAINERS_CORE: Dependency = Dependency {
    group_id: "org.testcontainers",
    artifact_id: "testcontainers",
    version: Some("2.0.5"),
    scope: Some("test"),
    optional: false,
};

pub(crate) const REDIS_IMAGE: &str = "redis:7-alpine";

pub(crate) fn redis_slice(slice: &Slice) -> Change {
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
        crate::template::template!("spring/key_value_store_java.java"),
        &[("pkg", pkg)],
    )
}

fn key_value_store_it_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/key_value_store_it_java.java"),
        &[("pkg", pkg), ("REDIS_IMAGE", REDIS_IMAGE)],
    )
}

// ---------------------------------------------------------------------------
// `add observability` -- metrics that are attributable and scrapeable.
// ---------------------------------------------------------------------------

/// Version managed by the Spring Boot parent, which imports the Micrometer
/// BOM (verified in spring-boot-dependencies, not assumed).
pub(crate) const PROMETHEUS_REGISTRY: Dependency = Dependency {
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
pub(crate) fn observability_slice(slice: &Slice) -> Change {
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
        crate::template::template!("spring/app_metrics_java.java"),
        &[("pkg", pkg)],
    )
}

fn app_metrics_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/app_metrics_test_java.java"),
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
}

fn prometheus_scrape_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/prometheus_scrape_test_java.java"),
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
        crate::template::template!("spring/metrics_config_java.java"),
        &[("pkg", pkg), ("customizer_import", customizer_import)],
    )
}
