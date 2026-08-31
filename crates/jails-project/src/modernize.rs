//! `jails modernize`: the version facts a Spring Boot project carries, moved
//! to the ones jails generates against.
//!
//! **Why this is a command rather than advice.** A Boot 2.7 project on JDK 21
//! is not one edit away from Boot 4 on JDK 26; it is five, spread over three
//! files, and four of them fail in ways that name something other than the
//! cause. `sourceCompatibility = 21` fails Gradle *evaluation* with "unknown
//! property" before a task runs. A missing `useJUnitPlatform()` reports "the
//! test task did not discover any tests" rather than "your tests are JUnit 5".
//! `DATETIME` in `schema.sql` fails as a bean-creation error four `Caused by`
//! levels above the actual message. Every one of these was hit, in this order,
//! upgrading one real project -- and the sequence is the whole content of this
//! module.
//!
//! **What it will not do.** It changes the build, and it reports what the
//! upgrade breaks in code the reader owns. A Jackson 2 import is not rewritten
//! to Jackson 3: the package moved *and* the API changed
//! (`JsonMapper.builder()`, unchecked exceptions), so a mechanical rename
//! would produce something that still does not compile while looking like it
//! had been migrated. The same rule as everywhere else here -- answer exactly
//! or say so.
//!
//! **Planning is pure.** Nothing in this module reads a file or writes one; it
//! is handed the captured text and returns the bytes it would write, which is
//! what lets `--pretend` and the committed transition come from one function
//! rather than two that drift.

use crate::model::Project;
use jails_support::Result;

/// One file this upgrade rewrites, and the reasons.
///
/// The bytes travel in `model::Artifact` rather than in a field of their own:
/// abstract.md §4.1 found four shapes for "a file to write" and settled on
/// one, and a fifth invented here would be the same mistake with a newer date
/// on it. `Artifact::path` is **project-relative** here, because every
/// consumer of this plan wants a `ProjectPath`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub artifact: crate::model::Artifact,
    /// One line per change made to this file, in the order they were applied.
    pub what: Vec<String>,
}

/// Something the upgrade breaks that only the reader can decide about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub what: String,
    pub fix: String,
}

/// What `jails modernize` would do to this project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Upgrade {
    pub edits: Vec<Step>,
    pub findings: Vec<Finding>,
    /// The facts that were already current, so a run that changes nothing can
    /// say *what* was already right rather than just "nothing to do".
    pub current: Vec<String>,
}

/// The captured text this plan is made against.
///
/// Captured rather than read from disk for the reason every other route here
/// captures: a plan made against one `build.gradle` must not be committed
/// against another.
#[derive(Debug, Default)]
pub struct Sources {
    pub build: Option<(String, String)>,
    pub wrapper: Option<(String, String)>,
    pub sql: Vec<(String, String)>,
    pub java: Vec<(String, String)>,
}

/// Jakarta EE packages that moved out of `javax` at Spring Boot 3.
///
/// A closed list, and it has to be: `javax.sql.DataSource`,
/// `javax.crypto.Cipher` and `javax.net.ssl` are **JDK** packages that never
/// moved, so a rule matching `javax.` would report three false breaks on a
/// project that has none.
const MOVED_TO_JAKARTA: [&str; 8] = [
    "javax.servlet",
    "javax.persistence",
    "javax.validation",
    "javax.transaction",
    "javax.ws.rs",
    "javax.jms",
    "javax.mail",
    "javax.annotation.PostConstruct",
];

pub fn plan(project: &Project, sources: &Sources) -> Result<Upgrade> {
    let mut upgrade = Upgrade::default();
    match project.build() {
        jails_spec::build::Build::Gradle => gradle(sources, &mut upgrade),
        jails_spec::build::Build::Maven => maven(sources, &mut upgrade),
        other => {
            return Err(format!(
                "`jails modernize` upgrades a Maven or Gradle build, and this project's is \
                 {other:?}.\n       fix: nothing here is safe to guess at -- the versions to \
                 move to are Spring Boot {}, Java {}, Gradle {}.",
                crate::pom::TARGET_BOOT,
                crate::pom::TARGET_RELEASE,
                crate::gradle::TARGET_GRADLE
            )
            .into());
        }
    }
    h2_types(project, sources, &mut upgrade);
    source_breaks(sources, &mut upgrade);
    Ok(upgrade)
}

