//! Pure test partitioning and its human explanation.

use super::TestOptions;
use jails_protocol::testing::{
    SelectionReason, TestEngine, TestEnginePolicy, TestExecutionPlanV1, TestPartition, TestSelector,
};
use jails_support::Result;

pub(super) fn plan(
    build: crate::build::Build,
    requested: &[String],
    options: &TestOptions,
) -> Result<TestExecutionPlanV1> {
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
    let engine = match options.engine {
        TestEnginePolicy::Build => build_engine,
        TestEnginePolicy::Warm => TestEngine::TestdV2,
        TestEnginePolicy::Auto
            if matches!(
                options.compile,
                jails_protocol::testing::TestCompilePolicy::Ide
                    | jails_protocol::testing::TestCompilePolicy::None
            ) && build_engine == TestEngine::Maven =>
        {
            TestEngine::TestdV2
        }
        TestEnginePolicy::Auto => build_engine,
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
    let plan = TestExecutionPlanV1 {
        scope: options.scope,
        requested: selectors.clone(),
        compile: options.compile,
        engine: options.engine,
        epoch: 0,
        partitions: vec![TestPartition {
            engine,
            selectors,
            reasons,
        }],
    };
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
    if options.database_schema && options.scope == jails_protocol::testing::TestScope::Unit {
        return Err(
            "`--db schema` has no meaning for the unit-test scope\n       fix: pass \
                    `--scope integration` or `--scope all`"
                .into(),
        );
    }
    if options.engine == TestEnginePolicy::Warm
        && options.compile == jails_protocol::testing::TestCompilePolicy::Build
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
            jails_protocol::testing::TestCompilePolicy::Ide
                | jails_protocol::testing::TestCompilePolicy::None
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

pub(super) fn explain(plan: &TestExecutionPlanV1) {
    println!("test selection: epoch {} {:?}", plan.epoch, plan.scope);
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
    use jails_protocol::testing::{TestCompilePolicy, TestScope};

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

    #[test]
    fn automatic_execution_keeps_every_requested_selector() {
        let plan = plan(
            crate::build::Build::Maven,
            &["BetaTest".into(), "AlphaTest".into(), "AlphaTest".into()],
            &options(),
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
        let plan = plan(crate::build::Build::Maven, &["AlphaTest".into()], &options).unwrap();
        assert_eq!(plan.partitions[0].engine, TestEngine::TestdV2);
    }

    #[test]
    fn duration_units_are_checked() {
        assert_eq!(parse_duration("30s").unwrap(), 30);
        assert_eq!(parse_duration("2m").unwrap(), 120);
        assert!(parse_duration("soon").is_err());
        assert!(parse_duration("0s").is_err());
    }
}
