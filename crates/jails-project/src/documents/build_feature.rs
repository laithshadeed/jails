//! Lossless rendering of semantic build features into reader-owned builds.

use super::{owned_block, pom, replace_owned_block};
use jails_contracts::BuildFeature;
use jails_model::Diagnostic;
use std::collections::BTreeSet;

const INTEGRATION_TESTS_MARKER: &str = "jails:integration-tests";
const COVERAGE_MARKER: &str = "jails:coverage";
const FORMATTING_MARKER: &str = "jails:formatting";

pub fn reconcile_maven_build_features(
    text: &str,
    features: &BTreeSet<BuildFeature>,
    managed_versions: bool,
) -> Result<String, Diagnostic> {
    let text = reconcile_maven_integration_tests(
        text,
        features.contains(&BuildFeature::IntegrationTests),
        managed_versions,
    )?;
    let text = reconcile_maven_coverage(&text, features.contains(&BuildFeature::Coverage))?;
    reconcile_maven_formatting(&text, features.contains(&BuildFeature::Formatting))
}

pub fn reconcile_gradle_build_features(
    text: &str,
    features: &BTreeSet<BuildFeature>,
    kotlin: bool,
) -> Result<String, Diagnostic> {
    let text = reconcile_gradle_integration_tests(
        text,
        features.contains(&BuildFeature::IntegrationTests),
        kotlin,
    )?;
    let text =
        reconcile_gradle_coverage(&text, features.contains(&BuildFeature::Coverage), kotlin)?;
    if features.contains(&BuildFeature::Formatting) {
        // Not a silent skip. Spotless needs an `id ... version ...` entry in
        // `plugins {}`, which is only legal as the *first* statement of the
        // script -- and this backend's whole contract is that it appends a
        // marked block and touches nothing else. Guessing where the top of
        // somebody's build file is produces a script that no longer evaluates,
        // which is worse than a capability that says it cannot.
        return Err(Diagnostic::new(
            "workspace-gradle-formatting-unsupported",
            super::BUILD_SUBJECT,
            "the Gradle adapter cannot install formatting: Spotless needs `id 'com.diffplug.spotless'` inside `plugins { }`, which must be the first statement in the script.",
            "add the plugin entry yourself and configure `spotless { }`, or keep formatting outside the model",
        ));
    }
    Ok(text)
}

fn reconcile_maven_coverage(text: &str, enabled: bool) -> Result<String, Diagnostic> {
    let open = format!("<!-- {COVERAGE_MARKER} -->");
    let close = format!("<!-- /{COVERAGE_MARKER} -->");
    let expected = maven_feature_blocks(COVERAGE_MARKER, maven_coverage_plugin());
    if let Some(existing) = owned_block(text, &open, &close)? {
        refuse_edited_feature_any(existing, &expected, "Maven coverage", COVERAGE_MARKER)?;
        return if enabled {
            Ok(text.to_string())
        } else {
            Ok(replace_owned_block(text, &open, &close, None)?.expect("the owned block was found"))
        };
    }
    if !enabled {
        return Ok(text.to_string());
    }
    if pom::has_plugin(text, "jacoco-maven-plugin") {
        return Err(reader_owned_feature(
            format!(
                "Maven already configures `jacoco-maven-plugin` outside `<!-- {COVERAGE_MARKER} -->`"
            ),
            "remove the reader-owned duplicate or keep coverage outside the model",
        ));
    }
    insert_maven_feature_plugin(text, COVERAGE_MARKER, maven_coverage_plugin())
}

