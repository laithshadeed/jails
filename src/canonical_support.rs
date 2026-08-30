//! Exhaustive cutover registry for the advertised CLI vocabulary.
//!
//! This is intentionally code, not a roadmap table. Adding a new clap variant
//! makes the exhaustive matches fail to compile until its canonical ownership
//! is decided. Frontends use the same answer they report to readers.

use crate::add::Capability;
use crate::generate::ArtifactKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Support {
    Native,
    Compatibility,
}

impl Support {
    pub(crate) fn is_native(self) -> bool {
        self == Self::Native
    }
}

pub(crate) fn generator(kind: ArtifactKind) -> Support {
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
        | ArtifactKind::Client => Support::Native,
        ArtifactKind::Migration
        | ArtifactKind::Handler
        | ArtifactKind::Command
        | ArtifactKind::Cli
        | ArtifactKind::Cases
        | ArtifactKind::Fetcher
        | ArtifactKind::Job
        | ArtifactKind::HttpWorkflow
        | ArtifactKind::Association
        | ArtifactKind::HttpSink
        | ArtifactKind::Idempotency
        | ArtifactKind::Auth
        | ArtifactKind::Webhook
        | ArtifactKind::Search
        | ArtifactKind::DurableJob
        | ArtifactKind::Socket
        | ArtifactKind::Presence
        | ArtifactKind::Seed => Support::Compatibility,
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
    use clap::ValueEnum as _;

    #[test]
    fn registry_classifies_every_advertised_word() {
        let generator_count = ArtifactKind::value_variants().len();
        let capability_count = Capability::value_variants().len();
        assert_eq!(generator_count, 39);
        assert_eq!(capability_count, 25);
        assert_eq!(
            ArtifactKind::value_variants()
                .iter()
                .filter(|kind| generator(**kind).is_native())
                .count(),
            21
        );
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
