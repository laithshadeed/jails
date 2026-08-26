//! Conservative source evidence for warm-engine eligibility.

use std::path::{Path, PathBuf};

#[derive(Debug, Default, Eq, PartialEq)]
pub(super) struct PartitionEvidence {
    pub eligible: Vec<String>,
    pub ineligible: Vec<(String, String)>,
    pub gaps: Vec<String>,
}

const FORK_SENSITIVE: &[(&str, &str)] = &[
    ("@SpringBootTest", "Spring application context"),
    ("@WebMvcTest", "Spring MVC context"),
    ("@DataJpaTest", "Spring data context"),
    ("@JdbcTest", "Spring JDBC context"),
    ("@ContextConfiguration", "Spring test context"),
    ("SpringExtension", "Spring test extension"),
    ("org.testcontainers", "Testcontainers"),
    ("@Testcontainers", "Testcontainers"),
    ("@Container", "container lifecycle"),
    ("@ServiceConnection", "service connection lifecycle"),
    ("System.setProperty", "global system properties"),
    ("System.clearProperty", "global system properties"),
    ("System.setOut", "global process output"),
    ("System.setErr", "global process output"),
    ("Locale.setDefault", "global locale"),
    ("TimeZone.setDefault", "global time zone"),
    ("addShutdownHook", "global shutdown hooks"),
    ("setDefaultUncaughtExceptionHandler", "global thread state"),
    ("System.load(", "native library loading"),
    ("System.loadLibrary(", "native library loading"),
    ("Mockito.mockStatic", "static mocking"),
    ("@ResourceLock", "shared global resource"),
    ("@Isolated", "explicit JUnit isolation"),
];

pub(super) fn partition_evidence(project: &Path, requested: &[String]) -> PartitionEvidence {
    let mut evidence = PartitionEvidence::default();
    if requested.is_empty() {
        for source in discover_tests(project) {
            match source {
                Ok(source) => match selector_for_source(project, &source) {
                    Some(selector) => classify(project, selector, source, &mut evidence),
                    None => evidence.gaps.push(format!(
                        "{} has no attributable Java test type\n       fix: use the build engine for this source layout",
                        source.display()
                    )),
                },
                Err(reason) => evidence.gaps.push(reason),
            }
        }
    } else {
        for selector in requested {
            match source_for(project, selector) {
                Some(source) => classify(project, selector.clone(), source, &mut evidence),
                None => evidence.ineligible.push((
                    selector.clone(),
                    format!(
                        "`{selector}` has no attributable test source\n       fix: pass its fully qualified test class or use the build engine"
                    ),
                )),
            }
        }
    }
    evidence.eligible.sort();
    evidence.eligible.dedup();
    evidence.ineligible.sort();
    evidence.ineligible.dedup();
    evidence.gaps.sort();
    evidence.gaps.dedup();
    evidence
}

fn classify(project: &Path, selector: String, source: PathBuf, evidence: &mut PartitionEvidence) {
    let label = source
        .strip_prefix(project)
        .unwrap_or(&source)
        .display()
        .to_string();
    if source
        .file_stem()
        .is_some_and(|name| name.to_string_lossy().ends_with("IT"))
    {
        evidence
            .ineligible
            .push((selector, format!("{label} is an integration test")));
        return;
    }
    let text = match std::fs::read_to_string(&source) {
        Ok(text) => text,
        Err(error) => {
            evidence
                .ineligible
                .push((selector, format!("{label} cannot be inspected ({error})")));
            return;
        }
    };
    if let Some((_, reason)) = FORK_SENSITIVE
        .iter()
        .find(|(needle, _)| text.contains(needle))
    {
        evidence
            .ineligible
            .push((selector, format!("{label} uses {reason}")));
    } else {
        evidence.eligible.push(selector);
    }
}

fn selector_for_source(project: &Path, source: &Path) -> Option<String> {
    let class = source.file_stem()?.to_str()?;
    let text = std::fs::read_to_string(source).ok()?;
    let package = text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("package ")
            .and_then(|rest| rest.strip_suffix(';'))
            .map(str::trim)
            .filter(|package| !package.is_empty())
    });
    let selector =
        package.map_or_else(|| class.to_string(), |package| format!("{package}.{class}"));
    let expected = project
        .join("src/test/java")
        .join(format!("{}.java", selector.replace('.', "/")));
    (expected == source).then_some(selector)
}