/// Spotless, keyed on `BuildFeature::Formatting`.
///
/// The plugin is pinned and so is the formatter under it: a formatter that
/// drifts version rewrites files nobody touched, and the diff blames whoever
/// happened to run the build.
fn reconcile_maven_formatting(text: &str, enabled: bool) -> Result<String, Diagnostic> {
    let open = format!("<!-- {FORMATTING_MARKER} -->");
    let close = format!("<!-- /{FORMATTING_MARKER} -->");
    let expected = maven_feature_blocks(FORMATTING_MARKER, maven_formatting_plugin());
    if let Some(existing) = owned_block(text, &open, &close)? {
        refuse_edited_feature_any(existing, &expected, "Maven formatting", FORMATTING_MARKER)?;
        return if enabled {
            Ok(text.to_string())
        } else {
            Ok(replace_owned_block(text, &open, &close, None)?.expect("the owned block was found"))
        };
    }
    if !enabled {
        return Ok(text.to_string());
    }
    if pom::has_plugin(text, "spotless-maven-plugin") {
        return Err(reader_owned_feature(
            format!(
                "Maven already configures `spotless-maven-plugin` outside `<!-- {FORMATTING_MARKER} -->`"
            ),
            "remove the reader-owned duplicate or keep formatting outside the model",
        ));
    }
    insert_maven_feature_plugin(text, FORMATTING_MARKER, maven_formatting_plugin())
}

/// palantir-java-format over google-java-format: it keeps a 120-column line,
/// which the generated code -- records with several components, fluent AssertJ
/// chains -- reads far better at than 100.
fn maven_formatting_plugin() -> &'static str {
    "<plugin>\n    <groupId>com.diffplug.spotless</groupId>\n    <artifactId>spotless-maven-plugin</artifactId>\n    <version>3.9.0</version>\n    <configuration>\n        <java>\n            <palantirJavaFormat>\n                <version>2.97.0</version>\n            </palantirJavaFormat>\n            <removeUnusedImports/>\n        </java>\n    </configuration>\n    <executions>\n        <execution>\n            <id>spotless-check</id>\n            <phase>verify</phase>\n            <goals>\n                <goal>check</goal>\n            </goals>\n        </execution>\n    </executions>\n</plugin>\n"
}

fn maven_coverage_plugin() -> &'static str {
    "<plugin>\n    <groupId>org.jacoco</groupId>\n    <artifactId>jacoco-maven-plugin</artifactId>\n    <version>0.8.15</version>\n    <executions>\n        <execution>\n            <id>coverage-agent</id>\n            <goals>\n                <goal>prepare-agent</goal>\n            </goals>\n        </execution>\n        <execution>\n            <id>coverage-report-and-check</id>\n            <phase>verify</phase>\n            <goals>\n                <goal>report</goal>\n                <goal>check</goal>\n            </goals>\n            <configuration>\n                <rules>\n                    <rule>\n                        <element>BUNDLE</element>\n                        <limits>\n                            <limit>\n                                <counter>LINE</counter>\n                                <value>COVEREDRATIO</value>\n                                <minimum>0.80</minimum>\n                            </limit>\n                        </limits>\n                    </rule>\n                </rules>\n            </configuration>\n        </execution>\n    </executions>\n</plugin>\n"
}

/// Every shape an owned feature block has ever been written in, so a block an
/// older jails wrote is still recognised as jails' own rather than as a
/// reader's edit.
fn maven_feature_blocks(marker: &str, plugin: &str) -> Vec<String> {
    let mut shapes = vec![marked_maven_feature(marker, plugin)];
    shapes.extend(
        pom::plugin_nest(plugin)
            .into_iter()
            .skip(1)
            .map(|shape| marked_maven_feature(marker, &shape)),
    );
    shapes.extend(
        pom::plugin_nest(&marked_maven_feature(marker, plugin))
            .into_iter()
            .skip(1),
    );
    shapes
}

fn marked_maven_feature(marker: &str, body: &str) -> String {
    format!("<!-- {marker} -->\n{body}<!-- /{marker} -->\n")
}

/// The marker goes *inside* whatever containers this has to create.
///
/// What jails owns is one `<plugin>`; wrapping `<build>` or `<plugins>` in the
/// marker as well claims a container the reader shares -- and the next command
/// to add a plugin legitimately writes inside it, which reads back as "the
/// owned block was edited" and refuses every later plan, which is what `add
/// coverage` then `add fake` would do on a pom with no `<build>`.
fn insert_maven_feature_plugin(
    text: &str,
    marker: &str,
    plugin: &str,
) -> Result<String, Diagnostic> {
    pom::insert_plugin(
        text,
        &pom::plugin_nest(&marked_maven_feature(marker, plugin)),
    )
}

