//! The rules every change that writes a test would otherwise have to remember.
//!
//! Not a route: a filter every route runs its planned [`Change`] through
//! before desiring it. The direct write path applied the same rules from
//! `write_new_file`/`add_in` rather than per recipe, for the reason the Java
//! shape rules live below every producer -- a rule twenty recipes have to
//! remember is a rule that decays, and the decay is silent. A generated `*IT`
//! with no Failsafe plugin does not fail; `mvn verify` completes, reports
//! success, and runs none of them.

use super::*;

/// The two things the write path adds to any change that writes tests.
///
/// A capability or a generator that emits a test emits it against AssertJ, and
/// one that emits an `*IT` needs Failsafe -- which is *not* in the Spring Boot
/// parent's default build, so without it `mvn verify` completes, reports
/// success and runs none of them. jails generated integration tests for months
/// that never ran once.
///
/// The direct write path applies both from `write_new_file`/`add_in` rather
/// than per recipe, for the same reason the Java shape rules live below every
/// producer: a rule twenty recipes have to remember is a rule that decays. So
/// every route applies them here, once, to whatever it is about to desire.
/// The Testcontainers configuration this project has, if it has one.
///
/// Found in the projection rather than on disk, so a config an earlier row of
/// the same transition wrote counts. Named by its file stem, which for Java is
/// always the top-level type.
struct ContainerConfig {
    package: String,
    class: String,
}

fn container_config(project: &Project) -> Option<ContainerConfig> {
    project
        .projected_test_sources()
        .iter()
        .find(|(path, _)| {
            path.file_stem()
                .is_some_and(|stem| stem == "TestcontainersConfig")
        })
        .map(|(_, source)| ContainerConfig {
            package: jails_java::java::package_of(source).unwrap_or_default(),
            class: "TestcontainersConfig".to_string(),
        })
}

pub fn with_test_support(project: &Project, mut change: Change) -> Change {
    // A `@SpringBootTest` **this change writes** carries the container import
    // from birth, when the project already has a container config.
    //
    // The alternative is what V1 did: `add db` splices its import into every
    // `@SpringBootTest` on disk, and a second reconciliation pass catches the
    // ones written after it. That needs the import to reach a file no change
    // owns, and here the row that owns a file decides its bytes -- so the
    // capability writing the test is the one that has to put the import in.
    // Without it the project comes out with a `@SpringBootTest` that has no
    // DataSource, and `mvn verify` fails on a test nobody wrote.
    if let Some(config) = container_config(project) {
        for file in &mut change.files {
            if !file.path.to_string_lossy().contains("src/test/java") {
                continue;
            }
            let stem = file
                .path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default();
            let annotated = jails_java::java::annotations(&file.contents)
                .into_iter()
                .any(|found| {
                    found.name == "SpringBootTest"
                        && found.target == jails_java::java::Target::Type(stem.to_string())
                });
            if !annotated {
                continue;
            }
            let package = jails_java::java::package_of(&file.contents).unwrap_or_default();
            let extra = match package == config.package {
                true => String::new(),
                false => format!("import {}.{};\n", config.package, config.class),
            };
            if let Some(spliced) =
                jails_java::annotate::splice_import(&file.contents, &config.class, &extra)
            {
                file.contents = spliced;
            }
        }
    }
    let writes = |suffix: &str| {
        change
            .files
            .iter()
            .any(|file| file.path.to_string_lossy().contains(suffix))
    };
    if writes("src/test/java")
        && project.lacks_dependency("org.assertj", "assertj-core")
        && !project.pom().contains("spring-boot-starter-test")
        && !project.pom().contains("spring-boot-starter-webmvc-test")
    {
        change
            .deps
            .push(jails_project::pom::assertj(project.flavor()));
    }
    if writes("IT.java") {
        change.plugins.push((
            jails_protocol::feature::BuildFeature::IntegrationTests,
            jails_generate::spring::failsafe_plugin(project.flavor()).to_string(),
        ));
    }
    // Boot 4 moved `@WebMvcTest` and `@AutoConfigureMockMvc` into a module
    // `spring-boot-starter-test` does not bring in, so a generated test that
    // uses either compiles only when this is declared. Applied here for the
    // same reason the two above are: a rule every recipe has to remember is
    // a rule that decays.
    if jails_generate::generate::writes_a_webmvc_test(&change.files)
        && project.lacks_dependency(
            "org.springframework.boot",
            "spring-boot-starter-webmvc-test",
        )
    {
        change.deps.push(jails_project::pom::WEBMVC_TEST_STARTER);
    }
    // A Spring project with compose services gets the module that starts them
    // for `spring-boot:run`. Same rule as the three above and the same reason
    // it lives here: the recipes that bring a service are `db`, `kafka`,
    // `redis` and `mail`, and a rule four of them have to remember is one that
    // decays. `add db` also writes `spring.docker.compose.enabled=false` on
    // this machine, and both are right: the property is a per-project answer
    // to a broken provider, and the dependency is what the property is *about*.
    if project.flavor() == jails_project::pom::Flavor::SpringBoot
        && !change.compose.is_empty()
        && project.lacks_dependency("org.springframework.boot", "spring-boot-docker-compose")
    {
        change.deps.push(jails_project::pom::SPRING_DOCKER_COMPOSE);
    }
    change
}
