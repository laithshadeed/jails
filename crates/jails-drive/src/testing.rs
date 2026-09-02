//! The one test-execution vocabulary: what was selected, how it ran, what
//! came back.
//!
//! Three engines execute tests -- the build tool, JUnit's console launcher and
//! the resident `testd` JVM -- and every one of them is asked for the same
//! thing and answers in the same words. `TestPlan` is the decision made
//! before anything runs; `TestReport` is what came back, whichever engine
//! produced it.
//!
//! **Nothing here is a wire format.** These values are passed between modules
//! in one process, so they carry no encoding at all. The daemon's socket is
//! the one place a value leaves this process, and `testd::protocol` encodes
//! what crosses it -- an encoding *of* this vocabulary, never a second copy of
//! it. The words a machine reads (`--output json`, the daemon's frames) come
//! from the `name` methods below, so a spelling has one owner.

use jails_support::Result;
use std::collections::BTreeSet;

/// Split `Class#method` into its two halves. Anything with no `#` is all
/// class.
///
/// One owner, because three readers ask this question -- the filter a person
/// typed, the qualifier that resolves a bare class name, and the matcher that
/// pairs a report case with a requested selector -- and a fourth spelling of
/// `split_once('#')` is how they drift apart.
pub(crate) fn split_selector(text: &str) -> (&str, Option<&str>) {
    match text.split_once('#') {
        Some((class, method)) => (class, Some(method)),
        None => (text, None),
    }
}

/// A class or class-and-method selector accepted by every test engine.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub(crate) struct TestSelector(String);

impl TestSelector {
    pub fn parse(text: &str) -> Result<Self> {
        if text.contains(['\0', '\n', '\r']) {
            return Err(jails_support::Failure::Told(
                "test selector contains a control character\n       fix: pass a class or \
                 `Class#method` selector on one line"
                    .to_string(),
            ));
        }
        let text = text.trim();
        if text.is_empty() {
            return Err(jails_support::Failure::Told(
                "test selector is empty\n       fix: omit the selector to run the selected scope, or \
                 pass a class or `Class#method`"
                    .to_string(),
            ));
        }
        if text.split('#').count() > 2 {
            return Err(jails_support::Failure::Told(format!(
                "test selector `{text}` contains more than one `#`\n       fix: use \
                 `Class#method` with exactly one separator"
            )));
        }
        if text.split('#').any(str::is_empty) {
            return Err(jails_support::Failure::Told(format!(
                "test selector `{text}` has an empty class or method\n       fix: name both \
                 sides as `Class#method`"
            )));
        }
        Ok(Self(text.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The class half, and the method half when one was named.
    pub fn split(&self) -> (&str, Option<&str>) {
        split_selector(&self.0)
    }
}

impl std::fmt::Display for TestSelector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for TestSelector {
    type Error = jails_support::Failure;

    fn try_from(text: String) -> Result<Self> {
        Self::parse(&text)
    }
}

impl From<TestSelector> for String {
    fn from(selector: TestSelector) -> Self {
        selector.0
    }
}

/// Which tests a run is about, before any selector narrows it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum TestScope {
    Unit,
    Integration,
    All,
}

impl TestScope {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Integration => "integration",
            Self::All => "all",
        }
    }
}

/// Who is allowed to compile before the tests run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum TestCompilePolicy {
    Auto,
    Ide,
    Build,
    None,
}

/// Which engine the reader will accept.
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum TestEnginePolicy {
    Auto,
    Build,
    Warm,
}

/// The engine that actually executed a partition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestEngine {
    Maven,
    Gradle,
    TestdV2,
}

impl TestEngine {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Maven => "maven",
            Self::Gradle => "gradle",
            Self::TestdV2 => "testd-v2",
        }
    }

    /// Who compiled the classes this engine ran.
    ///
    /// Derived rather than carried: the warm daemon never compiles, and each
    /// build tool always compiles its own partition, so a stored field could
    /// only ever disagree with the engine beside it.
    pub(crate) fn compile_owner_name(self) -> &'static str {
        match self {
            Self::Maven => "maven",
            Self::Gradle => "gradle",
            Self::TestdV2 => "none",
        }
    }
}

/// Why a selector is present in a partition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SelectionReason {
    /// The reader named it.
    Requested,
    /// It is in the requested scope.
    Scope(TestScope),
    /// The plan widened past what was asked for, and this says why.
    Widened(String),
}

/// One engine and the selectors it owns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TestPartition {
    pub engine: TestEngine,
    pub selectors: Vec<TestSelector>,
    pub reasons: Vec<SelectionReason>,
}

/// The complete, engine-independent decision made before any test runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TestPlan {
    pub scope: TestScope,
    pub requested: Vec<TestSelector>,
    pub compile: TestCompilePolicy,
    pub engine: TestEnginePolicy,
    pub partitions: Vec<TestPartition>,
}

