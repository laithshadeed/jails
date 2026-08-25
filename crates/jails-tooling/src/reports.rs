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
}

impl Case {
    /// The filter that reruns exactly this test.
    pub fn selector(&self) -> String {
        format!("{}#{}", short_class(&self.class), self.method)
    }

    /// The same case as a Gradle `--tests` pattern.
    ///
    /// `Class.method`, and the class **fully qualified** -- Gradle matches the
    /// pattern against the full name and treats a bare one as a prefix, so the
    /// short form silently selects every class in every package whose name
    /// happens to start with it. Surefire's `#` spelling is a different
    /// language, which is why this is a second method rather than a `replace`
    /// at the call site.
    pub fn pattern(&self) -> String {
        format!("{}.{}", self.class, self.method)
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

/// Which tests failed last time, as Gradle `--tests` patterns.
///
/// The counterpart of [`failed_selectors`], and separate for the reason
/// [`Case::pattern`] is separate from [`Case::selector`]: the two build tools
/// take different spellings, and one function returning whichever the caller
/// happened to want is how a selector reaches the wrong tool.
pub(crate) fn failed_patterns(root: &Path) -> Vec<String> {
    let mut patterns: Vec<String> = cases(root)
        .into_iter()
        .filter(|case| case.failed)
        .map(|case| case.pattern())
        .collect();
    patterns.sort();
    patterns.dedup();
    patterns
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

/// The `count` slowest cases, slowest first.
pub(crate) fn slowest(root: &Path, count: usize) -> Vec<Case> {
    let mut all = cases(root);
    // Descending by time. `total_cmp` rather than `partial_cmp().unwrap()`:
    // a malformed `time` attribute parses to NaN, and a comparator that
    // panics on one bad report is worse than one that sorts it to the end.
    all.sort_by(|a, b| b.seconds.total_cmp(&a.seconds));
    all.truncate(count);
    all
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
            });
        }
        rest = &after[close..];
    }
    found
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

/// The finished run, as data.
///
/// `passed` is the build's own verdict rather than "no failed cases": a build
/// can fail before a single test runs -- a compile error, a missing dependency
/// -- and an empty failure list would then read as success. The `cases` array
/// says what actually executed, which is the other half a consumer needs to
/// tell "all green" from "nothing ran".
pub(crate) fn report_json(root: &Path, passed: bool) -> Result<()> {
    let cases = cases(root);
    let rows: Vec<String> = cases
        .iter()
        .map(|case| {
            format!(
                "    {{\"class\": {}, \"method\": {}, \"seconds\": {:.3}, \"failed\": {}, \
                 \"selector\": {}}}",
                crate::json::string(&case.class),
                crate::json::string(&case.method),
                case.seconds,
                case.failed,
                crate::json::string(&case.selector())
            )
        })
        .collect();
    let failed = cases.iter().filter(|case| case.failed).count();
    println!(
        "{{\n  \"schema_version\": 1,\n  \"passed\": {passed},\n  \"total\": {},\n  \
         \"failed\": {failed},\n  \"cases\": [\n{}\n  ]\n}}",
        cases.len(),
        rows.join(",\n")
    );
    if passed {
        Ok(())
    } else {
        Err(jails_support::Failure::Reported)
    }
}
/// The slowest tests of the run that just finished.
///
/// Read from the reports rather than timed here: Maven already measured
/// each one, and a wall-clock number jails invented would include its own
/// startup.
pub(crate) fn report_slowest(root: &Path, count: usize) {
    let slowest = slowest(root, count);
    if slowest.is_empty() {
        println!();
        println!("jails: no test reports to read -- nothing ran, or the build failed first.");
        return;
    }
    println!();
    println!("slowest {} test(s):", slowest.len());
    for case in slowest {
        println!("  {:>8.2}s  {}", case.seconds, case.selector());
    }
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
        // Fully qualified, and dotted: Gradle matches `--tests` against the
        // whole name and treats a bare one as a prefix, so the short form
        // would select every class in every package starting with it.
        assert_eq!(gradle[1].pattern(), "com.example.SampleTest.failsOnPurpose");
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
    fn a_report_it_cannot_read_yields_no_cases_rather_than_no_failures() {
        // The distinction that matters: "I could not read this" must never
        // be reported as "nothing failed".
        assert!(parse("<testsuite/>").is_empty());
        assert!(parse("not xml at all").is_empty());
        assert!(parse("<testcase name=\"x\"/>").is_empty(), "no classname");
    }
}
