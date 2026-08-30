//! A deliberately small reader for the JUnit XML every build tool writes.
//!
//! Everything `jails test` can say about the *last* run comes from a report
//! directory: which tests failed, and which ones took the time. The build tool
//! already wrote it; asking it again costs a build.
//!
//! **One reader for Maven and Gradle**, because they write the same schema and
//! differ only in where. Surefire and Failsafe put theirs under
//! `target/*-reports/`; Gradle puts a `Test` task's under
//! `build/test-results/<task>/`. It was called `surefire.rs` while only one of
//! them existed.
//!
//! They differ in one detail, and it is the kind that produces a plausible
//! wrong answer rather than an error: **Gradle writes the method name with its
//! parentheses** -- `name="passes()"` where Surefire writes `name="passes"`.
//! Left alone that yields the rerun selector `SampleTest#passes()`, which
//! JUnit matches against nothing, so `jails test --failed` would run zero
//! tests and report success. Verified against Gradle 9.6.1 output, not
//! assumed.
//!
//! **Not an XML parser, and must not grow into one**, for the same reason
//! `java.rs` is not a Java parser. It reads two things out of a
//! `<testcase>` element -- the attributes, and whether a `<failure>` or
//! `<error>` child follows before the element ends -- and gives up on
//! anything it does not recognise rather than guessing. A report it cannot
//! read is reported as no data, never as "nothing failed": the difference
//! between those two is exactly the kind of quiet wrong answer that makes a
//! tool untrustworthy.

use crate::testing::{
    SelectionReason, TestCaseResultV1, TestCompileOwner, TestEngine, TestOutcome, TestReportV1,
    TestScope, TestSelector,
};
use jails_support::Result;
use std::path::{Path, PathBuf};

/// One `<testcase>` from a report.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Case {
    pub class: String,
    pub method: String,
    /// Seconds, as Surefire recorded them.
    pub seconds: f64,
    pub failed: bool,
    pub skipped: bool,
    pub error: bool,
}

impl Case {
    /// The filter that reruns exactly this test.
    pub fn selector(&self) -> String {
        format!("{}#{}", short_class(&self.class), self.method)
    }

    fn canonical_selector(&self) -> Result<TestSelector> {
        TestSelector::parse(&format!("{}#{}", self.class, self.method))
    }
}

/// `com.example.demo.PayoutTest` -> `PayoutTest`.
///
/// Surefire's `-Dtest` matches on the simple name, and the fully qualified
/// one is what a reader would have to look up.
fn short_class(class: &str) -> &str {
    class.rsplit('.').next().unwrap_or(class)
}

/// Every case both plugins recorded, newest report layout first.
///
/// Both directories, because the split between them is Maven's, not the
/// reader's: `jails test --failed` after a `verify` should offer to rerun the
/// integration test that failed, not silently ignore it because it was
/// Failsafe's.
/// Where a build tool leaves its JUnit XML.
///
/// Both tools' directories, always, rather than the ones this project's build
/// file implies: a directory that is not there reads as no reports, which is
/// the same answer as an empty one, and a project that switched build tools
/// mid-life still has the old reports to explain. Guessing from the build file
/// would make `--failed` silently answer from nothing on the run right after
/// the switch.
///
/// `integrationTest` is the task name `gradle::Feature::IntegrationTests`
/// renders, so this and that block have to keep saying the same word.
const REPORT_DIRECTORIES: &[&str] = &[
    "target/surefire-reports",
    "target/failsafe-reports",
    "build/test-results/test",
    "build/test-results/integrationTest",
];

pub(crate) fn cases(root: &Path) -> Vec<Case> {
    let mut found = Vec::new();
    for dir in REPORT_DIRECTORIES {
        for path in xml_reports(&root.join(dir)) {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            found.extend(parse(&text));
        }
    }
    found
}