impl TestPlan {
    pub fn validate(&self) -> Result<()> {
        unique_selectors("requested test", &self.requested)?;
        let mut partitioned = BTreeSet::new();
        for partition in &self.partitions {
            unique_selectors("partitioned test", &partition.selectors)?;
            for selector in &partition.selectors {
                if !partitioned.insert(selector) {
                    return Err(format!(
                        "test selector `{selector}` occurs in more than one execution partition\n       \
                         fix: assign each selector to exactly one engine"
                    )
                    .into());
                }
            }
        }
        if !self.requested.is_empty()
            && self
                .requested
                .iter()
                .any(|selector| !partitioned.contains(selector))
        {
            return Err(
                "an explicitly requested test was omitted from execution partitions\n       fix: \
                 delegate every ineligible selector instead of dropping it"
                    .into(),
            );
        }
        if self.engine == TestEnginePolicy::Build
            && self
                .partitions
                .iter()
                .any(|partition| partition.engine == TestEngine::TestdV2)
        {
            return Err(
                "build engine policy contains a warm testd partition\n       fix: route every \
                        partition to Maven or Gradle"
                    .into(),
            );
        }
        if self.engine == TestEnginePolicy::Warm
            && self
                .partitions
                .iter()
                .any(|partition| partition.engine != TestEngine::TestdV2)
        {
            return Err(
                "warm engine policy contains a build-tool partition\n       fix: refuse the \
                        ineligible selector or choose `--engine auto`"
                    .into(),
            );
        }
        Ok(())
    }
}

/// What happened to one test.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TestOutcome {
    Passed,
    Failed,
    Skipped,
    Error,
}

impl TestOutcome {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Error => "error",
        }
    }
}

/// One executed test, with the engine that ran it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TestCaseResult {
    pub engine: TestEngine,
    pub selector: TestSelector,
    pub outcome: TestOutcome,
    pub duration_us: u64,
    pub stdout_summary: String,
    pub stderr_summary: String,
    pub fallback_reason: Option<String>,
}

/// One ordered report regardless of how many engines executed its cases.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TestReport {
    pub epoch: u64,
    /// The execution owner's verdict. A build can fail before producing a
    /// case, so this cannot be reconstructed from `cases`.
    pub passed: bool,
    pub scope: TestScope,
    pub requested: Vec<TestSelector>,
    pub cases: Vec<TestCaseResult>,
    pub fallback_reasons: Vec<String>,
}

impl TestReport {
    pub fn succeeded(&self) -> bool {
        self.passed
    }
}

fn unique_selectors(what: &str, selectors: &[TestSelector]) -> Result<()> {
    let mut unique = BTreeSet::new();
    for selector in selectors {
        if !unique.insert(selector) {
            return Err(format!(
                "{what} selector `{selector}` is duplicated\n       fix: deduplicate selectors before \
                 constructing the execution plan"
            )
            .into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selector(text: &str) -> TestSelector {
        TestSelector::parse(text).unwrap()
    }

    #[test]
    fn selectors_are_validated_and_split_once() {
        for invalid in ["", "   ", "Type#", "#method", "Type#one#two", "Type\n"] {
            assert!(
                TestSelector::parse(invalid).is_err(),
                "accepted {invalid:?}"
            );
        }
        let parsed = selector("ExampleTest#works");
        assert_eq!(parsed.as_str(), "ExampleTest#works");
        assert_eq!(parsed.split(), ("ExampleTest", Some("works")));
        assert_eq!(selector("ExampleTest").split(), ("ExampleTest", None));
    }

    /// A selector arriving from the daemon is parsed, not trusted: the same
    /// constructor the CLI uses is the only way to make one.
    #[test]
    fn a_selector_on_the_wire_goes_through_its_constructor() {
        assert_eq!(
            serde_json::from_str::<TestSelector>("\"a.B#c\"").unwrap(),
            selector("a.B#c")
        );
        assert!(serde_json::from_str::<TestSelector>("\"a#b#c\"").is_err());
        assert_eq!(
            serde_json::to_string(&selector("a.B#c")).unwrap(),
            "\"a.B#c\""
        );
    }

    #[test]
    fn plan_refuses_dropped_and_duplicated_selectors() {
        let selected = selector("ExampleTest#works");
        let omitted = TestPlan {
            scope: TestScope::Unit,
            requested: vec![selected.clone()],
            compile: TestCompilePolicy::None,
            engine: TestEnginePolicy::Warm,
            partitions: Vec::new(),
        };
        assert!(omitted.validate().is_err());

        let duplicated = TestPlan {
            partitions: vec![
                TestPartition {
                    engine: TestEngine::Maven,
                    selectors: vec![selected.clone()],
                    reasons: vec![],
                },
                TestPartition {
                    engine: TestEngine::TestdV2,
                    selectors: vec![selected.clone()],
                    reasons: vec![],
                },
            ],
            requested: vec![selected],
            engine: TestEnginePolicy::Auto,
            ..omitted
        };
        assert!(duplicated.validate().is_err());
    }

    /// The machine-readable spellings have one owner, and the warm engine
    /// never owns compilation.
    #[test]
    fn every_engine_names_itself_and_its_compile_owner() {
        assert_eq!(TestEngine::TestdV2.name(), "testd-v2");
        assert_eq!(TestEngine::TestdV2.compile_owner_name(), "none");
        assert_eq!(TestEngine::Maven.compile_owner_name(), "maven");
        assert_eq!(TestOutcome::Error.name(), "error");
        assert_eq!(TestScope::All.name(), "all");
    }
}
