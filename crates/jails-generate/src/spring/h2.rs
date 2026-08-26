//! `add h2`: an in-process database with a browser console.
//!
//! missing.md §3's first half. It is the interview scaffold's database and a
//! common first choice, and before this the answer was "`add db` is PostgreSQL
//! and `add sqlite` is a different database, so hand-edit the pom".
//!
//! Three things this knows that a hand-edited pom gets wrong:
//!
//! - **The console is its own artifact.** Boot 4 split auto-configuration into
//!   ~130 modules and `H2ConsoleAutoConfiguration` moved into
//!   `spring-boot-h2console` (verified in `deps/spring-boot/module/`). Without
//!   it `spring.h2.console.enabled=true` is a property with nothing listening
//!   to it: no warning, no console. Same rule as `spring-boot-flyway` -- the
//!   technology jar and the auto-configuration jar are separate dependencies,
//!   and a capability shipping only the first ships something that does not
//!   run.
//! - **Tests must not share the application's database.** The application is
//!   file-backed, which is the point of choosing H2 here, so a suite that
//!   inherited that URL would write to the developer's working tree and would
//!   fail on H2's file lock the moment it ran while the server was up. The
//!   override goes in the test overlay, which is additive -- see
//!   `model::Change::test_properties`.
//! - **Raw SQL, so no exception translation.** Same reason `add db` disables
//!   it: JDBC auto-configuration registers a post-processor that CGLIB-proxies
//!   every `@Repository`, and that fails on a `final` class.

use super::*;
use crate::model::Change;

/// The driver. `version: None` throughout: `require_spring` has already
/// established a Boot parent, and Boot's own BOM manages H2 (2.4.240 at the
/// time of writing) -- a pinned version here would drift away from the console
/// module that has to agree with it.
const H2: Dependency = Dependency {
    group_id: "com.h2database",
    artifact_id: "h2",
    version: None,
    // `runtime`: nothing in the generated code names an `org.h2` type, and a
    // compile-scoped driver invites one to.
    scope: Some("runtime"),
    optional: false,
};

/// The console's auto-configuration, which is a *different artifact* from the
/// database. See the module docs.
///
/// **Boot 4 only.** The split that created it is Boot 4's; before that
/// `H2ConsoleAutoConfiguration` ships inside `spring-boot-autoconfigure`, which
/// every Boot project already has. Declaring it on a Boot 2 or 3 build is not
/// a redundant dependency, it is an artifact that does not exist at that
/// version -- so the build fails to resolve, and the failure names a coordinate
/// rather than the capability that asked for it.
const H2_CONSOLE: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-h2console",
    version: None,
    scope: None,
    optional: false,
};

/// Which spelling of the exception-translation switch this project answers to.
///
/// Renamed at 4.0.0, recorded as a `level: "error"` deprecation in
/// `spring-boot-persistence`'s `additional-spring-configuration-metadata.json`
/// under `deps/`. The pre-4 spelling on a Boot 4 project is rejected outright;
/// the Boot 4 spelling on an older one is worse, because nothing rejects it --
/// the property is simply unbound, exception translation stays on, and the
/// CGLIB proxy this exists to prevent is created anyway.
fn exception_translation_property(boot_major: u32) -> &'static str {
    match boot_major >= 4 {
        true => "spring.persistence.exceptiontranslation.enabled=false",
        false => "spring.dao.exceptiontranslation.enabled=false",
    }
}

/// Where the application's database lives, relative to the working directory.
///
/// Inside the project rather than `~`: the URL the migration this replaces
/// used was `jdbc:h2:file:~/minicom-spring-4`, which puts a project's data in
/// the developer's home directory where nothing cleans it up and two checkouts
/// of the same project silently share one database.
const FILE_URL: &str = "jdbc:h2:file:./data/app";

/// What the tests get instead.
///
/// `DB_CLOSE_DELAY=-1` keeps the in-memory database alive for the whole JVM:
/// without it H2 drops the database when the last connection closes, so a
/// schema applied at startup is gone by the second test and the failure reads
/// like a missing table.
const TEST_URL: &str = "jdbc:h2:mem:test;DB_CLOSE_DELAY=-1";

pub(crate) fn h2_slice(slice: &Slice) -> Change {
    let root: &Path = slice.project().root();
    let boot_major = slice.project().boot_major();
    let mut deps = vec![crate::add::SPRING_JDBC, H2];
    if boot_major >= 4 {
        deps.push(H2_CONSOLE);
    }
    // In `adapters`, not beside the application, because the test opens a
    // `java.sql.Connection` -- and `g scaffold` writes an ArchUnit rule saying
    // raw JDBC stays in `adapters`. Two first-party generators cannot
    // disagree about that: `jails add h2` on a project with a scaffold was a
    // red build, and the rule is right. A capability's *configuration* still
    // belongs beside the app; a test about the driver belongs where the
    // driver is allowed.
    let adapters = slice.placed(Layer::Adapters);
    Change {
        deps,
        files: vec![artifact(
            crate::generate::test_dir(root, &adapters).join("H2DatabaseTest.java"),
            h2_database_test_java(&adapters),
        )],
        properties: [
            vec![
                "# The application's own database, inside the project rather than the home"
                    .to_string(),
                "# directory -- two checkouts must not share one file.".to_string(),
                format!("spring.datasource.url={FILE_URL}"),
            ],
            console_note(boot_major),
            vec![
                "spring.h2.console.enabled=true".to_string(),
                "# Open http://localhost:8080/h2-console and connect with the URL above."
                    .to_string(),
                "spring.h2.console.path=/h2-console".to_string(),
                "# Raw SQL, no ORM -- so no CGLIB proxy around every @Repository.".to_string(),
                exception_translation_property(boot_major).to_string(),
            ],
        ]
        .concat(),
        test_properties: vec![
            "# Tests get their own in-memory database. Inheriting the file URL above".to_string(),
            "# would write into the working tree, and would fail on H2's file lock the".to_string(),
            "# moment the suite ran while the application was up.".to_string(),
            format!("spring.datasource.url={TEST_URL}"),
        ],
        ..Change::default()
    }
}

/// The comment jails writes above `spring.h2.console.enabled`.
///
/// It names the dependency that makes the property do something, and on a
/// pre-4 project that dependency is not `spring-boot-h2console` -- telling the
/// reader to add an artifact that does not exist at their version is the same
/// wrong answer as adding it.
fn console_note(boot_major: u32) -> Vec<String> {
    // One element per line: `Change::properties` is a list of *lines*, and a
    // `\n` inside one is refused by the writer rather than split.
    match boot_major >= 4 {
        true => vec![
            "# The console is an auto-configuration module, not the driver: without".to_string(),
            "# spring-boot-h2console on the classpath this property does nothing.".to_string(),
        ],
        false => vec![
            "# Boot auto-configures the console from spring-boot-autoconfigure at this".to_string(),
            "# version; the separate spring-boot-h2console module is Boot 4 and later.".to_string(),
        ],
    }
}

fn h2_database_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/h2_database_test_java.java"),
        // What the *driver* reports, which is not what was configured:
        // `DatabaseMetaData.getURL()` drops everything after the first `;`, so
        // asserting the full URL fails against a connection that is correct.
        // Derived from the one constant rather than written out again -- two
        // spellings of one URL is how a test comes to pass over a database
        // nobody configured.
        &[
            ("pkg", pkg),
            ("test_url", TEST_URL.split(';').next().unwrap()),
        ],
    )
}