fn reconcile_gradle_coverage(
    text: &str,
    enabled: bool,
    kotlin: bool,
) -> Result<String, Diagnostic> {
    let open = format!("// {COVERAGE_MARKER}");
    let close = format!("// /{COVERAGE_MARKER}");
    let expected = gradle_coverage_block(kotlin);
    if let Some(existing) = owned_block(text, &open, &close)? {
        refuse_edited_feature(existing, &expected, "Gradle coverage", COVERAGE_MARKER)?;
        return if enabled {
            Ok(text.to_string())
        } else {
            Ok(replace_owned_block(text, &open, &close, None)?.expect("the owned block was found"))
        };
    }
    if !enabled {
        return Ok(text.to_string());
    }
    if text.contains("jacocoTestCoverageVerification") || text.contains("plugin: 'jacoco'") {
        return Err(reader_owned_feature(
            format!("Gradle already configures JaCoCo outside `// {COVERAGE_MARKER}`"),
            "remove the reader-owned duplicate or keep coverage outside the model",
        ));
    }
    let separator = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    Ok(format!("{text}{separator}{expected}"))
}

fn gradle_coverage_block(kotlin: bool) -> String {
    let body = if kotlin {
        "apply(plugin = \"jacoco\")\n\ntasks.named<org.gradle.testing.jacoco.tasks.JacocoCoverageVerification>(\"jacocoTestCoverageVerification\") {\n    violationRules {\n        rule {\n            limit {\n                counter = \"LINE\"\n                minimum = \"0.70\".toBigDecimal()\n            }\n        }\n    }\n}\n\ntasks.named(\"check\") {\n    dependsOn(tasks.named(\"jacocoTestCoverageVerification\"))\n}\n"
    } else {
        "apply plugin: 'jacoco'\n\ntasks.named('jacocoTestCoverageVerification') {\n    violationRules {\n        rule {\n            limit {\n                counter = 'LINE'\n                minimum = 0.70\n            }\n        }\n    }\n}\n\ntasks.named('check') {\n    dependsOn tasks.named('jacocoTestCoverageVerification')\n}\n"
    };
    format!("// {COVERAGE_MARKER}\n{body}// /{COVERAGE_MARKER}\n")
}

fn refuse_edited_feature(
    existing: &str,
    expected: &str,
    label: &str,
    marker: &str,
) -> Result<(), Diagnostic> {
    if normalized(existing) == normalized(expected) {
        return Ok(());
    }
    Err(block_edited(label, marker))
}

fn refuse_edited_feature_any(
    existing: &str,
    expected: &[String],
    label: &str,
    marker: &str,
) -> Result<(), Diagnostic> {
    if expected
        .iter()
        .any(|candidate| normalized(existing) == normalized(candidate))
    {
        return Ok(());
    }
    Err(block_edited(label, marker))
}

fn reconcile_maven_integration_tests(
    text: &str,
    enabled: bool,
    managed_versions: bool,
) -> Result<String, Diagnostic> {
    let open = format!("<!-- {INTEGRATION_TESTS_MARKER} -->");
    let close = format!("<!-- /{INTEGRATION_TESTS_MARKER} -->");
    let expected = maven_integration_tests_blocks(managed_versions);
    if let Some(existing) = owned_block(text, &open, &close)? {
        refuse_edited_any(existing, &expected, "Maven integration-test")?;
        return if enabled {
            Ok(text.to_string())
        } else {
            Ok(replace_owned_block(text, &open, &close, None)?.expect("the owned block was found"))
        };
    }
    if !enabled {
        return Ok(text.to_string());
    }
    if pom::has_plugin(text, "maven-failsafe-plugin") {
        return Err(reader_owned_feature(
            format!(
                "Maven already configures `maven-failsafe-plugin` outside `<!-- {INTEGRATION_TESTS_MARKER} -->`"
            ),
            "remove the reader-owned duplicate or keep integration-test execution outside the model",
        ));
    }
    insert_maven_plugin(text, managed_versions)
}

