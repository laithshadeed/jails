//! Canonical test selection, execution, and result values.
//!
//! The coordinator, build-tool adapters, and resident test engine all exchange
//! these values. Keeping selection and results here means an engine may change
//! without changing which tests were requested or how their outcomes are
//! reported.

use jails_support::Result;
use jails_support::codec::{Codec, Decoder, Encoder};
use jails_support::codec::{decode_all, encode_all};
use jails_support::identity::ProjectPath;
use std::collections::BTreeSet;

pub mod testd;

/// A class or class-and-method selector accepted by every test engine.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TestSelector(String);

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
}

impl std::fmt::Display for TestSelector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Codec for TestSelector {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(&self.0)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.string()?)
    }
}

macro_rules! closed_enum {
    ($name:ident { $($variant:ident = $tag:literal),+ $(,)? }) => {
        impl Codec for $name {
            fn encode(&self, encoder: &mut Encoder) -> Result<()> {
                encoder.tag(match self { $(Self::$variant => $tag),+ });
                Ok(())
            }

            fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
                match decoder.tag()? {
                    $($tag => Ok(Self::$variant),)+
                    other => Err(format!(
                        "unknown {} tag {other}\n       fix: upgrade jails so both protocol peers use \
                         a compatible version", stringify!($name)
                    ).into()),
                }
            }
        }
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestScope {
    Unit,
    Integration,
    All,
}
closed_enum!(TestScope { Unit = 0, Integration = 1, All = 2 });

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestCompilePolicy {
    Auto,
    Ide,
    Build,
    None,
}
closed_enum!(TestCompilePolicy { Auto = 0, Ide = 1, Build = 2, None = 3 });

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestEnginePolicy {
    Auto,
    Build,
    Warm,
}
closed_enum!(TestEnginePolicy { Auto = 0, Build = 1, Warm = 2 });

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestEngine {
    Maven,
    Gradle,
    TestdV2,
}
closed_enum!(TestEngine { Maven = 0, Gradle = 1, TestdV2 = 2 });

/// Why a selector is present in a partition or result.
#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
#[codec(unknown_fix = "upgrade jails so both protocol \
                 peers use a compatible version")]
pub enum SelectionReason {
    #[codec(tag = 0)]
    Requested,
    #[codec(tag = 1)]
    Scope(TestScope),
    #[codec(tag = 2)]
    Tag(String),
    #[codec(tag = 3)]
    Affected(ProjectPath),
    #[codec(tag = 4)]
    PreviousFailure,
    #[codec(tag = 5)]
    Widened(String),
}

#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub struct TestPartition {
    pub engine: TestEngine,
    pub selectors: Vec<TestSelector>,
    pub reasons: Vec<SelectionReason>,
}

/// The complete, engine-independent decision made before any test runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestExecutionPlanV1 {
    pub scope: TestScope,
    pub requested: Vec<TestSelector>,
    pub compile: TestCompilePolicy,
    pub engine: TestEnginePolicy,
    pub epoch: u64,
    pub partitions: Vec<TestPartition>,
}

impl TestExecutionPlanV1 {
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

impl Codec for TestExecutionPlanV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        self.scope.encode(encoder)?;
        encoder.seq(self.requested.len(), &self.requested)?;
        self.compile.encode(encoder)?;
        self.engine.encode(encoder)?;
        encoder.u64(self.epoch);
        encoder.seq(self.partitions.len(), &self.partitions)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let plan = Self {
            scope: TestScope::decode(decoder)?,
            requested: decoder.seq()?,
            compile: TestCompilePolicy::decode(decoder)?,
            engine: TestEnginePolicy::decode(decoder)?,
            epoch: decoder.u64()?,
            partitions: decoder.seq()?,
        };
        plan.validate()?;
        Ok(plan)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestCompileOwner {
    Ide,
    Maven,
    Gradle,
    None,
}
closed_enum!(TestCompileOwner { Ide = 0, Maven = 1, Gradle = 2, None = 3 });

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestOutcome {
    Passed,
    Failed,
    Skipped,
    Error,
}
closed_enum!(TestOutcome { Passed = 0, Failed = 1, Skipped = 2, Error = 3 });

#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub struct TestCaseResultV1 {
    pub engine: TestEngine,
    pub compile_owner: TestCompileOwner,
    pub selector: TestSelector,
    pub source: Option<ProjectPath>,
    pub outcome: TestOutcome,
    pub duration_us: u64,
    pub stdout_summary: String,
    pub stderr_summary: String,
    pub selection_reasons: Vec<SelectionReason>,
    pub fallback_reason: Option<String>,
}

/// One ordered report regardless of how many engines executed its cases.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestReportV1 {
    pub epoch: u64,
    /// The execution owner's verdict. A build can fail before producing a
    /// case, so this cannot be reconstructed from `cases`.
    pub passed: bool,
    pub scope: TestScope,
    pub requested: Vec<TestSelector>,
    pub cases: Vec<TestCaseResultV1>,
    pub fallback_reasons: Vec<String>,
}

