//! `jails db` against an in-process H2 database, in a shell or in a browser.
//!
//! `psql` and `sqlite3` are programs on PATH; H2's clients are *classes*, in a
//! jar the project already depends on. So this resolves the runtime classpath
//! the same way `jails console` resolves it for JShell, and runs
//! `org.h2.tools.Shell` or `org.h2.tools.Server` out of it. Nothing is
//! installed and nothing is downloaded: if the project can run, its console
//! can run.
//!
//! Two things this knows that a hand-written `java -cp ...` gets wrong.
//!
//! **A file-backed H2 is locked by whoever opened it first.** Without
//! `AUTO_SERVER=TRUE` in the URL, a console started while the application is
//! up is refused with `Database may be already in use`, and one started
//! *before* the application locks the file out from under it. `add h2` puts
//! `AUTO_SERVER=TRUE` in the URL it writes for exactly this, and a project
//! that lacks it is told so rather than handed H2's message, which names a
//! lock file and not the reason.
//!
//! **`AUTO_SERVER=TRUE` and `DB_CLOSE_ON_EXIT=FALSE` cannot both be set.** H2
//! refuses the pair outright -- `Database.java:282` in `deps/h2database`
//! throws `getUnsupportedException("AUTO_SERVER=TRUE && DB_CLOSE_ON_EXIT=
//! FALSE")` -- so a reader who adds the first to get a console breaks startup
//! if the second is already there. `doctor` reports that pair; the note here
//! refuses to recommend making it.

use super::{joined_classpath, run};
use jails_support::Result;
use std::process::Command;

/// The two clients, and what each is for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Client {
    /// `org.h2.tools.Shell` -- a SQL prompt on this terminal, like `psql`.
    Shell,
    /// `org.h2.tools.Server -web` -- H2's own browser console, which works
    /// whether or not the application is running. Deliberately not Spring's
    /// `/h2-console`: that one exists only while the application is up, and
    /// "open a console" should not first require "start the app".
    Web,
}

/// Read one `key=value` out of a properties file.
///
/// The `=` is matched explicitly rather than by prefix, so
/// `spring.datasource.url-shadow` is not `spring.datasource.url` -- and
/// whitespace is allowed *around* it, because `key = value` is a properties
/// file and a reader writes one; a hand-spaced line read as absent would send
/// `jails db` to PostgreSQL on an H2 project.
fn property<'a>(properties: &'a str, key: &str) -> Option<&'a str> {
    properties
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#') && !line.starts_with('!'))
        .find_map(|line| {
            line.strip_prefix(key)?
                .trim_start()
                .strip_prefix('=')
                .map(str::trim)
        })
}

fn application_properties(project: &crate::project::Project) -> String {
    std::fs::read_to_string(
        project
            .root()
            .join("src/main/resources/application.properties"),
    )
    .unwrap_or_default()
}

/// The datasource this project declares, if it is an H2 one.
///
/// Read from `application.properties`, which is where `add h2` writes it and
/// where a reader would look. A URL assembled at run time from an environment
/// variable is invisible here, which is the same limit `jails routes` states
/// about a path built at run time.
pub(crate) fn declared_url(project: &crate::project::Project) -> Option<String> {
    let properties = application_properties(project);
    let url = property(&properties, "spring.datasource.url")?;
    url.starts_with("jdbc:h2:").then(|| url.to_string())
}

/// Open the requested client against the project's declared H2 database.
pub fn open(
    project: &crate::project::Project,
    url: &str,
    client: Client,
    args: &[String],
    debug: bool,
) -> Result<()> {
    // A file-backed database with no `AUTO_SERVER` cannot be shared, and the
    // message H2 gives for that names a lock file rather than the cause. Said
    // up front, with the exact line to add, because otherwise the reader
    // discovers it only when the application happens to be up.
    if url.contains("jdbc:h2:file:") && !url.to_ascii_uppercase().contains("AUTO_SERVER=TRUE") {
        println!(
            "note: this database has no `AUTO_SERVER=TRUE`, so only one process can hold it.\n      \
             If the application is running, this console is refused; if it is not, starting\n      \
             the application while this console is open fails instead.\n      \
             fix: add `;AUTO_SERVER=TRUE` to `spring.datasource.url`. Do not also set\n           \
             `DB_CLOSE_ON_EXIT=FALSE` -- H2 refuses that pair outright."
        );
    }
    let properties = application_properties(project);
    let user = property(&properties, "spring.datasource.username").unwrap_or("sa");
    let password = property(&properties, "spring.datasource.password").unwrap_or("");

    let java = run::selected_java(project, debug)?;
    let classpath = joined_classpath(&run::runtime_classpath(
        project,
        crate::run::RunCompile::Auto,
        debug,
    )?)?;
    let mut cmd = Command::new(java);
    cmd.arg("-cp").arg(&classpath).current_dir(project.root());
    match client {
        Client::Shell => {
            cmd.args(["org.h2.tools.Shell", "-url", url, "-user", user]);
            // `-password ""` is not the same as omitting the flag: H2 prompts
            // when it is absent, which is right when the project declares no
            // password rather than declares an empty one.
            if !password.is_empty() {
                cmd.args(["-password", password]);
            }
        }
        Client::Web => {
            // `-webAllowOthers` is deliberately absent: it opens the console
            // to the network, and this is a convenience on one machine.
            cmd.args(["org.h2.tools.Server", "-web", "-browser"]);
            println!(
                "jails: starting H2's own web console. Connect with\n         url:  {url}\n         \
                 user: {user}\n       Ctrl-C stops it."
            );
        }
    }
    cmd.args(args);
    run::run_inherited(cmd, debug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_property_is_read_past_comments_and_whitespace() {
        let properties = "\
# spring.datasource.url=jdbc:h2:file:./commented-out
spring.datasource.url = jdbc:h2:file:./data/app;AUTO_SERVER=TRUE
spring.datasource.username=sa
";
        assert_eq!(
            property(properties, "spring.datasource.url"),
            Some("jdbc:h2:file:./data/app;AUTO_SERVER=TRUE")
        );
        assert_eq!(
            property(properties, "spring.datasource.username"),
            Some("sa")
        );
        assert_eq!(property(properties, "spring.datasource.password"), None);
    }

    /// A key that is a *prefix* of another is not that other one.
    #[test]
    fn a_longer_key_is_not_the_one_asked_for() {
        let properties = "spring.datasource.url-shadow=nope\nspring.datasource.url=jdbc:h2:mem:t\n";
        assert_eq!(
            property(properties, "spring.datasource.url"),
            Some("jdbc:h2:mem:t")
        );
    }
}