fn maven_integration_tests_plugin(managed_versions: bool) -> String {
    let version = if managed_versions {
        String::new()
    } else {
        "    <version>3.5.6</version>\n".to_string()
    };
    format!(
        "<plugin>\n\
             <groupId>org.apache.maven.plugins</groupId>\n\
             <artifactId>maven-failsafe-plugin</artifactId>\n\
         {version}\
             <executions>\n\
                 <execution>\n\
                     <goals>\n\
                         <goal>integration-test</goal>\n\
                         <goal>verify</goal>\n\
                     </goals>\n\
                 </execution>\n\
             </executions>\n\
         </plugin>\n"
    )
}

fn marked_maven(body: &str) -> String {
    format!("<!-- {INTEGRATION_TESTS_MARKER} -->\n{body}<!-- /{INTEGRATION_TESTS_MARKER} -->\n")
}

fn maven_integration_tests_blocks(managed_versions: bool) -> [String; 3] {
    pom::plugin_nest(&maven_integration_tests_plugin(managed_versions))
        .map(|shape| marked_maven(&shape))
}

/// **The marker wraps the containers here**, unlike its coverage and
/// formatting siblings: removing the last integration-test unit takes the
/// `<build><plugins>` this created back out with it, so a pom jails found
/// without one is left exactly as it was found.
fn insert_maven_plugin(text: &str, managed_versions: bool) -> Result<String, Diagnostic> {
    pom::insert_plugin(text, &maven_integration_tests_blocks(managed_versions))
}

fn reconcile_gradle_integration_tests(
    text: &str,
    enabled: bool,
    kotlin: bool,
) -> Result<String, Diagnostic> {
    let open = format!("// {INTEGRATION_TESTS_MARKER}");
    let close = format!("// /{INTEGRATION_TESTS_MARKER}");
    let expected = gradle_integration_tests_block(kotlin);
    if let Some(existing) = owned_block(text, &open, &close)? {
        refuse_edited(existing, &expected, "Gradle integration-test")?;
        return if enabled {
            Ok(text.to_string())
        } else {
            Ok(replace_owned_block(text, &open, &close, None)?.expect("the owned block was found"))
        };
    }
    if !enabled {
        return Ok(text.to_string());
    }
    if text.contains("integrationTest") {
        return Err(reader_owned_feature(
            format!(
                "Gradle already declares `integrationTest` outside `// {INTEGRATION_TESTS_MARKER}`"
            ),
            "remove the reader-owned duplicate or keep integration-test execution outside the model",
        ));
    }
    let separator = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    Ok(format!("{text}{separator}{expected}"))
}

fn gradle_integration_tests_block(kotlin: bool) -> String {
    let body = if kotlin {
        "tasks.named<org.gradle.api.tasks.testing.Test>(\"test\") {\n    useJUnitPlatform()\n    filter { excludeTestsMatching(\"*IT\") }\n}\n\ntasks.register<org.gradle.api.tasks.testing.Test>(\"integrationTest\") {\n    useJUnitPlatform()\n    testClassesDirs = sourceSets[\"test\"].output.classesDirs\n    classpath = sourceSets[\"test\"].runtimeClasspath\n    filter { includeTestsMatching(\"*IT\") }\n    shouldRunAfter(tasks.named(\"test\"))\n}\n\ntasks.named(\"check\") {\n    dependsOn(tasks.named(\"integrationTest\"))\n}\n"
    } else {
        "tasks.named('test') {\n    useJUnitPlatform()\n    filter { excludeTestsMatching '*IT' }\n}\n\ntasks.register('integrationTest', Test) {\n    useJUnitPlatform()\n    testClassesDirs = sourceSets.test.output.classesDirs\n    classpath = sourceSets.test.runtimeClasspath\n    filter { includeTestsMatching '*IT' }\n    shouldRunAfter tasks.named('test')\n}\n\ntasks.named('check') {\n    dependsOn tasks.named('integrationTest')\n}\n"
    };
    format!("// {INTEGRATION_TESTS_MARKER}\n{body}// /{INTEGRATION_TESTS_MARKER}\n")
}

