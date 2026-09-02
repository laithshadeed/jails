//! Pure test partitioning and its human explanation.

use super::TestOptions;
use crate::testing::{
    SelectionReason, TestEngine, TestEnginePolicy, TestPartition, TestPlan, TestSelector,
};
use jails_support::Result;
use std::path::Path;

pub(super) fn plan(
    project: &Path,
    build: crate::build::Build,
    requested: &[String],
    options: &TestOptions,
    compiled_outputs_current: bool,
) -> Result<TestPlan> {
    let mut selectors: Vec<TestSelector> = requested
        .iter()
        .map(|selector| TestSelector::parse(selector))
        .collect::<Result<_>>()?;
    selectors.sort();
    selectors.dedup();

    let build_engine = match build {
        crate::build::Build::Maven => TestEngine::Maven,
        crate::build::Build::Gradle => TestEngine::Gradle,
        other => {
            return Err(format!(
                "jails test cannot execute a {} build\n       fix: add a Maven `pom.xml` or \
                 supported Groovy `build.gradle`",
                other.name()
            )
            .into());
        }
    };
    let reasons = if options.fast {
        vec![SelectionReason::Widened(
            "`--fast` normalized to auto; compiled classes are tried before build fallback"
                .to_string(),
        )]
    } else if selectors.is_empty() {
        vec![SelectionReason::Scope(options.scope)]
    } else {
        vec![SelectionReason::Requested]
    };
    if options.compile == crate::testing::TestCompilePolicy::Ide {
        return Err(
            "`--compile ide` requires a negotiated editor output epoch\n       fix: connect the editor session, or use `--compile auto`"
                .into(),
        );
    }

    let build_only_reason = if build_engine != TestEngine::Maven {
        Some("the warm engine is unavailable for this build system")
    } else if options.compile == crate::testing::TestCompilePolicy::Build {
        Some("the build tool is the explicit compile owner")
    } else if !compiled_outputs_current {
        Some("compiled test outputs are stale")
    } else if options.scope != crate::testing::TestScope::Unit || options.database_schema {
        Some("the warm engine only accepts isolated unit tests")
    } else if !options.tags.is_empty() {
        Some("JUnit tag eligibility is not yet attributable per warm test")
    } else {
        None
    };

    if options.engine == TestEnginePolicy::Warm
        && build_engine == TestEngine::Maven
        && options.scope == crate::testing::TestScope::Unit
        && !options.database_schema
        && options.tags.is_empty()
    {
        let evidence = super::isolation::partition_evidence(project, requested);
        if !evidence.ineligible.is_empty() || !evidence.gaps.is_empty() {
            let reason = evidence
                .ineligible
                .iter()
                .map(|(_, reason)| reason.as_str())
                .chain(evidence.gaps.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(format!(
                "strict warm execution is ineligible: {reason}\n       fix: choose `--engine auto` so the build tool owns this partition"
            )
            .into());
        }
    }

    let mut partitions = Vec::new();
    if options.engine == TestEnginePolicy::Build || build_only_reason.is_some() {
        if options.engine == TestEnginePolicy::Warm {
            return Err(format!(
                "strict warm execution is unavailable: {}\n       fix: choose `--engine auto` so the build tool owns this partition",
                build_only_reason.unwrap_or("the selected policy is incompatible")
            )
            .into());
        }
        partitions.push(TestPartition {
            engine: build_engine,
            selectors: selectors.clone(),
            reasons: if options.engine == TestEnginePolicy::Build {
                reasons.clone()
            } else {
                let mut partition_reasons = reasons.clone();
                if let Some(reason) = build_only_reason {
                    partition_reasons.push(SelectionReason::Widened(reason.to_string()));
                }
                partition_reasons
            },
        });
    } else if options.affected {
        partitions.push(TestPartition {
            engine: TestEngine::TestdV2,
            selectors: selectors.clone(),
            reasons: reasons.clone(),
        });
    } else {
        let evidence = super::isolation::partition_evidence(project, requested);
        if !evidence.gaps.is_empty() {
            let reason = format!("test discovery is incomplete: {}", evidence.gaps.join("; "));
            if options.engine == TestEnginePolicy::Warm {
                return Err(format!(
                    "strict warm execution is ineligible: {reason}\n       fix: choose `--engine auto` so the build tool widens safely"
                )
                .into());
            }
            partitions.push(TestPartition {
                engine: build_engine,
                selectors: Vec::new(),
                reasons: vec![SelectionReason::Widened(reason)],
            });
        } else {
            if !evidence.ineligible.is_empty() {
                let reason = evidence
                    .ineligible
                    .iter()
                    .map(|(_, reason)| reason.as_str())
                    .collect::<Vec<_>>()
                    .join("; ");
                if options.engine == TestEnginePolicy::Warm {
                    return Err(format!(
                        "strict warm execution is ineligible: {reason}\n       fix: choose `--engine auto` so the build tool owns this partition"
                    )
                    .into());
                }
                partitions.push(TestPartition {
                    engine: build_engine,
                    selectors: evidence
                        .ineligible
                        .iter()
                        .map(|(selector, _)| TestSelector::parse(selector))
                        .collect::<Result<Vec<_>>>()?,
                    reasons: vec![SelectionReason::Widened(reason)],
                });
            }
            if !evidence.eligible.is_empty() || evidence.ineligible.is_empty() {
                partitions.push(TestPartition {
                    engine: TestEngine::TestdV2,
                    selectors: evidence
                        .eligible
                        .iter()
                        .map(|selector| TestSelector::parse(selector))
                        .collect::<Result<Vec<_>>>()?,
                    reasons: reasons.clone(),
                });
            }
        }
    }

    let plan = TestPlan {
        scope: options.scope,
        requested: selectors.clone(),
        compile: options.compile,
        engine: options.engine,
        partitions,
    };
    if matches!(
        options.compile,
        crate::testing::TestCompilePolicy::Ide | crate::testing::TestCompilePolicy::None
    ) && plan
        .partitions
        .iter()
        .any(|partition| partition.engine != TestEngine::TestdV2)
    {
        return Err(match options.compile {
            crate::testing::TestCompilePolicy::None => {
                "automatic warm execution is ineligible and `--compile none` forbids the build partition\n       fix: compile explicitly, or choose `--compile auto` so the build tool may own this partition"
            }
            _ => {
                "the selected compile policy cannot supply a current warm-engine output for this project\n       fix: use `--compile auto` or `--compile build`"
            }
        }
        .into());
    }
    plan.validate()?;
    Ok(plan)
}

pub(super) fn validate_runtime_options(options: &TestOptions) -> Result<()> {
    if options.repeat == 0 {
        return Err(
            "`--repeat` must be at least one\n       fix: pass `--repeat 1` or omit it".into(),
        );
    }
    if options.until_fail && options.repeat != 1 {
        return Err(
            "`--until-fail` and `--repeat` describe competing run limits\n       fix: use \
                    one of them, not both"
                .into(),
        );
    }
    if options.database_schema && options.scope == crate::testing::TestScope::Unit {
        return Err(
            "`--db schema` has no meaning for the unit-test scope\n       fix: pass \
                    `--scope integration` or `--scope all`"
                .into(),
        );
    }
    if options.engine == TestEnginePolicy::Warm
        && options.compile == crate::testing::TestCompilePolicy::Build
    {
        return Err(
            "strict warm execution cannot hand compilation to the build engine\n       fix: use \
             `--compile auto`, `--compile ide`, or choose `--engine auto`"
                .into(),
        );
    }
    if options.engine == TestEnginePolicy::Build
        && matches!(
            options.compile,
            crate::testing::TestCompilePolicy::Ide | crate::testing::TestCompilePolicy::None
        )
    {
        return Err(
            "the build engine necessarily owns compilation\n       fix: use `--compile build`, \
             `--compile auto`, or choose `--engine warm`"
                .into(),
        );
    }
    if let Some(timeout) = &options.timeout {
        parse_duration(timeout)?;
    }
    Ok(())
}

pub(super) fn explain(plan: &TestPlan) {
    println!("test selection: {:?}", plan.scope);
    println!("  compile: {:?}", plan.compile);
    println!("  engine policy: {:?}", plan.engine);
    for partition in &plan.partitions {
        let selection = if partition.selectors.is_empty() {
            "all tests in scope".to_string()
        } else {
            partition
                .selectors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        };
        println!("  {:?}: {selection}", partition.engine);
        for reason in &partition.reasons {
            println!("    reason: {reason:?}");
        }
    }
}

pub(super) fn parse_duration(text: &str) -> Result<u64> {
    let (number, multiplier) = if let Some(seconds) = text.strip_suffix('s') {
        (seconds, 1)
    } else if let Some(minutes) = text.strip_suffix('m') {
        (minutes, 60)
    } else if let Some(hours) = text.strip_suffix('h') {
        (hours, 60 * 60)
    } else {
        return Err(format!(
            "unsupported timeout `{text}`\n       fix: use seconds, minutes, or hours such as \
             `30s`, `2m`, or `1h`"
        )
        .into());
    };
    let count: u64 = number.parse().map_err(|_| {
        format!(
            "invalid timeout `{text}`\n       fix: use a positive integer followed by `s`, `m`, \
             or `h`"
        )
    })?;
    if count == 0 {
        return Err(
            "timeout must be greater than zero\n       fix: pass a positive duration such as `1s`"
                .into(),
        );
    }
    count
        .checked_mul(multiplier)
        .ok_or_else(|| "timeout is too large\n       fix: pass a smaller duration".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TestCompilePolicy, TestScope};

    fn options() -> TestOptions {
        TestOptions {
            scope: TestScope::Unit,
            compile: TestCompilePolicy::Auto,
            engine: TestEnginePolicy::Auto,
            watch: false,
            affected: false,
            failed: false,
            tags: Vec::new(),
            fail_fast: false,
            slowest: None,
            json: false,
            fast: false,
            until_fail: false,
            repeat: 1,
            timeout: None,
            database_schema: false,
            explain_selection: false,
        }
    }

    fn project_with(tests: &[(&str, &str)]) -> jails_support::scratch::ScratchDir {
        let project = jails_support::scratch::ScratchDir::in_temp("test-plan").unwrap();
        for (name, body) in tests {
            let source = project
                .path()
                .join("src/test/java/com/example")
                .join(format!("{name}.java"));
            std::fs::create_dir_all(source.parent().unwrap()).unwrap();
            std::fs::write(source, format!("package com.example;\n{body}\n")).unwrap();
        }
        project
    }

    #[test]
    fn automatic_execution_keeps_every_requested_selector() {
        let plan = plan(
            project_with(&[
                (
                    "AlphaTest",
                    "class AlphaTest { @org.junit.Test void ok() {} }",
                ),
                (
                    "BetaTest",
                    "class BetaTest { @org.junit.Test void ok() {} }",
                ),
            ])
            .path(),
            crate::build::Build::Maven,
            &["BetaTest".into(), "AlphaTest".into(), "AlphaTest".into()],
            &options(),
            false,
        )
        .unwrap();
        assert_eq!(plan.requested.len(), 2);
        assert_eq!(plan.partitions[0].selectors, plan.requested);
        assert_eq!(plan.partitions[0].engine, TestEngine::Maven);
    }

    #[test]
    fn strict_warm_never_creates_a_build_partition() {
        let mut options = options();
        options.engine = TestEnginePolicy::Warm;
        let plan = plan(
            project_with(&[(
                "AlphaTest",
                "class AlphaTest { @org.junit.Test void ok() {} }",
            )])
            .path(),
            crate::build::Build::Maven,
            &["AlphaTest".into()],
            &options,
            true,
        )
        .unwrap();
        assert_eq!(plan.partitions[0].engine, TestEngine::TestdV2);
    }

    #[test]
    fn automatic_compile_uses_current_maven_outputs_and_builds_when_stale() {
        let current = plan(
            project_with(&[(
                "AlphaTest",
                "class AlphaTest { @org.junit.Test void ok() {} }",
            )])
            .path(),
            crate::build::Build::Maven,
            &["AlphaTest".into()],
            &options(),
            true,
        )
        .unwrap();
        assert_eq!(current.partitions[0].engine, TestEngine::TestdV2);

        let stale = plan(
            project_with(&[(
                "AlphaTest",
                "class AlphaTest { @org.junit.Test void ok() {} }",
            )])
            .path(),
            crate::build::Build::Maven,
            &["AlphaTest".into()],
            &options(),
            false,
        )
        .unwrap();
        assert_eq!(stale.partitions[0].engine, TestEngine::Maven);
    }

    #[test]
    fn automatic_execution_partitions_warm_and_fork_sensitive_selectors() {
        let project = project_with(&[
            (
                "PlainTest",
                "class PlainTest { @org.junit.Test void ok() {} }",
            ),
            (
                "ContextTest",
                "@SpringBootTest class ContextTest { @org.junit.Test void ok() {} }",
            ),
        ]);
        let plan = plan(
            project.path(),
            crate::build::Build::Maven,
            &["PlainTest".into(), "ContextTest".into()],
            &options(),
            true,
        )
        .unwrap();
        assert_eq!(plan.partitions.len(), 2);
        assert_eq!(plan.partitions[0].engine, TestEngine::Maven);
        assert_eq!(plan.partitions[0].selectors[0].as_str(), "ContextTest");
        assert_eq!(plan.partitions[1].engine, TestEngine::TestdV2);
        assert_eq!(plan.partitions[1].selectors[0].as_str(), "PlainTest");
    }

    #[test]
    fn gradle_cannot_compile_implicitly_under_none() {
        let mut no_compile = options();
        no_compile.compile = TestCompilePolicy::None;
        assert!(
            plan(
                project_with(&[(
                    "AlphaTest",
                    "class AlphaTest { @org.junit.Test void ok() {} }"
                )])
                .path(),
                crate::build::Build::Gradle,
                &["AlphaTest".into()],
                &no_compile,
                false,
            )
            .is_err()
        );
    }

    #[test]
    fn duration_units_are_checked() {
        assert_eq!(parse_duration("30s").unwrap(), 30);
        assert_eq!(parse_duration("2m").unwrap(), 120);
        assert!(parse_duration("soon").is_err());
        assert!(parse_duration("0s").is_err());
    }
}