/// Which tests failed last time, as rerun selectors, deduplicated and in a
/// stable order.
pub(crate) fn failed_selectors(root: &Path) -> Vec<String> {
    let mut selectors: Vec<String> = cases(root)
        .into_iter()
        .filter(|case| case.failed)
        .map(|case| case.selector())
        .collect();
    selectors.sort();
    selectors.dedup();
    selectors
}

fn xml_reports(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "xml"))
        .collect();
    found.sort();
    found
}

/// Pull every `<testcase>` out of one report.
pub(crate) fn parse(xml: &str) -> Vec<Case> {
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find("<testcase ") {
        let after = &rest[at + "<testcase ".len()..];
        // The element ends at the first `>`; whether it is self-closing
        // decides where its children stop.
        let Some(close) = after.find('>') else {
            break;
        };
        let attributes = &after[..close];
        let self_closing = attributes.trim_end().ends_with('/');
        let body_end = if self_closing {
            0
        } else {
            after[close..].find("</testcase>").unwrap_or(0)
        };
        let body = &after[close..close + body_end];

        let class = attribute(attributes, "classname").unwrap_or_default();
        // Gradle writes `passes()`; Surefire writes `passes`. Trimmed here
        // rather than in `selector()`, because the parenthesised form is a
        // spelling of the report format and nothing above this line should
        // have to know which tool wrote the file.
        let method = attribute(attributes, "name")
            .map(|name| name.trim_end_matches("()").to_string())
            .unwrap_or_default();
        if !class.is_empty() && !method.is_empty() {
            found.push(Case {
                class,
                method,
                seconds: attribute(attributes, "time")
                    .and_then(|t| t.replace(',', "").parse::<f64>().ok())
                    .unwrap_or(0.0),
                // A skipped test is not a failure, and `<skipped/>` is a
                // sibling of `<failure>` -- reading "has any child" as
                // "failed" would make `--failed` rerun every `@Disabled`
                // test in the project.
                failed: body.contains("<failure") || body.contains("<error"),
                skipped: body.contains("<skipped"),
                error: body.contains("<error"),
            });
        }
        rest = &after[close..];
    }
    found
}