fn gradle(sources: &Sources, upgrade: &mut Upgrade) {
    if let Some((path, text)) = &sources.build {
        let mut edit = Step {
            artifact: crate::model::Artifact {
                kind: "build file",
                path: path.into(),
                contents: text.clone(),
            },
            what: Vec::new(),
        };
        let boot = crate::pom::TARGET_BOOT;
        match crate::gradle::boot_version(text) {
            Some(found) if found == boot => upgrade
                .current
                .push(format!("spring boot   already {boot}")),
            Some(found) => match crate::gradle::with_boot_version(&edit.artifact.contents, boot) {
                Some(next) => {
                    edit.artifact.contents = next;
                    edit.what.push(format!("spring boot   {found} -> {boot}"));
                }
                None => upgrade.findings.push(Finding {
                    what: format!(
                        "the Spring Boot plugin says {found}, and jails could not \
                                   locate that version as a literal to rewrite"
                    ),
                    fix: format!("set it to {boot} by hand"),
                }),
            },
            None => upgrade.findings.push(Finding {
                what: "no Spring Boot plugin version this build file states as a literal"
                    .to_string(),
                fix: format!(
                    "if the version comes from a version catalog, move it to {boot} there"
                ),
            }),
        }
        let release: u32 = crate::pom::TARGET_RELEASE.parse().unwrap_or(26);
        match crate::gradle::release_level(text) {
            Some(found) if found == release => {
                upgrade
                    .current
                    .push(format!("java release  already {found}"));
            }
            found => {
                if let Some(next) =
                    crate::gradle::with_release_level(&edit.artifact.contents, release)
                {
                    edit.artifact.contents = next;
                    let from = found.map_or("unstated".to_string(), |n| n.to_string());
                    edit.what.push(format!(
                        "java release  {from} -> {release}, as a toolchain -- Gradle 9 removed \
                         the project-level sourceCompatibility"
                    ));
                }
            }
        }
        if let Some(next) = crate::gradle::with_junit_platform(&edit.artifact.contents) {
            edit.artifact.contents = next;
            edit.what.push(
                "test task     useJUnitPlatform() added -- without it Gradle reports \"did not \
                 discover any tests\" and runs none"
                    .to_string(),
            );
        } else {
            upgrade
                .current
                .push("test task     already useJUnitPlatform()".to_string());
        }
        if !edit.what.is_empty() {
            upgrade.edits.push(edit);
        }
    }
    if let Some((path, text)) = &sources.wrapper {
        let target = crate::gradle::TARGET_GRADLE;
        match crate::gradle::wrapper_version(text) {
            Some(found) if found == target => upgrade
                .current
                .push(format!("gradle        already {target}")),
            Some(found) => {
                if let Some(contents) = crate::gradle::with_wrapper_version(text, target) {
                    upgrade.edits.push(Step {
                        artifact: crate::model::Artifact {
                            kind: "gradle wrapper",
                            path: path.into(),
                            contents,
                        },
                        what: vec![format!(
                            "gradle        {found} -> {target} -- {found} does not run on JDK \
                             {}",
                            crate::pom::TARGET_RELEASE
                        )],
                    });
                }
            }
            None => upgrade.findings.push(Finding {
                what: format!("the wrapper's distributionUrl is not one jails can read a version out of, so it was left alone ({path})"),
                fix: format!("point it at gradle-{target}, which is what supports JDK {}", crate::pom::TARGET_RELEASE),
            }),
        }
    }
}

