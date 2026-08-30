//! Exhaustive cutover registry for the advertised CLI vocabulary.
//!
//! This is intentionally code, not a roadmap table. Adding a new clap variant
//! makes the exhaustive matches fail to compile until its canonical ownership
//! is decided. Frontends use the same answer they report to readers.
//!
//! **`Native` means the compiler has a backend.** The generator half of this
//! registry is gone: `model_generate_jdl::run` is an exhaustive match over
//! `ArtifactKind` and is the only route a generator has, so a kind added
//! without a canonical backend is a compile error rather than a row somebody
//! has to remember to flip. What is left is the capability side, which
//! dispatches on this classifier, and the counts the cutover is measured on.

use crate::add::Capability;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Support {
    Native,
    /// **Nothing is classified this way any more**, and the variant stays
    /// because the registry's whole purpose is that a new clap variant cannot
    /// compile until somebody decides its ownership -- with the answer
    /// deleted, "it has no backend yet" would have no spelling and the easy
    /// thing would be to claim `Native`.
    ///
    /// `expect` rather than `allow` so the attribute is itself a ratchet: the
    /// moment a kind is classified here the expectation is unfulfilled, the
    /// build says so, and this line comes off in the same change.
    #[expect(dead_code, reason = "39/39: no advertised kind is on the legacy route")]
    Compatibility,
}

impl Support {
    pub(crate) fn is_native(self) -> bool {
        self == Self::Native
    }
}

pub(crate) fn capability(kind: Capability) -> Support {
    match kind {
        Capability::Db
        | Capability::Fake
        | Capability::Api
        | Capability::Csv
        | Capability::Json
        | Capability::Http
        | Capability::Sqlite
        | Capability::H2
        | Capability::Actuator
        | Capability::Cache
        | Capability::Coverage
        | Capability::Cors
        | Capability::Observability
        | Capability::Security
        | Capability::Sse
        | Capability::Redis
        | Capability::Kafka
        | Capability::Mail
        | Capability::Toxiproxy
        | Capability::Loadtest
        | Capability::Ci
        | Capability::Docker
        | Capability::K8s
        | Capability::Format
        | Capability::Testkit => Support::Native,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::ArtifactKind;
    use clap::ValueEnum as _;

    /// **The generator half of this row is a compile error now.**
    /// `model_generate_jdl::run` is an exhaustive match over `ArtifactKind`
    /// and is the only route a generator has, so a kind added without a
    /// canonical backend does not build. What is left to hold here is the
    /// number itself -- 39 advertised words, so a kind added without being
    /// counted still surfaces -- and the capability side, which dispatches on
    /// a classifier rather than a match.
    #[test]
    fn registry_classifies_every_advertised_word() {
        let generator_count = ArtifactKind::value_variants().len();
        let capability_count = Capability::value_variants().len();
        assert_eq!(generator_count, 39);
        assert_eq!(capability_count, 25);
        // All 25. `format`, `ci`, `docker` and `k8s` were the last four --
        // `plan.md` P13.8 measured them and this is where that number lives,
        // so a capability added without a canonical backend fails here rather
        // than discovering it at the cutover.
        assert_eq!(
            Capability::value_variants()
                .iter()
                .filter(|kind| capability(**kind).is_native())
                .count(),
            25
        );
    }
}
