//! A name that already carries its kind's suffix must not get it twice.

use super::kind::ArtifactKind;

/// The suffix a kind's principal Java type carries, if it has one.
pub(crate) fn kind_suffix(kind: ArtifactKind) -> Option<&'static str> {
    match kind {
        ArtifactKind::Controller => Some("Controller"),
        ArtifactKind::Service => Some("Service"),
        ArtifactKind::Repo => Some("Repository"),
        ArtifactKind::Cli => Some("Cli"),
        ArtifactKind::Job | ArtifactKind::DurableJob => Some("Job"),
        ArtifactKind::HttpWorkflow => Some("Workflow"),
        ArtifactKind::Client => Some("Client"),
        ArtifactKind::Fetcher => Some("Fetcher"),
        ArtifactKind::Usecase => Some("UseCase"),
        ArtifactKind::Query => Some("Query"),
        ArtifactKind::Socket => Some("SocketHandler"),
        ArtifactKind::Presence => Some("Presence"),
        ArtifactKind::Seed => Some("Seeder"),
        ArtifactKind::Test => Some("Test"),
        ArtifactKind::IntegrationTest => Some("IT"),
        ArtifactKind::HttpSink
        | ArtifactKind::Scaffold
        | ArtifactKind::Class
        | ArtifactKind::Interface
        | ArtifactKind::Record
        | ArtifactKind::Field
        | ArtifactKind::Factory
        | ArtifactKind::Value
        | ArtifactKind::Enum
        | ArtifactKind::Sealed
        | ArtifactKind::Strategy
        | ArtifactKind::Migration
        | ArtifactKind::Handler
        | ArtifactKind::Command
        | ArtifactKind::Cases
        | ArtifactKind::Association
        | ArtifactKind::Idempotency
        | ArtifactKind::Auth
        | ArtifactKind::Webhook
        | ArtifactKind::Search
        | ArtifactKind::Dto
        | ArtifactKind::Transition
        | ArtifactKind::Event => None,
    }
}

/// The name a declaration is recorded under: capitalised, without a redundant
/// kind suffix. `cases` and `migration` names are file names and stay as typed.
pub fn recorded_name(kind: ArtifactKind, name: &str) -> String {
    if matches!(kind, ArtifactKind::Cases | ArtifactKind::Migration) {
        return name.to_string();
    }
    strip_redundant_suffix(kind, &capitalize(name))
}

pub fn strip_redundant_suffix(kind: ArtifactKind, name: &str) -> String {
    match kind_suffix(kind) {
        Some(suffix) => match name.strip_suffix(suffix) {
            Some(stem) if !stem.is_empty() => stem.to_string(),
            _ => name.to_string(),
        },
        None => name.to_string(),
    }
}

/// Upper-case the first character.
pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
