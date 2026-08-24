//! Which JUnit this project is on, and therefore which console launcher.
//!
//! Split out of `pom.rs` rather than living in it: the pom is where the answer
//! is *read*, but the question is about JUnit's own versioning scheme, and
//! that scheme has already changed once inside the range jails supports.
//!
//! **The console launcher's version must equal the project's own JUnit
//! version**, and getting it wrong does not fail to resolve -- it fails at
//! *run* time with a `NoSuchMethodError` wrapped in "the versions of JUnit
//! jars on the classpath are not properly aligned". That is what a pinned
//! guess produced here on the first try.

use crate::pom::{Flavor, flavor};

/// The version the console launcher must be, for this project.
///
/// Confirmed in `deps/junit-framework`, not from memory: `junit-bom` constrains
/// **every** mavenized project to the single root `version` in
/// `gradle.properties`, so from JUnit 6 the jupiter and platform artifacts
/// share one number. Before that they did not -- jupiter `5.y.z` paired with
/// platform `1.y.z` -- which is the mapping below.
///
/// `None` means the pom manages it (a Spring Boot parent, or a `junit-bom`
/// import), and a version must then be **omitted**: a redundant one pins the
/// launcher while the BOM moves the engine, which is the same misalignment
/// arriving by a different route.
pub enum ConsoleVersion {
    Managed,
    Pinned(String),
    /// Nothing declares JUnit at all, so there is nothing to align with.
    Unknown,
}

pub fn console_version(pom: &str) -> ConsoleVersion {
    if matches!(flavor(pom), Flavor::SpringBoot) || pom.contains("junit-bom") {
        return ConsoleVersion::Managed;
    }
    let Some(declared) = declared_version(pom, "junit-jupiter") else {
        return ConsoleVersion::Unknown;
    };
    let mut parts = declared.split('.');
    let major: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    if major >= 6 {
        return ConsoleVersion::Pinned(declared);
    }
    // JUnit 5: platform `1.y.z` against jupiter `5.y.z`.
    let rest: Vec<&str> = parts.collect();
    if rest.is_empty() {
        return ConsoleVersion::Unknown;
    }
    ConsoleVersion::Pinned(format!("1.{}", rest.join(".")))
}

/// The `<version>` inside the `<dependency>` block declaring `artifact`.
fn declared_version(pom: &str, artifact: &str) -> Option<String> {
    let at = pom.find(&format!("<artifactId>{artifact}</artifactId>"))?;
    let rest = &pom[at..];
    let open = rest.find("<version>")?;
    let close = rest.find("</version>")?;
    if close < open {
        return None;
    }
    Some(rest[open + "<version>".len()..close].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug this pins cost a real run: a guessed `1.14.0` against a project
    /// on JUnit 6.1.2 resolved fine and then died with `NoSuchMethodError`.
    #[test]
    fn the_console_version_follows_the_projects_own_junit() {
        let junit6 = "<project><dependencies><dependency>\
                      <groupId>org.junit.jupiter</groupId>\
                      <artifactId>junit-jupiter</artifactId>\
                      <version>6.1.2</version></dependency></dependencies></project>";
        assert!(matches!(
            console_version(junit6),
            ConsoleVersion::Pinned(v) if v == "6.1.2"
        ));

        // JUnit 5 numbered them apart: jupiter 5.11.4 <-> platform 1.11.4.
        let junit5 = junit6.replace("6.1.2", "5.11.4");
        assert!(matches!(
            console_version(&junit5),
            ConsoleVersion::Pinned(v) if v == "1.11.4"
        ));

        // Managed: a redundant version would pin the launcher while the BOM
        // moves the engine -- the same misalignment by another route.
        assert!(matches!(
            console_version("<parent><artifactId>spring-boot-starter-parent</artifactId></parent>"),
            ConsoleVersion::Managed
        ));
        assert!(matches!(
            console_version("<project><artifactId>junit-bom</artifactId></project>"),
            ConsoleVersion::Managed
        ));

        // Nothing to align with is not a guess.
        assert!(matches!(
            console_version("<project><dependencies/></project>"),
            ConsoleVersion::Unknown
        ));
    }
}
