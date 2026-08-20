//! `add csv` and `add json`: the two formats a service actually meets.
//!
//! `json` is Jackson 3 (`tools.jackson`), and that is one artifact, not two:
//! java.time is in core databind in 3.x, so adding `jackson-datatype-jsr310`
//! drags the 2.x line in beside it and half the code ends up on a mapper
//! nobody configured.

use super::*;

// ---------------------------------------------------------------------------
// csv
// ---------------------------------------------------------------------------

/// Commons CSV renamed `Builder.build()` to `Builder.get()` in 1.13, so the
/// pinned version and the generated call have to move together.
pub(super) const COMMONS_CSV: Dependency = Dependency {
    group_id: "org.apache.commons",
    artifact_id: "commons-csv",
    version: Some("1.14.1"),
    scope: None,
    optional: false,
};

pub(super) fn csv_plan(
    root: &std::path::Path,
    pkg: &str,
    _flavor: Flavor,
    name: Option<&str>,
) -> Result<Plan> {
    let base = capitalize(name.unwrap_or("Csv"));
    let class = format!("{base}Reader");

    Ok(Plan {
        // Spring Boot's dependency management does not cover commons-csv, so
        // the version is pinned in both flavors.
        deps: vec![COMMONS_CSV],
        files: vec![
            NewFile {
                path: main_dir(root, pkg).join(format!("{class}.java")),
                contents: csv_reader_java(pkg, &class),
            },
            NewFile {
                path: test_dir(root, pkg).join(format!("{class}Test.java")),
                contents: csv_reader_test_java(pkg, &class),
            },
        ],
        ..Plan::default()
    })
}

pub(super) fn csv_reader_java(pkg: &str, class: &str) -> String {
    crate::template::render(include_str!("../../templates/add/csv_reader_java.java"), &[("pkg", pkg), ("class", class)])
}

pub(super) fn csv_reader_test_java(pkg: &str, class: &str) -> String {
    crate::template::render(include_str!("../../templates/add/csv_reader_test_java.java"), &[("pkg", pkg), ("class", class)])
}

// ---------------------------------------------------------------------------
// json
// ---------------------------------------------------------------------------

/// Jackson **3**, whose coordinates changed with the major version:
/// `tools.jackson.core`, not `com.fasterxml.jackson.core`.
///
/// This matters more than a version bump usually does. Spring Boot 4's web
/// starter already brings Jackson 3 in, so adding the 2.x artifact put *two
/// Jackson majors on one classpath* and generated a utility written against
/// the deprecated one. They do not conflict at the class level -- the
/// packages differ -- which is exactly why nothing complains and the wrong
/// mapper is used forever.
pub(super) const JACKSON_VERSION: &str = "3.0.1";

pub(super) const JACKSON: Dependency = Dependency {
    group_id: "tools.jackson.core",
    artifact_id: "jackson-databind",
    version: Some(JACKSON_VERSION),
    scope: None,
    optional: false,
};

/// Jackson 3 needs **no** `jackson-datatype-jsr310`: java.time support moved
/// into the core databind module, so the 2.x migration *deletes* a dependency
/// rather than adding one.
///
/// Kept as a constant so `remove json` can still unsplice it from a project
/// that jails wrote before the move.
pub(super) const JACKSON_JSR310: Dependency = Dependency {
    group_id: "com.fasterxml.jackson.datatype",
    artifact_id: "jackson-datatype-jsr310",
    version: Some("2.19.0"),
    scope: None,
    optional: false,
};

pub(super) fn json_plan(
    root: &std::path::Path,
    pkg: &str,
    flavor: Flavor,
    name: Option<&str>,
) -> Result<Plan> {
    let base = name.map(capitalize).unwrap_or_default();
    let class = format!("{base}Json");

    // Spring Boot's dependency management already pins Jackson (and the web
    // starter pulls it in transitively), so declaring a version here would
    // fight the parent pom.
    // One artifact, not two: Jackson 3 has java.time built in. On Spring the
    // version is left to the parent, which already manages Jackson 3.
    let deps = match flavor {
        Flavor::SpringBoot => vec![Dependency {
            version: None,
            ..JACKSON
        }],
        Flavor::PlainMaven => vec![JACKSON],
    };

    Ok(Plan {
        deps,
        legacy_deps: vec![JACKSON_JSR310, Dependency { group_id: "com.fasterxml.jackson.core", ..JACKSON }],
        files: vec![
            NewFile {
                path: main_dir(root, pkg).join(format!("{class}.java")),
                contents: json_java(pkg, &class),
            },
            NewFile {
                path: test_dir(root, pkg).join(format!("{class}Test.java")),
                contents: json_test_java(pkg, &class),
            },
        ],
        ..Plan::default()
    })
}

pub(super) fn json_java(pkg: &str, class: &str) -> String {
    crate::template::render(include_str!("../../templates/add/json_java.java"), &[("pkg", pkg), ("class", class)])
}

pub(super) fn json_test_java(pkg: &str, class: &str) -> String {
    crate::template::render(include_str!("../../templates/add/json_test_java.java"), &[("pkg", pkg), ("class", class)])
}

