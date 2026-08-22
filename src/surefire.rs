//! A deliberately small reader for Surefire's and Failsafe's XML reports.
//!
//! Everything `jails test` can say about the *last* run comes from
//! `target/surefire-reports/` and `target/failsafe-reports/`: which tests
//! failed, and which ones took the time. Maven already wrote it; asking it
//! again costs a build.
//!
//! **Not an XML parser, and must not grow into one**, for the same reason
//! `java.rs` is not a Java parser. It reads two things out of a
//! `<testcase>` element -- the attributes, and whether a `<failure>` or
//! `<error>` child follows before the element ends -- and gives up on
//! anything it does not recognise rather than guessing. A report it cannot
//! read is reported as no data, never as "nothing failed": the difference
//! between those two is exactly the kind of quiet wrong answer that makes a
//! tool untrustworthy.

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
    pub(crate) fn selector(&self) -> String {
        format!("{}#{}", short_class(&self.class), self.method)
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
pub(crate) fn cases(root: &Path) -> Vec<Case> {
    let mut found = Vec::new();
    for dir in ["target/surefire-reports", "target/failsafe-reports"] {
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
        let method = attribute(attributes, "name").unwrap_or_default();
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

#[cfg(test)]
mod tests {
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