impl TestReportV1 {
    pub fn succeeded(&self) -> bool {
        self.passed
    }
}

impl Codec for TestReportV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        unique_selectors("requested test", &self.requested)?;
        encoder.u64(self.epoch);
        encoder.bool(self.passed);
        self.scope.encode(encoder)?;
        encoder.seq(self.requested.len(), &self.requested)?;
        encoder.seq(self.cases.len(), &self.cases)?;
        encode_all(encoder, &self.fallback_reasons, |reason, encoder| {
            encoder.string(reason)
        })
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let report = Self {
            epoch: decoder.u64()?,
            passed: decoder.bool()?,
            scope: TestScope::decode(decoder)?,
            requested: decoder.seq()?,
            cases: decoder.seq()?,
            fallback_reasons: decode_all(decoder, |decoder| decoder.string())?,
        };
        unique_selectors("requested test", &report.requested)?;
        Ok(report)
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

/// Whether the warm engine's console launcher is on this project's test
/// classpath, read off the build file the project declares it in.
///
/// **The warm engine has a dependency, and the plan has to ask about it like
/// it asks about anything else.** `junit-platform-console` is what
/// `jails testd` runs; it reaches a project through the `fast-test`
/// capability, which is installed by an ordinary transition when the reader
/// asks for the warm engine by name (`--fast`, `--engine warm`,
/// `--affected`). Automatic selection asks for nobody's pom to change, so on
/// a project that never opted in this answers `false` and the plan widens to
/// the build tool -- where it used to select the warm engine anyway and die
/// inside the daemon, telling the reader to run a *different* command.
pub(crate) fn warm_launcher_declared(build_file: &str) -> bool {
    build_file.contains("junit-platform-console")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T: Codec + Eq + std::fmt::Debug>(value: &T) {
        let mut encoder = Encoder::new();
        value.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        let decoded = T::decode(&mut decoder).unwrap();
        decoder.finish().unwrap();
        assert_eq!(&decoded, value);
    }

    fn selector(text: &str) -> TestSelector {
        TestSelector::parse(text).unwrap()
    }

    #[test]
    fn selectors_are_validated_at_both_boundaries() {
        for invalid in ["", "   ", "Type#", "#method", "Type#one#two", "Type\n"] {
            assert!(
                TestSelector::parse(invalid).is_err(),
                "accepted {invalid:?}"
            );
        }
        assert_eq!(selector("ExampleTest#works").as_str(), "ExampleTest#works");
    }

    #[test]
    fn execution_plan_round_trips() {
        let selected = selector("ExampleTest#works");
        round_trip(&TestExecutionPlanV1 {
            scope: TestScope::Unit,
            requested: vec![selected.clone()],
            compile: TestCompilePolicy::Auto,
            engine: TestEnginePolicy::Auto,
            epoch: 42,
            partitions: vec![TestPartition {
                engine: TestEngine::TestdV2,
                selectors: vec![selected],
                reasons: vec![SelectionReason::Requested],
            }],
        });
    }

    #[test]
    fn report_round_trips_and_derives_success() {
        let report = TestReportV1 {
            epoch: 42,
            passed: true,
            scope: TestScope::Unit,
            requested: vec![selector("ExampleTest#works")],
            cases: vec![TestCaseResultV1 {
                engine: TestEngine::TestdV2,
                compile_owner: TestCompileOwner::Ide,
                selector: selector("ExampleTest#works"),
                source: Some(ProjectPath::parse("src/test/java/ExampleTest.java").unwrap()),
                outcome: TestOutcome::Passed,
                duration_us: 8_000,
                stdout_summary: String::new(),
                stderr_summary: String::new(),
                selection_reasons: vec![SelectionReason::Requested],
                fallback_reason: None,
            }],
            fallback_reasons: Vec::new(),
        };
        assert!(report.succeeded());
        round_trip(&report);
    }

    #[test]
    fn plan_refuses_dropped_and_duplicated_selectors() {
        let selected = selector("ExampleTest#works");
        let omitted = TestExecutionPlanV1 {
            scope: TestScope::Unit,
            requested: vec![selected.clone()],
            compile: TestCompilePolicy::None,
            engine: TestEnginePolicy::Warm,
            epoch: 1,
            partitions: Vec::new(),
        };
        assert!(omitted.validate().is_err());

        let duplicated = TestExecutionPlanV1 {
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

    #[test]
    fn closed_enums_refuse_unknown_tags() {
        let bytes = [255];
        let mut decoder = Decoder::new(&bytes).unwrap();
        let error = TestEngine::decode(&mut decoder).unwrap_err();
        assert!(error.to_string().contains("unknown TestEngine tag 255"));
    }
}
