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
const H2_CONSOLE: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-h2console",
    version: None,
    scope: None,
    optional: false,
};

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

pub fn h2_slice(slice: &Slice) -> Change {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.root_package();
    Change {
        deps: vec![crate::add::SPRING_JDBC, H2, H2_CONSOLE],
        files: vec![artifact(
            crate::generate::test_dir(root, pkg).join("H2DatabaseTest.java"),
            h2_database_test_java(pkg),
        )],
        properties: vec![
            "# The application's own database, inside the project rather than the home".to_string(),
            "# directory -- two checkouts must not share one file.".to_string(),
            format!("spring.datasource.url={FILE_URL}"),
            "# The console is an auto-configuration module, not the driver: without".to_string(),
            "# spring-boot-h2console on the classpath this property does nothing.".to_string(),
            "spring.h2.console.enabled=true".to_string(),
            "# Open http://localhost:8080/h2-console and connect with the URL above.".to_string(),
            "spring.h2.console.path=/h2-console".to_string(),
            "# Raw SQL, no ORM -- so no CGLIB proxy around every @Repository.".to_string(),
            "spring.persistence.exceptiontranslation.enabled=false".to_string(),
        ],
        test_properties: vec![
            "# Tests get their own in-memory database. Inheriting the file URL above".to_string(),
            "# would write into the working tree, and would fail on H2's file lock the".to_string(),
            "# moment the suite ran while the application was up.".to_string(),
            format!("spring.datasource.url={TEST_URL}"),
        ],
        ..Change::default()
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