/// Convert Maven and Gradle's common JUnit XML into the engine-independent
/// result contract consumed by every renderer.
pub(crate) fn normalized(
    root: &Path,
    engine: TestEngine,
    scope: TestScope,
    requested: &[String],
    passed: bool,
    fallback_reason: Option<String>,
) -> Result<TestReportV1> {
    let requested = requested
        .iter()
        .map(|selector| TestSelector::parse(selector))
        .collect::<Result<Vec<_>>>()?;
    let compile_owner = match engine {
        TestEngine::Maven => TestCompileOwner::Maven,
        TestEngine::Gradle => TestCompileOwner::Gradle,
        TestEngine::TestdV2 => TestCompileOwner::None,
    };
    let selection_reasons = if requested.is_empty() {
        vec![SelectionReason::Scope(scope)]
    } else {
        vec![SelectionReason::Requested]
    };
    let results = cases(root)
        .into_iter()
        .filter(|case| {
            requested.is_empty() || requested.iter().any(|selector| matches(case, selector))
        })
        .map(|case| {
            let outcome = if case.error {
                TestOutcome::Error
            } else if case.failed {
                TestOutcome::Failed
            } else if case.skipped {
                TestOutcome::Skipped
            } else {
                TestOutcome::Passed
            };
            Ok(TestCaseResultV1 {
                engine,
                compile_owner,
                selector: case.canonical_selector()?,
                source: None,
                outcome,
                duration_us: (case.seconds.max(0.0) * 1_000_000.0) as u64,
                stdout_summary: String::new(),
                stderr_summary: String::new(),
                selection_reasons: selection_reasons.clone(),
                fallback_reason: fallback_reason.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(TestReportV1 {
        epoch: 0,
        passed,
        scope,
        requested,
        cases: results,
        fallback_reasons: fallback_reason.into_iter().collect(),
    })
}

fn matches(case: &Case, selector: &TestSelector) -> bool {
    let (class, method) = selector
        .as_str()
        .split_once('#')
        .map_or((selector.as_str(), None), |(class, method)| {
            (class, Some(method))
        });
    (class == case.class || class == short_class(&case.class))
        && method.is_none_or(|method| method == case.method)
}

pub(crate) fn merge(
    scope: TestScope,
    requested: &[String],
    reports: Vec<TestReportV1>,
) -> Result<TestReportV1> {
    let requested = requested
        .iter()
        .map(|selector| TestSelector::parse(selector))
        .collect::<Result<Vec<_>>>()?;
    let epoch = reports.iter().map(|report| report.epoch).max().unwrap_or(0);
    let passed = reports.iter().all(TestReportV1::succeeded);
    let mut cases = reports
        .iter()
        .flat_map(|report| report.cases.iter().cloned())
        .collect::<Vec<_>>();
    cases.sort_by(|left, right| {
        left.selector
            .cmp(&right.selector)
            .then_with(|| engine_name(left.engine).cmp(engine_name(right.engine)))
    });
    let mut fallback_reasons = reports
        .into_iter()
        .flat_map(|report| report.fallback_reasons)
        .collect::<Vec<_>>();
    fallback_reasons.sort();
    fallback_reasons.dedup();
    Ok(TestReportV1 {
        epoch,
        passed,
        scope,
        requested,
        cases,
        fallback_reasons,
    })
}

pub(crate) fn render(
    report: &TestReportV1,
    json: bool,
    slowest_count: Option<usize>,
) -> Result<()> {
    if json {
        println!("{}", json_line(report));
    } else {
        for case in &report.cases {
            if !case.stdout_summary.is_empty() {
                print!("{}", case.stdout_summary);
            }
            if !case.stderr_summary.is_empty() {
                eprint!("{}", case.stderr_summary);
            }
        }
        if let Some(count) = slowest_count {
            report_slowest_normalized(report, count);
        }
    }
    if report.succeeded() {
        Ok(())
    } else {
        Err(jails_support::Failure::Reported)
    }
}

pub(crate) fn json_line(report: &TestReportV1) -> String {
    let cases = report.cases.iter().map(|case| {
        format!(
            "{{\"engine\":{},\"compile_owner\":{},\"selector\":{},\"outcome\":{},\"duration_us\":{},\"stdout_summary\":{},\"stderr_summary\":{},\"fallback_reason\":{}}}",
            crate::json::string(engine_name(case.engine)),
            crate::json::string(compile_owner_name(case.compile_owner)),
            crate::json::string(case.selector.as_str()),
            crate::json::string(outcome_name(case.outcome)),
            case.duration_us,
            crate::json::string(&case.stdout_summary),
            crate::json::string(&case.stderr_summary),
            case.fallback_reason.as_ref().map_or_else(|| "null".into(), |reason| crate::json::string(reason)),
        )
    }).collect::<Vec<_>>().join(",");
    let requested = report
        .requested
        .iter()
        .map(|selector| crate::json::string(selector.as_str()))
        .collect::<Vec<_>>()
        .join(",");
    let fallbacks = report
        .fallback_reasons
        .iter()
        .map(|reason| crate::json::string(reason))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema_version\":1,\"epoch\":{},\"scope\":{},\"passed\":{},\"requested\":[{}],\"fallback_reasons\":[{}],\"cases\":[{}]}}",
        report.epoch,
        crate::json::string(scope_name(report.scope)),
        report.succeeded(),
        requested,
        fallbacks,
        cases
    )
}

fn report_slowest_normalized(report: &TestReportV1, count: usize) {
    let mut cases = report.cases.iter().collect::<Vec<_>>();
    cases.sort_by_key(|case| std::cmp::Reverse(case.duration_us));
    cases.truncate(count);
    println!();
    println!("slowest {} test(s):", cases.len());
    for case in cases {
        println!(
            "  {:>8.2}s  {}",
            case.duration_us as f64 / 1_000_000.0,
            case.selector
        );
    }
}

fn engine_name(engine: TestEngine) -> &'static str {
    match engine {
        TestEngine::Maven => "maven",
        TestEngine::Gradle => "gradle",
        TestEngine::TestdV2 => "testd-v2",
    }
}

fn compile_owner_name(owner: TestCompileOwner) -> &'static str {
    match owner {
        TestCompileOwner::Ide => "ide",
        TestCompileOwner::Maven => "maven",
        TestCompileOwner::Gradle => "gradle",
        TestCompileOwner::None => "none",
    }
}

fn outcome_name(outcome: TestOutcome) -> &'static str {
    match outcome {
        TestOutcome::Passed => "passed",
        TestOutcome::Failed => "failed",
        TestOutcome::Skipped => "skipped",
        TestOutcome::Error => "error",
    }
}

