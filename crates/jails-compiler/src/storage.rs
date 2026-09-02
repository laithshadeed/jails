//! Which storage dependencies this project's Spring Boot wants.
//!
//! Everything here answers one question -- *given the Boot this project has,
//! which artifacts and versions does `storage postgres` need* -- and nothing
//! else in the compiler asks it.
//!
//! The rule the answers exist for: a `<dependency>` with no `<version>` is
//! correct under `spring-boot-starter-parent`, which manages it, and fatal
//! without one. Maven refuses to *read* such a pom at all, so every goal fails
//! including `validate`, and the project is left worse than before.

use jails_contracts::BuildDependency;
use jails_model::DependencyScope;

/// The build dependencies `storage postgres` needs, versioned for the Boot the
/// project actually has.
///
/// Two boundaries, both verified against `deps/spring-boot` rather than
/// recalled:
///
/// - Boot manages `flyway-database-postgresql` from **3.3**. Below that both
///   Flyway artifacts are pinned, and they are pinned *together* because they
///   must move together.
/// - `spring-boot-flyway` -- Flyway's auto-configuration, its own module from
///   Boot 4 -- exists only from **4.0**. Naming it below 4 asks for a jar that
///   does not exist; omitting it at 4 is worse than an error, because the
///   migrations then never run and nothing says so: no log line, and then
///   `relation "..." does not exist` from the first query, which reads like a
///   broken migration rather than an absent one.
pub(crate) fn storage_dependencies(spring_boot: Option<&str>) -> Vec<BuildDependency> {
    /// Both Flyway artifacts, pinned to one version.
    const FLYWAY_PIN: &str = "12.8.1";
    /// The driver, for a project with no parent managing it.
    const POSTGRES_PIN: &str = "42.7.11";
    let version = boot_version(spring_boot);
    let managed_flyway = version.is_some_and(|version| version >= (3, 3));
    let flyway_version = (!managed_flyway || spring_boot.is_none()).then(|| FLYWAY_PIN.to_string());
    // **A plain Maven project gets the same database and none of the Spring.**
    // `java.sql` is in the JDK, so the driver and Flyway are the whole of what
    // `storage postgres` needs there; the JDBC *starter* is Spring's and
    // naming it would drag in a framework the project did not ask for. Every
    // version is pinned for the reason this module exists: with no parent to
    // manage it, a versionless dependency makes Maven refuse to read the pom.
    let mut dependencies = vec![
        BuildDependency {
            group: "org.postgresql".to_string(),
            artifact: "postgresql".to_string(),
            version: spring_boot.is_none().then(|| POSTGRES_PIN.to_string()),
            scope: DependencyScope::Runtime,
            optional: false,
        },
        BuildDependency {
            group: "org.flywaydb".to_string(),
            artifact: "flyway-core".to_string(),
            version: flyway_version.clone(),
            scope: DependencyScope::Compile,
            optional: false,
        },
        BuildDependency {
            group: "org.flywaydb".to_string(),
            artifact: "flyway-database-postgresql".to_string(),
            version: flyway_version,
            scope: DependencyScope::Runtime,
            optional: false,
        },
    ];
    if spring_boot.is_some() {
        dependencies.push(BuildDependency {
            group: "org.springframework.boot".to_string(),
            artifact: "spring-boot-starter-jdbc".to_string(),
            version: None,
            scope: DependencyScope::Compile,
            optional: false,
        });
    }
    if version.is_some_and(|version| version >= (4, 0)) {
        dependencies.push(BuildDependency {
            group: "org.springframework.boot".to_string(),
            artifact: "spring-boot-flyway".to_string(),
            version: None,
            scope: DependencyScope::Compile,
            optional: false,
        });
    }
    dependencies
}

/// The captured Spring Boot version as `(major, minor)`.
///
/// `emit_capability::boot_major` is not enough here: both boundaries above
/// are minor-version boundaries, and rounding 3.1 and 3.3 together would
/// either pin what Boot already manages or leave unmanaged what it does not.
fn boot_version(version: Option<&str>) -> Option<(u32, u32)> {
    let mut parts = version?.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().and_then(|minor| minor.parse().ok())?;
    Some((major, minor))
}
