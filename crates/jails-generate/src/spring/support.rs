//! Where a generated integration test finds this project's container config.
//!
//! Its own file rather than a block in `spring.rs` because it is not about
//! that file's secret: `require_spring` is one precondition shared by the
//! capabilities, and this is a question three *generators* ask about a
//! project's test wiring.

/// Where this project's `TestcontainersConfig` is, and what a generated
/// integration test says when there is not one.
///
/// **A generator that emits code must supply what that code needs, and when it
/// cannot it must degrade rather than hand the reader a compile error for a
/// file they did not write.** `g scaffold`'s JDBC round trip already did this
/// -- absent a container config it emits `@Disabled` naming `jails add db`.
/// `g usecase --on-conflict` and `g presence` did not: both wrote
/// `@Import(TestcontainersConfig.class)` unconditionally, so on any project
/// that runs against a real database of its own -- an H2 file, a shared
/// server, anything that is not Testcontainers -- `./gradlew build` stopped on
/// `cannot find symbol: class TestcontainersConfig` in a test jails itself had
/// just written.
///
/// **The projection, not the directory**, for the reason
/// `jdbc_repository_test_for` records: in an `app apply` the whole manifest is
/// one transition, so `add db`'s config is not on disk yet when a use case
/// that needs it plans. The package is read off the file rather than assumed
/// to be the base one, so a project that put it elsewhere still imports it.
pub(crate) struct TestSupport {
    /// The two imports `@Import(TestcontainersConfig.class)` needs, or empty.
    /// Both, because Spring's `@Import` is used for nothing else in these
    /// templates and an import with no annotation under it is what
    /// `add format` reports.
    pub import: String,
    /// `@Import(TestcontainersConfig.class)\n`, or empty.
    pub annotation: String,
    /// `import org.junit.jupiter.api.Disabled;\n`, or empty.
    pub disabled_import: &'static str,
    /// `@Disabled("…")\n`, or empty.
    pub disabled: &'static str,
}

impl TestSupport {
    /// What an integration test in `pkg` needs to reach a container config.
    pub(crate) fn resolve(project: &crate::model::Project, pkg: &str) -> Self {
        let Some(config_pkg) = project
            .projected_test_sources()
            .iter()
            .find(|(path, _)| {
                path.file_stem().and_then(|stem| stem.to_str()) == Some("TestcontainersConfig")
            })
            .and_then(|(_, source)| jails_java::java::package_of(source))
        else {
            return Self {
                import: String::new(),
                annotation: String::new(),
                disabled_import: "import org.junit.jupiter.api.Disabled;\n",
                disabled: "@Disabled(\"todo: run jails add db to generate TestcontainersConfig, \
                           or point this at the database this project already has\")\n",
            };
        };
        Self {
            import: format!(
                "{}import org.springframework.context.annotation.Import;\n",
                crate::generate::import_of(pkg, &config_pkg, "TestcontainersConfig")
            ),
            annotation: "@Import(TestcontainersConfig.class)\n".to_string(),
            disabled_import: "",
            disabled: "",
        }
    }
}