fn maven(sources: &Sources, upgrade: &mut Upgrade) {
    let Some((path, text)) = &sources.build else {
        return;
    };
    let mut edit = Step {
        artifact: crate::model::Artifact {
            kind: "build file",
            path: path.into(),
            contents: text.clone(),
        },
        what: Vec::new(),
    };
    let boot = crate::pom::TARGET_BOOT;
    match crate::pom::spring_boot_version_of(text) {
        Some((major, minor)) => {
            match crate::pom::with_parent_version(&edit.artifact.contents, boot) {
                Some(next) => {
                    edit.artifact.contents = next;
                    edit.what
                        .push(format!("spring boot   {major}.{minor}.x -> {boot}"));
                }
                None => upgrade
                    .current
                    .push(format!("spring boot   already {boot}")),
            }
        }
        None => upgrade.findings.push(Finding {
            what: "no `spring-boot-starter-parent` version in this POM".to_string(),
            fix: format!(
                "a project that manages Boot through `spring-boot-dependencies` says its version \
                 somewhere jails does not look -- set it to {boot} there"
            ),
        }),
    }
    let release: u32 = crate::pom::TARGET_RELEASE.parse().unwrap_or(26);
    match crate::pom::release_level(text) {
        Some(found) if found == release => {
            upgrade
                .current
                .push(format!("java release  already {found}"));
        }
        found => match crate::pom::with_release_level(&edit.artifact.contents, release) {
            Some(next) => {
                edit.artifact.contents = next;
                let from = found.map_or("unstated".to_string(), |n| n.to_string());
                edit.what.push(format!("java release  {from} -> {release}"));
            }
            None => upgrade.findings.push(Finding {
                what: "this POM states no Java release of its own".to_string(),
                fix: format!(
                    "it inherits one from its parent; set `<java.version>{release}</java.version>` \
                     here if this project should decide"
                ),
            }),
        },
    }
    if !edit.what.is_empty() {
        upgrade.edits.push(edit);
    }
}

/// `DATETIME` in a Spring-initialised schema, which H2 2.x does not have.
///
/// Verified the hard way: H2 2.4.240 answers `Unknown data type: "DATETIME"`,
/// while the H2 that Boot 2.7 manages accepts it. The rewrite is exact rather than a guess
/// -- it is gated on H2 actually being this project's driver, and `timestamp`
/// is the type H2 documents in its place -- which is the same bargain
/// `Dialect::column_type` takes for `timestamptz`.
///
/// Flyway migrations under `db/migration` are deliberately out of scope: those
/// are applied-once history, and rewriting one that has already run changes a
/// checksum rather than a schema.
fn h2_types(project: &Project, sources: &Sources, upgrade: &mut Upgrade) {
    // `declares_dependency`, not `has_dependency`: the second reads the text
    // as XML whatever the build tool is, so on a Gradle project it answers a
    // confident "no" to every question. `Some(true)` is the only answer that
    // may reach a rewrite of somebody's DDL -- "cannot tell" leaves the file
    // alone, which is the whole rule this repository keeps for Gradle.
    if project.declares_dependency("com.h2database", "h2") != Some(true) {
        return;
    }
    for (path, text) in &sources.sql {
        let found = words(text)
            .into_iter()
            .filter(|(_, word)| word.eq_ignore_ascii_case("datetime"))
            .count();
        if found == 0 {
            continue;
        }
        upgrade.edits.push(Step {
            artifact: crate::model::Artifact {
                kind: "schema",
                path: path.into(),
                contents: rewritten(text),
            },
            what: vec![format!(
                "h2 types      datetime -> timestamp x{found} -- H2 2.x answers `Unknown data \
                 type: \"DATETIME\"`, and Boot 4 manages H2 2.x"
            )],
        });
    }
}

/// Every ASCII-word token and where it starts.
fn words(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            out.push((start, &text[start..i]));
        } else {
            i += 1;
        }
    }
    out
}