fn discover_tests(project: &Path) -> Vec<Result<PathBuf, String>> {
    let root = project.join("src/test/java");
    let mut stack = vec![root.clone()];
    let mut found = Vec::new();
    while let Some(directory) = stack.pop() {
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                found.push(Err(format!(
                    "{} cannot be inspected ({error})\n       fix: restore readable test sources or use the build engine",
                    directory.display()
                )));
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    found.push(Err(format!(
                        "{} has an unreadable entry ({error})\n       fix: restore readable test sources or use the build engine",
                        directory.display()
                    )));
                    continue;
                }
            };
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind)
                    if kind.is_file()
                        && path
                            .extension()
                            .is_some_and(|extension| extension == "java") =>
                {
                    match std::fs::read_to_string(&path) {
                        Ok(text)
                            if text.contains("@Test")
                                || text.contains("org.junit")
                                || text.contains("org.testng") =>
                        {
                            found.push(Ok(path));
                        }
                        Ok(_) => {}
                        Err(error) => found.push(Err(format!(
                            "{} cannot be inspected ({error})\n       fix: restore readable test sources or use the build engine",
                            path.display()
                        ))),
                    }
                }
                Ok(kind) if kind.is_symlink() => found.push(Err(format!(
                    "{} is a symlink and its test source is not attributable\n       fix: replace it with a project-owned source or use the build engine",
                    path.display()
                ))),
                Ok(_) => {}
                Err(error) => found.push(Err(format!(
                    "{} cannot be classified ({error})\n       fix: restore readable test sources or use the build engine",
                    path.display()
                ))),
            }
        }
    }
    if found.is_empty() && !root.exists() {
        found.push(Err("src/test/java is absent, so the test universe is unknown\n       fix: use the build engine for this project layout".into()));
    }
    found.sort_by(|left, right| format!("{left:?}").cmp(&format!("{right:?}")));
    found
}

fn source_for(project: &Path, selector: &str) -> Option<PathBuf> {
    let class = selector
        .split_once('#')
        .map_or(selector, |(class, _)| class)
        .split('$')
        .next()
        .unwrap_or(selector);
    if class.contains('.') {
        let exact = project
            .join("src/test/java")
            .join(format!("{}.java", class.replace('.', "/")));
        return exact
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.is_file())
            .then_some(exact);
    }
    let file = format!("{class}.java");
    let mut stack = vec![project.join("src/test/java")];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind)
                    if kind.is_file()
                        && path.file_name().is_some_and(|name| name == file.as_str()) =>
                {
                    return Some(path);
                }
                _ => {}
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(body: &str, name: &str) -> (jails_support::scratch::ScratchDir, String) {
        let project = jails_support::scratch::ScratchDir::in_temp("test-isolation").unwrap();
        let path = project
            .path()
            .join("src/test/java/com/example")
            .join(format!("{name}.java"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, body).unwrap();
        (project, format!("com.example.{name}"))
    }

    #[test]
    fn a_plain_unit_test_is_warm_eligible() {
        let (project, selector) = source(
            "import org.junit.Test; class PlainTest { @Test void ok() {} }",
            "PlainTest",
        );
        assert!(
            partition_evidence(project.path(), &[selector])
                .ineligible
                .is_empty()
        );
    }

    #[test]
    fn spring_containers_integration_and_global_state_are_not_warm_eligible() {
        for (name, body, expected) in [
            (
                "ContextTest",
                "@SpringBootTest class ContextTest {}",
                "Spring application context",
            ),
            (
                "ContainerTest",
                "import org.testcontainers.Container; class ContainerTest {}",
                "Testcontainers",
            ),
            ("DatabaseIT", "class DatabaseIT {}", "integration test"),
            (
                "GlobalTest",
                "class GlobalTest { void x(){ System.setProperty(\"x\", \"y\"); } }",
                "global system properties",
            ),
        ] {
            let (project, selector) = source(body, name);
            let reasons = partition_evidence(project.path(), &[selector])
                .ineligible
                .into_iter()
                .map(|(_, reason)| reason)
                .collect::<Vec<_>>();
            assert!(
                reasons.iter().any(|reason| reason.contains(expected)),
                "{reasons:?}"
            );
        }
    }

    #[test]
    fn an_unknown_selector_is_not_assumed_safe() {
        let project = jails_support::scratch::ScratchDir::in_temp("test-isolation").unwrap();
        assert!(
            partition_evidence(project.path(), &["MissingTest".into()]).ineligible[0]
                .1
                .contains("no attributable")
        );
    }
}
