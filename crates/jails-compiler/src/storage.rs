//! Which storage dependencies this project's Spring Boot wants.
//!
//! Split out of `lib.rs` by secret when `Compiler::compile` grew past the
//! largest-module ceiling. Everything here answers one question -- *given the
//! Boot this project has, which artifacts and versions does `storage postgres`
//! need* -- and nothing else in the compiler asks it.
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
/// `audit.md` A2.1. These were four `BuildDependency { version: None, .. }`
/// values written inline in `compile`, which is correct under
/// `spring-boot-starter-parent` and **fatal** without one: Maven refuses to
/// read a pom whose dependency has no version and no parent managing it, so
/// every goal fails, `validate` included, and the project is left worse than
/// before the command ran. `CLAUDE.md` records that trap; the canonical path
/// had reintroduced it.
///
/// Two boundaries, both taken from `add/database.rs` where they were verified
/// against `deps/spring-boot` rather than recalled:
///
/// - Boot manages `flyway-database-postgresql` from **3.3**. Below that both
///   Flyway artifacts are pinned, and they are pinned *together* because they
///   must move together.
/// - `spring-boot-flyway` -- Flyway's auto-configuration, split out when Boot
///   4 broke `spring-boot-autoconfigure` into ~130 modules -- exists only from
///   **4.0**. Naming it below 4 asks for a jar that does not exist; omitting it
///   at 4 is worse than an error, because the migrations then never run and
///   nothing says so: no log line, and then `relation "..." does not exist`
///   from the first query, which reads like a broken migration rather than an
///   absent one.
pub(crate) fn storage_dependencies(spring_boot: Option<&str>) -> Vec<BuildDependency> {
    /// Both Flyway artifacts, pinned to one version. Verified in
    /// `add/database.rs`, which is the only other place this number lives.
    const FLYWAY_PIN: &str = "12.8.1";
    let version = boot_version(spring_boot);
    let managed_flyway = version.is_some_and(|version| version >= (3, 3));
    let flyway_version = (!managed_flyway).then(|| FLYWAY_PIN.to_string());
    let mut dependencies = vec![
        BuildDependency {
            group: "org.springframework.boot".to_string(),
            artifact: "spring-boot-starter-jdbc".to_string(),
            version: None,
            scope: DependencyScope::Compile,
        },
        BuildDependency {
            group: "org.postgresql".to_string(),
            artifact: "postgresql".to_string(),
            version: None,
            scope: DependencyScope::Runtime,
        },
        BuildDependency {
            group: "org.flywaydb".to_string(),
            artifact: "flyway-core".to_string(),
            version: flyway_version.clone(),
            scope: DependencyScope::Compile,
        },
        BuildDependency {
            group: "org.flywaydb".to_string(),
            artifact: "flyway-database-postgresql".to_string(),
            version: flyway_version,
            scope: DependencyScope::Runtime,
        },
    ];
    if version.is_some_and(|version| version >= (4, 0)) {
        dependencies.push(BuildDependency {
            group: "org.springframework.boot".to_string(),
            artifact: "spring-boot-flyway".to_string(),
            version: None,
            scope: DependencyScope::Compile,
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