fn refuse_edited(existing: &str, expected: &str, label: &str) -> Result<(), Diagnostic> {
    if normalized(existing) == normalized(expected) {
        return Ok(());
    }
    Err(block_edited(label, INTEGRATION_TESTS_MARKER))
}

fn refuse_edited_any(existing: &str, expected: &[String], label: &str) -> Result<(), Diagnostic> {
    if expected
        .iter()
        .any(|candidate| normalized(existing) == normalized(candidate))
    {
        return Ok(());
    }
    Err(block_edited(label, INTEGRATION_TESTS_MARKER))
}

/// The reader's own build already does what a feature would install.
///
/// One code for the five: Maven and Gradle, coverage, formatting and
/// integration tests. Each names its own plugin and its own way out in the
/// sentence, but the refusal is the same one -- jails will not claim a block
/// somebody else wrote.
fn reader_owned_feature(message: String, fix: &str) -> Diagnostic {
    Diagnostic::new(
        "workspace-build-feature-reader-owned",
        super::BUILD_SUBJECT,
        message,
        fix,
    )
}

/// The owned block is no longer the block jails wrote. One code for the four
/// callers, which differ only in which marker they name.
fn block_edited(label: &str, marker: &str) -> Diagnostic {
    Diagnostic::new(
        "workspace-owned-block-edited",
        marker.to_string(),
        format!("the owned {label} block was edited"),
        format!("restore the complete `{marker}` block, or remove it and re-plan"),
    )
}

