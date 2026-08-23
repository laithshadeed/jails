//! Which Testcontainers modules jails depends on, and at what version.
//!
//! One place, because the artifact ids are a trap. **Testcontainers 2.0 renamed
//! every module** — `postgresql` became `testcontainers-postgresql`,
//! `junit-jupiter` became `testcontainers-junit-jupiter` — and the old names
//! still resolve from Maven Central at their old versions. So a stale name does
//! not fail to resolve; it pins an old major beside a new one, or, when nothing
//! manages it, makes Maven refuse to read the pom at all.
//!
//! Declaring `junit-jupiter` versionless is exactly that second failure, and it
//! is how these constants came to live together: `add mail` did it, every goal
//! failed with "'dependencies.dependency.version' is missing", and only the
//! real-toolchain tier saw it. A second copy of an artifact id is a second
//! place to get the rename wrong.
//!
//! The version is pinned rather than left to the Spring Boot parent because the
//! parent does not manage Testcontainers 2.x. `spring-boot-testcontainers` is
//! the exception and is versionless: that one *is* Boot's.

use super::*;

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

/// Testcontainers' JUnit 5 integration: `@Testcontainers` and `@Container`.
///
/// **`testcontainers-junit-jupiter`, not `junit-jupiter`.** Testcontainers 2.0
/// renamed every module, and the old name is not managed by anything the
/// Spring Boot parent imports -- a versionless declaration of it makes Maven
/// refuse to read the pom at all, `validate` included. Only the real-toolchain
/// tier catches that, which is where it was caught.
pub(crate) const TESTCONTAINERS_JUNIT: Dependency = Dependency {
    group_id: "org.testcontainers",
    artifact_id: "testcontainers-junit-jupiter",
    version: Some("2.0.5"),
    scope: Some("test"),
    optional: false,
};