/// The same text with every whole-word `datetime` replaced, case preserved.
fn rewritten(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (index, word) in words(text) {
        if !word.eq_ignore_ascii_case("datetime") {
            continue;
        }
        out.push_str(&text[at..index]);
        out.push_str(if word.chars().all(|c| c.is_ascii_uppercase()) {
            "TIMESTAMP"
        } else {
            "timestamp"
        });
        at = index + word.len();
    }
    out.push_str(&text[at..]);
    out
}

/// The Jackson 2 names that are not a package rename away from Jackson 3.
///
/// Verified in `deps/jackson-databind`, which is Jackson 3: **zero**
/// occurrences of either under `src/main/java/tools/`. `JsonProcessingException`
/// became unchecked and moved, so a `throws`/`catch` naming it changes shape;
/// java.time is in core databind now, so `JavaTimeModule` and the `jsr310`
/// artifact are gone; `WRITE_DATES_AS_TIMESTAMPS` moved to `cfg.DateTimeFeature`.
///
/// A file touching any of these is reported and not rewritten. A file touching
/// none of them is a rename -- `ObjectMapper`, `JsonNode`, `ObjectNode` and
/// `new ObjectMapper()` all exist unchanged in 3.x
/// (`tools/jackson/databind/ObjectMapper.java:276`).
const JACKSON_3_CHANGED: [&str; 4] = [
    "JsonProcessingException",
    "JavaTimeModule",
    "WRITE_DATES_AS_TIMESTAMPS",
    "jsr310",
];

/// What the upgrade breaks in Java the reader owns.
fn source_breaks(sources: &Sources, upgrade: &mut Upgrade) {
    let mut jackson = Vec::new();
    let mut jakarta = Vec::new();
    for (path, text) in &sources.java {
        let blanked = jails_java::java::blanked(text);
        if blanked.contains("com.fasterxml.jackson") {
            // Split by whether the rename is the whole migration. Refusing
            // every file was too blunt: it left a project that jails had just
            // moved to Boot 4 unable to compile, over three imports whose
            // types exist in 3.x under the same names -- and the reader was
            // handed a paragraph instead of a working build.
            match JACKSON_3_CHANGED.iter().any(|name| blanked.contains(name)) {
                true => jackson.push(path.clone()),
                false => upgrade.edits.push(Step {
                    artifact: crate::model::Artifact {
                        kind: "jackson package",
                        path: path.into(),
                        contents: text.replace("com.fasterxml.jackson", "tools.jackson"),
                    },
                    what: vec![format!(
                        "jackson       com.fasterxml.jackson -> tools.jackson in {path} -- every \
                         type it names exists in 3.x under the same name"
                    )],
                }),
            }
        }
        if MOVED_TO_JAKARTA
            .iter()
            .any(|package| blanked.contains(package))
        {
            jakarta.push(path.clone());
        }
    }
    if !jackson.is_empty() {
        upgrade.findings.push(Finding {
            what: format!(
                "{} file(s) use Jackson 2 (`com.fasterxml.jackson`), which Spring Boot 4 does \
                 not ship: {}",
                jackson.len(),
                jackson.join(", ")
            ),
            fix: "Boot 4 manages Jackson 3 (`tools.jackson`), whose API differs -- \
                  `JsonMapper.builder().build()`, and no checked `JsonProcessingException`. \
                  jails does not rewrite these, because a package rename alone would leave \
                  code that still does not compile while looking migrated. `jails add json` \
                  writes a reader against the 3.x API."
                .to_string(),
        });
    }
    if !jakarta.is_empty() {
        upgrade.findings.push(Finding {
            what: format!(
                "{} file(s) import a `javax.*` package that moved to `jakarta.*` at Spring Boot \
                 3: {}",
                jakarta.len(),
                jakarta.join(", ")
            ),
            fix: "rename the import prefix `javax.` to `jakarta.` in those files -- the types \
                  and their methods are unchanged, only the package moved."
                .to_string(),
        });
    }
}