fn scope_name(scope: TestScope) -> &'static str {
    match scope {
        TestScope::Unit => "unit",
        TestScope::Integration => "integration",
        TestScope::All => "all",
    }
}

/// Print the canonical command that reruns the failures recorded by the build.
pub(crate) fn rerun_line(root: &Path, already_filtered: Option<&str>) {
    let failures = failed_selectors(root);
    println!();
    match failures.len() {
        0 => {
            if let Some(filter) = already_filtered {
                println!("jails: rerun with  jails test '{filter}'");
            }
        }
        1 => println!("jails: rerun with  jails test '{}'", failures[0]),
        count => {
            println!("jails: {count} test(s) failed. Rerun just those with  jails test --failed");
            for selector in failures.iter().take(5) {
                println!("         {selector}");
            }
            if count > 5 {
                println!("         ... and {} more", count - 5);
            }
        }
    }
}

/// `name="value"` out of an attribute list. Single quotes too: Surefire
/// writes double, but a report is XML and both are legal.
fn attribute(attributes: &str, name: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!("{name}={quote}");
        if let Some(at) = attributes.find(&needle) {
            let after = &attributes[at + needle.len()..];
            if let Some(end) = after.find(quote) {
                return Some(after[..end].to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    /// Gradle writes the method name with its parentheses; Surefire does not.
    ///
    /// Left alone, that yields the selector `SampleTest#passes()`, which JUnit
    /// matches against nothing -- so `--failed` would run zero tests and
    /// report success. Verified against Gradle 9.6.1's own output.
    #[test]
    fn a_gradle_report_reads_the_same_as_a_surefire_one() {
        let gradle = parse(
            r#"<testsuite name="SampleTest" tests="2">
  <testcase name="passes()" classname="com.example.SampleTest" time="0.013"/>
  <testcase name="failsOnPurpose()" classname="com.example.SampleTest" time="0.006">
    <failure message="deliberate"/>
  </testcase>
</testsuite>"#,
        );
        assert_eq!(gradle.len(), 2);
        assert_eq!(gradle[0].method, "passes");
        assert_eq!(gradle[0].selector(), "SampleTest#passes");
        assert!(gradle[1].failed);
    }

    use super::*;

    const REPORT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="com.example.demo.PayoutTest" tests="4" time="1.5">
  <testcase name="settles" classname="com.example.demo.PayoutTest" time="0.25"/>
  <testcase name="rejectsANullComponent" classname="com.example.demo.PayoutTest" time="1.20">
    <failure message="expected: not null" type="java.lang.AssertionError">stack</failure>
  </testcase>
  <testcase name="explodes" classname="com.example.demo.PayoutTest" time="0.05">
    <error message="boom" type="java.lang.IllegalStateException">stack</error>
  </testcase>
  <testcase name="todo" classname="com.example.demo.PayoutTest" time="0">
    <skipped/>
  </testcase>
</testsuite>
"#;

    #[test]
    fn a_failure_and_an_error_are_failures_and_a_skip_is_not() {
        let cases = parse(REPORT);
        assert_eq!(cases.len(), 4, "{cases:?}");
        let failed: Vec<String> = cases
            .iter()
            .filter(|c| c.failed)
            .map(Case::selector)
            .collect();
        // The skipped one is absent on purpose: `--failed` that reruns every
        // @Disabled test in the project is worse than no flag.
        assert_eq!(
            failed,
            vec![
                "PayoutTest#rejectsANullComponent".to_string(),
                "PayoutTest#explodes".to_string()
            ]
        );
    }

    #[test]
    fn the_selector_is_the_simple_class_name_surefire_matches_on() {
        let cases = parse(REPORT);
        assert_eq!(cases[0].selector(), "PayoutTest#settles");
    }

    #[test]
    fn times_are_read_so_the_slowest_can_be_named() {
        let cases = parse(REPORT);
        let mut times: Vec<f64> = cases.iter().map(|c| c.seconds).collect();
        times.sort_by(f64::total_cmp);
        assert_eq!(times, vec![0.0, 0.05, 0.25, 1.20]);
    }

    #[test]
    fn normalized_reports_keep_run_verdict_and_canonical_case_identity() {
        let root = jails_support::scratch::ScratchDir::in_temp("normalized-test-report").unwrap();
        let directory = root.path().join("target/surefire-reports");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("TEST-PayoutTest.xml"), REPORT).unwrap();
        let report = normalized(
            root.path(),
            TestEngine::Maven,
            TestScope::Unit,
            &[],
            false,
            Some("warm partition delegated".into()),
        )
        .unwrap();
        assert!(!report.succeeded());
        assert_eq!(
            report.cases[0].selector.as_str(),
            "com.example.demo.PayoutTest#settles"
        );
        assert_eq!(report.cases[2].outcome, TestOutcome::Error);
        assert_eq!(report.cases[3].outcome, TestOutcome::Skipped);
        let json = json_line(&report);
        assert!(json.contains("\"engine\":\"maven\""));
        assert!(json.contains("\"passed\":false"));
        assert!(json.contains("warm partition delegated"));
    }

    #[test]
    fn requested_reports_exclude_stale_xml_and_mixed_reports_merge_once() {
        let root = jails_support::scratch::ScratchDir::in_temp("partitioned-test-report").unwrap();
        let directory = root.path().join("target/surefire-reports");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("TEST-PayoutTest.xml"), REPORT).unwrap();
        let build = normalized(
            root.path(),
            TestEngine::Maven,
            TestScope::Unit,
            &["PayoutTest#settles".into()],
            true,
            Some("other selector required process isolation".into()),
        )
        .unwrap();
        assert_eq!(build.cases.len(), 1, "unselected XML must not leak in");
        let mut warm = build.clone();
        warm.cases[0].engine = TestEngine::TestdV2;
        warm.cases[0].selector = TestSelector::parse("com.example.demo.PlainTest#ok").unwrap();
        warm.fallback_reasons.clear();
        let merged = merge(
            TestScope::Unit,
            &["PayoutTest#settles".into(), "PlainTest#ok".into()],
            vec![build, warm],
        )
        .unwrap();
        assert_eq!(merged.cases.len(), 2);
        assert_eq!(merged.fallback_reasons.len(), 1);
        assert!(merged.succeeded());
    }

    #[test]
    fn a_report_it_cannot_read_yields_no_cases_rather_than_no_failures() {
        // The distinction that matters: "I could not read this" must never
        // be reported as "nothing failed".
        assert!(parse("<testsuite/>").is_empty());
        assert!(parse("not xml at all").is_empty());
        assert!(parse("<testcase name=\"x\"/>").is_empty(), "no classname");
    }
}