fn normalized(block: &str) -> String {
    block.lines().map(str::trim).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled() -> BTreeSet<BuildFeature> {
        BTreeSet::from([BuildFeature::IntegrationTests])
    }

    fn coverage() -> BTreeSet<BuildFeature> {
        BTreeSet::from([BuildFeature::Coverage])
    }

    #[test]
    fn maven_feature_is_lossless_idempotent_removable_and_managed_by_boot() {
        let pom = "<project>\n    <!-- reader -->\n</project>\n";
        let once = reconcile_maven_build_features(pom, &enabled(), true).unwrap();
        assert!(once.contains("maven-failsafe-plugin"), "{once}");
        assert!(once.contains("<goal>integration-test</goal>"), "{once}");
        assert!(once.contains("<goal>verify</goal>"), "{once}");
        assert!(!once.contains("<version>3.5.6</version>"), "{once}");
        assert_eq!(
            reconcile_maven_build_features(&once, &enabled(), true).unwrap(),
            once
        );
        let removed = reconcile_maven_build_features(&once, &BTreeSet::new(), true).unwrap();
        assert_eq!(removed, pom);
    }

    #[test]
    fn plain_maven_pins_failsafe_and_an_edited_block_refuses() {
        let once =
            reconcile_maven_build_features("<project>\n</project>\n", &enabled(), false).unwrap();
        assert!(once.contains("<version>3.5.6</version>"), "{once}");
        let edited = once.replace("<goal>verify</goal>", "<goal>none</goal>");
        let error = reconcile_maven_build_features(&edited, &enabled(), false).unwrap_err();
        assert!(error.to_string().contains("was edited"), "{error}");
    }

    #[test]
    fn gradle_feature_has_separate_unit_and_integration_runs_in_both_dialects() {
        let groovy = reconcile_gradle_build_features("plugins {}\n", &enabled(), false).unwrap();
        assert!(groovy.contains("excludeTestsMatching '*IT'"), "{groovy}");
        assert!(
            groovy.contains("tasks.register('integrationTest', Test)"),
            "{groovy}"
        );
        assert_eq!(
            reconcile_gradle_build_features(&groovy, &enabled(), false).unwrap(),
            groovy
        );
        let kotlin = reconcile_gradle_build_features("plugins {}\n", &enabled(), true).unwrap();
        assert!(kotlin.contains("tasks.register<org.gradle.api.tasks.testing.Test>"));
        assert!(kotlin.contains("includeTestsMatching(\"*IT\")"));
        assert_eq!(
            reconcile_gradle_build_features(&kotlin, &BTreeSet::new(), true).unwrap(),
            "plugins {}\n"
        );
    }

    #[test]
    fn maven_coverage_is_lossless_idempotent_removable_and_refuses_an_edited_gate() {
        let pom = "<project>\n    <!-- reader -->\n</project>\n";
        let once = reconcile_maven_build_features(pom, &coverage(), false).unwrap();
        for expected in [
            "jails:coverage",
            "jacoco-maven-plugin",
            "<version>0.8.15</version>",
            "<minimum>0.80</minimum>",
        ] {
            assert!(once.contains(expected), "missing {expected}: {once}");
        }
        assert_eq!(
            reconcile_maven_build_features(&once, &coverage(), false).unwrap(),
            once
        );
        let edited = once.replace("<minimum>0.80</minimum>", "<minimum>0.75</minimum>");
        let error = reconcile_maven_build_features(&edited, &coverage(), false).unwrap_err();
        assert!(
            error.to_string().contains("coverage block was edited"),
            "{error}"
        );
        // **Removal takes back the plugin, not the container.** `<build>` and
        // `<plugins>` are created outside the markers on purpose (see
        // `insert_maven_feature_plugin`), because owning them makes the next
        // command that legitimately adds a plugin read as an edit to this
        // block and refuse every later plan. So the inverse of `add` is "the
        // plugin is gone", not "the file is byte-identical": what is left is
        // an empty container that jails never claimed and that changes no
        // build behaviour.
        let removed = reconcile_maven_build_features(&once, &BTreeSet::new(), false).unwrap();
        assert!(!removed.contains("jacoco-maven-plugin"), "{removed}");
        assert!(!removed.contains(COVERAGE_MARKER), "{removed}");
        assert!(removed.contains("<!-- reader -->"), "{removed}");
        assert_eq!(
            removed,
            "<project>\n    <!-- reader -->\n    <build>\n        <plugins>\n        \
             </plugins>\n    </build>\n</project>\n"
        );
        // Removing twice is still a no-op, and re-adding lands in the
        // container that is already there rather than nesting a second one.
        assert_eq!(
            reconcile_maven_build_features(&removed, &BTreeSet::new(), false).unwrap(),
            removed
        );
        let re_added = reconcile_maven_build_features(&removed, &coverage(), false).unwrap();
        assert_eq!(re_added.matches("<plugins>").count(), 1, "{re_added}");
        assert!(re_added.contains("jacoco-maven-plugin"), "{re_added}");
    }

    #[test]
    fn gradle_coverage_stacks_with_integration_tests_in_both_dialects() {
        let both = BTreeSet::from([BuildFeature::Coverage, BuildFeature::IntegrationTests]);
        let groovy = reconcile_gradle_build_features("plugins {}\n", &both, false).unwrap();
        assert!(groovy.contains("jails:integration-tests"), "{groovy}");
        assert!(groovy.contains("jails:coverage"), "{groovy}");
        assert!(groovy.contains("minimum = 0.70"), "{groovy}");
        let without_coverage = reconcile_gradle_build_features(&groovy, &enabled(), false).unwrap();
        assert!(without_coverage.contains("jails:integration-tests"));
        assert!(!without_coverage.contains("jails:coverage"));

        let kotlin = reconcile_gradle_build_features("plugins {}\n", &coverage(), true).unwrap();
        assert!(kotlin.contains("apply(plugin = \"jacoco\")"), "{kotlin}");
        assert!(kotlin.contains("JacocoCoverageVerification"), "{kotlin}");
        assert_eq!(
            reconcile_gradle_build_features(&kotlin, &BTreeSet::new(), true).unwrap(),
            "plugins {}\n"
        );
    }
}
