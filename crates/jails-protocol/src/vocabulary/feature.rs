//! What the build has to *do*, named by what it is for.
//!
//! `pending.md` §3's naming debt, paid. A claim on a build plugin used to be
//! keyed by a Maven coordinate — `org.jacoco:jacoco-maven-plugin` — which is a
//! stable name for a closed set of three and **not a thing Gradle resolves**.
//! A Gradle project's coverage claim was therefore filed under the name of a
//! plugin that project does not have and will never have, and two places had
//! to map the coordinate back onto what it was for before they could act.
//!
//! So the key is the feature, and the Maven plugin block is one *rendering* of
//! it, exactly as the Gradle block is the other. Two consequences worth having:
//!
//! - **The producer says which feature**, rather than jails inferring it from
//!   an artifact id. `Change.plugins` carries a `BuildFeature`, so there is no
//!   coordinate to fail to recognise -- and `require_renderable_plugins`, the
//!   run-time refusal that existed for exactly that failure, is gone. Adding a
//!   variant here is a compile error in `gradle.rs`'s four exhaustive matches
//!   until somebody writes the Gradle side, which is a better guarantee than a
//!   message.
//! - **The coordinate is still checked.** [`BuildFeature::of_maven_plugin`]
//!   survives as a *confirmation*: the plugin XML a claim carries has to be the
//!   one this feature means, or the two halves of the claim describe different
//!   things.

use crate::Result;
use jails_support::codec::{Codec, Decoder, Encoder};

/// A thing the build has to do, named by what it is for.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum BuildFeature {
    /// Run `*IT` classes, and fail the build when they fail.
    IntegrationTests,
    /// Enforce line coverage during `check`.
    Coverage,
    /// Format on `check`, and offer a task that fixes.
    Formatting,
}

impl BuildFeature {
    /// Fixed wire tags. These numbers may never be reused for a different
    /// meaning.
    fn tag(self) -> u8 {
        match self {
            Self::IntegrationTests => 0,
            Self::Coverage => 1,
            Self::Formatting => 2,
        }
    }

    /// The feature a Maven plugin coordinate stands for.
    ///
    /// Closed on purpose, and a *check* rather than a derivation now: the
    /// producer of a claim states its feature, and this confirms that the
    /// plugin block it carries is the one that feature means.
    pub fn of_maven_plugin(artifact_id: &str) -> Option<Self> {
        match artifact_id {
            "maven-failsafe-plugin" => Some(Self::IntegrationTests),
            "jacoco-maven-plugin" => Some(Self::Coverage),
            "spotless-maven-plugin" => Some(Self::Formatting),
            _ => None,
        }
    }

    /// The Maven plugin that provides it.
    ///
    /// The inverse of [`Self::of_maven_plugin`], and the reason both exist: a
    /// Maven build is configured by naming a plugin, so unsplicing one needs
    /// the coordinate even though the claim is keyed by the feature. Total,
    /// because every feature jails knows has a Maven plugin -- that is what
    /// made the coordinate a plausible key for as long as Maven was the only
    /// build tool.
    pub fn maven_artifact_id(self) -> &'static str {
        match self {
            Self::IntegrationTests => "maven-failsafe-plugin",
            Self::Coverage => "jacoco-maven-plugin",
            Self::Formatting => "spotless-maven-plugin",
        }
    }

    /// What this feature is for, in a sentence fragment a refusal can use.
    pub fn purpose(self) -> &'static str {
        match self {
            Self::IntegrationTests => "run `*IT` integration tests",
            Self::Coverage => "enforce line coverage",
            Self::Formatting => "check formatting",
        }
    }

    /// The stable name, for a report and for a marker comment.
    pub fn label(self) -> &'static str {
        match self {
            Self::IntegrationTests => "integration-tests",
            Self::Coverage => "coverage",
            Self::Formatting => "formatting",
        }
    }
}

impl std::fmt::Display for BuildFeature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl Codec for BuildFeature {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::IntegrationTests,
            1 => Self::Coverage,
            2 => Self::Formatting,
            other => return Err(format!("unknown build feature tag {other}").into()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_feature_round_trips_and_has_a_distinct_tag() {
        let all = [
            BuildFeature::IntegrationTests,
            BuildFeature::Coverage,
            BuildFeature::Formatting,
        ];
        let mut tags = std::collections::BTreeSet::new();
        for feature in all {
            assert!(tags.insert(feature.tag()), "{feature} duplicates a tag");
            let mut encoder = Encoder::new();
            feature.encode(&mut encoder).unwrap();
            let bytes = encoder.finish().unwrap();
            let mut decoder = Decoder::new(&bytes).unwrap();
            assert_eq!(BuildFeature::decode(&mut decoder).unwrap(), feature);
        }
    }

    /// The three plugins jails splices, and nothing else.
    #[test]
    fn the_maven_plugins_jails_writes_all_name_a_feature() {
        assert_eq!(
            BuildFeature::of_maven_plugin("maven-failsafe-plugin"),
            Some(BuildFeature::IntegrationTests)
        );
        assert_eq!(
            BuildFeature::of_maven_plugin("jacoco-maven-plugin"),
            Some(BuildFeature::Coverage)
        );
        assert_eq!(
            BuildFeature::of_maven_plugin("spotless-maven-plugin"),
            Some(BuildFeature::Formatting)
        );
        assert_eq!(BuildFeature::of_maven_plugin("maven-shade-plugin"), None);
    }
}
