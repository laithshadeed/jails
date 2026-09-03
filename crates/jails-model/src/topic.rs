//! What `jails explain` can be asked about: a generator kind or a capability.
//!
//! **A union of two closed sets, computed from them rather than restated.**
//! The two vocabularies have one owner each in this crate, and a third list
//! naming their members would be exactly the drift the owners exist to
//! prevent. [`ExplainTopic`]'s `clap::ValueEnum` is therefore hand-written
//! and delegates: the variants are whatever [`crate::ArtifactKind`] and
//! [`crate::CapabilityKind`] declare, and each one's spelling, aliases and
//! help come from the enum it belongs to.
//!
//! The two sets are disjoint today (thirty-eight kinds, twenty-five
//! capabilities, no shared word) and
//! `no_capability_is_spelled_like_a_generator_kind` keeps them that way,
//! because one positional argument cannot resolve a word that is both.

use crate::{ArtifactKind, CapabilityKind};

/// One thing `jails explain` can be asked about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExplainTopic {
    /// A generator kind: what `jails g <kind>` writes.
    Kind(ArtifactKind),
    /// A capability: what `jails add <capability>` installs.
    Capability(CapabilityKind),
}

#[cfg(feature = "cli")]
mod cli {
    use super::ExplainTopic;
    use crate::{ArtifactKind, CapabilityKind};
    use clap::ValueEnum;
    use std::sync::OnceLock;

    static VARIANTS: OnceLock<Vec<ExplainTopic>> = OnceLock::new();

    impl ValueEnum for ExplainTopic {
        fn value_variants<'a>() -> &'a [Self] {
            VARIANTS.get_or_init(|| {
                ArtifactKind::value_variants()
                    .iter()
                    .copied()
                    .map(ExplainTopic::Kind)
                    .chain(
                        CapabilityKind::value_variants()
                            .iter()
                            .copied()
                            .map(ExplainTopic::Capability),
                    )
                    .collect()
            })
        }

        fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
            match self {
                ExplainTopic::Kind(kind) => kind.to_possible_value(),
                ExplainTopic::Capability(capability) => capability.to_possible_value(),
            }
        }
    }
}

#[cfg(all(test, feature = "cli"))]
mod tests {
    use super::*;
    use clap::ValueEnum;

    /// One positional argument cannot resolve a word that is both a kind and
    /// a capability, and the first match would win silently.
    #[test]
    fn no_capability_is_spelled_like_a_generator_kind() {
        let name = |topic: &ExplainTopic| {
            topic
                .to_possible_value()
                .expect("every topic has a spelling")
                .get_name()
                .to_string()
        };
        let mut seen = std::collections::BTreeMap::new();
        for topic in ExplainTopic::value_variants() {
            let previous = seen.insert(name(topic), *topic);
            assert!(
                previous.is_none(),
                "`{}` is spelled by two vocabularies: {previous:?} and {topic:?}",
                name(topic)
            );
        }
    }

    /// The union is the two sets, not a copy of either.
    #[test]
    fn the_topics_are_every_kind_and_every_capability() {
        assert_eq!(
            ExplainTopic::value_variants().len(),
            ArtifactKind::value_variants().len() + CapabilityKind::value_variants().len()
        );
    }
}
