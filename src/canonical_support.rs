//! Exhaustive cutover registry for the advertised CLI vocabulary.
//!
//! This is intentionally code, not a roadmap table. Adding a new clap variant
//! makes the exhaustive matches fail to compile until its canonical ownership
//! is decided. Frontends use the same answer they report to readers.
//!
//! **`Native` means the compiler has a backend.** Every project authors in
//! `.jails/model.jdl`, so the generator half of this table routes nothing --
//! it is the coverage number, and the compiler refuses an unserved kind at
//! *compile* time through `component_kind_is_emitted`. A kind marked
//! `Compatibility` that the compiler actually emits under-reports coverage,
//! so a kind whose backend has landed must be moved to `Native` here.
//!
//! **Every arm below is `Native`**, and the table's job is making the next
//! clap variant a compile error until its ownership is decided.

#[cfg(test)]
use crate::ArtifactKind;
use crate::CapabilityKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Support {
    Native,
    /// **Nothing is classified this way**, and the variant stays because the
    /// registry's whole purpose is that a new clap variant cannot compile
    /// until somebody decides its ownership -- with the answer deleted, "it
    /// has no backend yet" would have no spelling and the easy thing would be
    /// to claim `Native`.
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

/// **Nothing in production consults this; the gate is why it exists.**
/// `.jails/model.toml` does not accept edits, so every generation goes to the
/// JDL frontend and an unserved kind refuses at compile time through
/// `component_kind_is_emitted`. What the exhaustive match is for is the next
/// clap variant, which cannot be added without a decision recorded here, and
/// the 39/39 coverage number the test below asserts.
#[cfg(test)]
fn generator(kind: ArtifactKind) -> Support {
    match kind {
        ArtifactKind::Scaffold
        | ArtifactKind::Controller
        | ArtifactKind::Service
        | ArtifactKind::Class
        | ArtifactKind::Interface
        | ArtifactKind::Record
        | ArtifactKind::Field
        | ArtifactKind::Factory
        | ArtifactKind::Value
        | ArtifactKind::Enum
        | ArtifactKind::Sealed
        | ArtifactKind::Strategy
        | ArtifactKind::Repo
        | ArtifactKind::Dto
        | ArtifactKind::Usecase
        | ArtifactKind::Query
        | ArtifactKind::Transition
        | ArtifactKind::Event
        | ArtifactKind::Test
        | ArtifactKind::IntegrationTest
        | ArtifactKind::Client
        | ArtifactKind::Fetcher
        | ArtifactKind::Job
        | ArtifactKind::Socket
        | ArtifactKind::Webhook
        | ArtifactKind::Auth
        | ArtifactKind::Cases
        | ArtifactKind::Idempotency
        | ArtifactKind::Handler
        | ArtifactKind::Presence
        | ArtifactKind::Command
        | ArtifactKind::Cli
        | ArtifactKind::HttpSink
        | ArtifactKind::HttpWorkflow
        | ArtifactKind::DurableJob
        | ArtifactKind::Seed
        | ArtifactKind::Search
        | ArtifactKind::Association
        | ArtifactKind::Migration => Support::Native,
    }
}

pub(crate) fn capability(kind: CapabilityKind) -> Support {
    match kind {
        CapabilityKind::Db
        | CapabilityKind::Fake
        | CapabilityKind::Api
        | CapabilityKind::Csv
        | CapabilityKind::Json
        | CapabilityKind::Http
        | CapabilityKind::Sqlite
        | CapabilityKind::H2
        | CapabilityKind::Actuator
        | CapabilityKind::Cache
        | CapabilityKind::Coverage
        | CapabilityKind::Cors
        | CapabilityKind::Observability
        | CapabilityKind::Security
        | CapabilityKind::Sse
        | CapabilityKind::Redis
        | CapabilityKind::Kafka
        | CapabilityKind::Mail
        | CapabilityKind::Toxiproxy
        | CapabilityKind::Loadtest
        | CapabilityKind::Ci
        | CapabilityKind::Docker
        | CapabilityKind::K8s
        | CapabilityKind::Format
        | CapabilityKind::Testkit
        | CapabilityKind::FastTest => Support::Native,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ValueEnum as _;

    #[test]
    fn registry_classifies_every_advertised_word() {
        let generator_count = ArtifactKind::value_variants().len();
        let capability_count = CapabilityKind::value_variants().len();
        assert_eq!(generator_count, 39);
        assert_eq!(capability_count, 25);
        assert_eq!(
            ArtifactKind::value_variants()
                .iter()
                .filter(|kind| generator(**kind).is_native())
                .count(),
            39
        );
        // All 25, so a capability added without a canonical backend fails
        // here rather than at the cutover.
        assert_eq!(
            CapabilityKind::value_variants()
                .iter()
                .filter(|kind| capability(**kind).is_native())
                .count(),
            25
        );
    }
}
