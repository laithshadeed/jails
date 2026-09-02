//! The closed vocabularies the CLI is built from.
//!
//! Only the *names* are here. What each one means is the compiler's, and
//! stays there.

use clap::ValueEnum;

/// Every artifact `jails generate` can write.
///
/// A `clap::ValueEnum`, and that must stay true: it is the only way
/// `clap_complete` can emit a static completion list for `jails g <TAB>`. It
/// lives here rather than with the generators because these are the closed
/// vocabularies the CLI is built from, and the layers below the generators
/// validate against them.
///
/// What each kind *writes* is the compiler's, and stays there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum ArtifactKind {
    /// A REST resource that runs: record, port, in-memory adapter, DTOs,
    /// service, controller and tests -- plus the JDBC adapter and forward
    /// migration when the model declares storage
    Scaffold,
    /// A Spring `@RestController` stub and its test
    Controller,
    /// A `@Component` stub and its test
    Service,
    /// A plain `final class` and its test, in the base package
    Class,
    /// A plain Java interface
    Interface,
    /// An immutable record with compact-constructor validation, plus a test
    Record,
    /// Add one component to an existing record and safely refresh unchanged
    /// derived files; edited files are reported, never overwritten
    Field,
    /// A fluent test-data builder for an existing record
    Factory,
    /// A record whose fields are all validated as a value object
    Value,
    /// An enum and its test -- the one type jails can build a sample of
    Enum,
    /// A sealed interface with one record per variant; adding one breaks the build
    Sealed,
    /// An open set: one port interface and a bean per implementation, which
    /// Spring collects into a `List<Port>`. The counterpart to `sealed`.
    #[value(alias = "rule")]
    Strategy,
    /// Repository port, a derived JDBC adapter, and a real-database IT
    #[value(alias = "repository")]
    Repo,
    /// The next `VNNN__description.sql` under db/migration; forward-only
    #[value(alias = "mig")]
    Migration,
    /// An `HttpHandler` on the JDK's own server -- no framework
    Handler,
    /// A CLI subcommand, registered in the project's dispatcher
    Command,
    /// A second CLI dispatcher, separate from App.java
    Cli,
    /// A test class per scenario in a markdown file
    Cases,
    /// A declarative HTTP client: `@HttpExchange` interface, group
    /// registration, and a test against a real socket (Spring only)
    Client,
    /// A bounded outbound HTTP fetch port with redirect revalidation, DNS
    /// pinning, SSRF protection, metrics, and real-socket adversarial tests
    /// (Spring only)
    Fetcher,
    /// Scheduled work: a `@Scheduled` component that cannot cancel its own
    /// schedule by throwing (Spring only)
    Job,
    /// A durable, bounded HTTP graph walk composed with an existing safe
    /// fetcher. Generates a PostgreSQL frontier, robots policy, canonical
    /// exact-origin traversal, status/pages/cancel API, and adversarial IT.
    /// `--on` names the fetcher; limits are request/configuration data.
    #[value(name = "http-workflow", alias = "hflow")]
    HttpWorkflow,
    /// A validated relational invariant between two existing scaffolds.
    /// `--on` names the child, `--yields` the parent, and each field is an
    /// explicit `childField=parentField` mapping. Composite mappings enforce
    /// tenant-safe ownership in PostgreSQL instead of trusting HTTP checks.
    #[value(alias = "fk")]
    Association,
    /// An HTTP delivery sink attached to an existing transactional outbox.
    /// `--on` names the use case and `--yields` its typed event. Delivery uses
    /// the event id as an idempotency key and inherits the outbox's leases,
    /// bounded retries, and terminal diagnostics.
    // The alias is `outbound`, not `webhook`: this *sends* one, but "webhook"
    // means the endpoint that receives Stripe's far more often than the client
    // that posts yours, and that kind exists too. `outbound` says which half.
    #[value(name = "http-sink", alias = "outbound")]
    HttpSink,
    /// At-most-once execution with a *retained result*: a scoped receipt keyed
    /// by request hash, so a retry replays the first response instead of being
    /// answered 409 by a unique constraint. Needs `jails add db`.
    #[value(alias = "idempotent")]
    Idempotency,
    /// A JWT issuer for this service's own tokens: the `JwtEncoder` Boot does
    /// not auto-configure, and a decoder that refuses a token with no `exp` --
    /// which every default configuration accepts. Needs `jails add security`.
    #[value(alias = "jwt")]
    Auth,
    /// An inbound webhook endpoint whose signature is checked over the raw
    /// request bytes, in constant time, with a bounded timestamp window
    #[value(alias = "hook")]
    Webhook,
    /// PostgreSQL full-text search over an existing record: a generated
    /// `tsvector` column, a GIN index, and a port with its JDBC adapter
    #[value(alias = "fts")]
    Search,
    /// PostgreSQL-backed, leased, bounded-retry work that invokes an existing
    /// generated create use case. `--on` names the use case and `--yields`
    /// names its resource; fields include the stable resource `id`.
    #[value(name = "durable-job", alias = "djob")]
    DurableJob,
    /// Request/response records for a domain type, with the mapping and a
    /// round-trip test (Spring only)
    Dto,
    /// An executable create operation over an existing scaffold: typed
    /// command, use-case port and implementation, HTTP adapter, and tests
    /// (Spring only). `--on` names the target resource.
    #[value(alias = "uc")]
    Usecase,
    /// A typed read operation over an existing scaffold: query record, port,
    /// JDBC adapter, HTTP adapter, and a real-database test. `--on` names the
    /// target resource and fields become equality filters (Spring only).
    Query,
    /// An optimistic, scope-aware update over an existing scaffold. `id`,
    /// `@scope` fields, and `version` identify the row; every other field is
    /// updated and the stored version is incremented atomically (Spring only).
    Transition,
    /// A Kafka slice: payload record, publisher, listener, and an IT against
    /// a real broker (Spring only)
    Event,
    /// A bidirectional WebSocket endpoint: a `TextWebSocketHandler`, its
    /// `WebSocketConfigurer` registration and a test. `add sse` is the
    /// server-to-client half only; this is the half a chat needs (Spring only)
    #[value(alias = "websocket", alias = "ws")]
    Socket,
    /// Who is connected, in PostgreSQL rather than in one process's memory,
    /// so two nodes give one answer. Needs `jails add db` (Spring only)
    Presence,
    /// Development data for a resource: `db/seeds/<table>.json` and a
    /// `@Profile("seed")` runner that loads it through the repository port.
    /// Needs `jails add db` and `jails add json` (Spring only)
    Seed,
    /// A `<Name>Test` skeleton
    Test,
    /// A disabled `<Name>IT` skeleton for a real boundary test; also splices
    /// the Failsafe plugin, without which no `*IT` ever runs
    #[value(name = "integration-test", alias = "it")]
    IntegrationTest,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every kind clap accepts is one the recipe table classifies.
    ///
    /// `ArtifactKind` has no `label()` of its own -- it goes through clap
    /// already -- so what is worth pinning here is that the two vocabularies
    /// have the same members.
    #[test]
    fn every_artifact_kind_round_trips_through_its_clap_name() {
        for kind in ArtifactKind::value_variants() {
            let name = kind
                .to_possible_value()
                .expect("every ArtifactKind has a clap value")
                .get_name()
                .to_string();
            assert_eq!(
                ArtifactKind::from_str(&name, false).as_ref(),
                Ok(kind),
                "`{name}` is printed in refusals, so it has to parse back"
            );
        }
    }
}
