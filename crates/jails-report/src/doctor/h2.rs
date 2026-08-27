//! H2's URL grammar, and the two spellings of it that go wrong.
//!
//! Split from `wiring.rs` by subject: that module asks whether a capability is
//! wired up, and this one reads one property against facts about H2 that are
//! in `deps/h2database` rather than in the project.

use super::{Check, Project, Status, wiring::property_value};

/// The H2 URL combinations that do not start, and the one that cannot be
/// inspected.
///
/// Both are read off `spring.datasource.url`, which is where a reader puts
/// them and where `add h2` writes them.
///
/// The first is a hard failure and not a matter of taste: `Database.java:282`
/// in `deps/h2database` throws
/// `getUnsupportedException("AUTO_SERVER=TRUE && DB_CLOSE_ON_EXIT=FALSE")`, so
/// the application dies at startup with `JdbcSQLFeatureNotSupportedException:
/// Feature not supported` -- a message naming neither property. It is easy to
/// arrive at honestly: `AUTO_SERVER=TRUE` is what a reader adds to get a
/// console, and `DB_CLOSE_ON_EXIT=FALSE` is what a tutorial adds to stop H2
/// dropping the database.
///
/// The second is a warning rather than a failure, because the project works
/// -- it just cannot be looked at while it runs.
pub(super) fn checks(project: &Project) -> Vec<Check> {
    let properties = std::fs::read_to_string(
        project
            .root()
            .join("src/main/resources/application.properties"),
    )
    .unwrap_or_default();
    let Some(url) = property_value(&properties, "spring.datasource.url") else {
        return Vec::new();
    };
    if !url.starts_with("jdbc:h2:") {
        return Vec::new();
    }
    let upper = url.to_ascii_uppercase();
    let auto_server = upper.contains("AUTO_SERVER=TRUE");
    if auto_server && upper.contains("DB_CLOSE_ON_EXIT=FALSE") {
        return vec![
            Check::new(
                Status::Fail,
                "h2 url",
                "AUTO_SERVER=TRUE and DB_CLOSE_ON_EXIT=FALSE together: H2 refuses this pair, so \
                 the application dies at startup with `Feature not supported`",
            )
            .fix(
                "drop `DB_CLOSE_ON_EXIT=FALSE` from spring.datasource.url -- AUTO_SERVER is what \
                 keeps the database reachable, and it does not need the other",
            ),
        ];
    }
    if url.contains("jdbc:h2:file:") && !auto_server {
        return vec![
            Check::new(
                Status::Warn,
                "h2 url",
                "file-backed H2 without AUTO_SERVER=TRUE: whichever process opens it first takes \
                 an exclusive lock",
            )
            .fix(
                "add `;AUTO_SERVER=TRUE` so `jails db` can attach while the application runs \
                 (do not also set DB_CLOSE_ON_EXIT=FALSE)",
            ),
        ];
    }
    Vec::new()
}
