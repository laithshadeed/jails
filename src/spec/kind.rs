//! The closed vocabularies the CLI is built from.
//!
//! [`Capability`] lives here rather than beside `add::plan_for` because
//! `config.rs` validates a manifest's `capabilities` list against it, and
//! `config` is below the capability layer. Deriving the valid names from the
//! enum instead of restating them is the point -- a capability added without a
//! thought for the manifest is then automatically valid in it -- and doing that
//! across a layer boundary is what made `config` and `add` mutually dependent.
//!
//! Only the *names* are here. What a capability installs is
//! `add::plan_for`'s, and stays there.

use clap::ValueEnum;

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Capability {
    /// PostgreSQL + Flyway + Testcontainers + a compose service; raw SQL only, never an ORM
    #[value(alias = "postgres")]
    Db,
    /// Apache Kafka client + a compose broker (KRaft, no ZooKeeper)
    Kafka,
    /// Read CSV files into records (Apache Commons CSV)
    Csv,
    /// SQLite persistence: JDBC connections and a migration runner (sqlite-jdbc)
    Sqlite,
    /// Read and write JSON (Jackson databind)
    Json,
    /// Deterministic test helpers: clocks, ids, fixtures, in-process CLI runs
    Testkit,
    /// A scripted test double for any interface, driven by a lambda
    Fake,
    /// An HTTP server on the JDK's own httpserver -- no framework
    Http,
    /// Automatic formatting on `mvn verify` (Spotless + palantir-java-format)
    Format,
    /// JaCoCo line coverage with an explicit minimum enforced during `mvn verify`
    Coverage,
    /// Route-derived k6 load tests, payload helpers, and repeatable commands
    Loadtest,
    /// Least-privilege GitHub Actions verification with immutable action pins
    Ci,
    /// Multi-stage, non-root OCI image using the project's configured Java release
    #[value(alias = "image")]
    Docker,
    /// Helm deployment with isolated management probes and SLO burn-rate alerts
    #[value(name = "k8s", alias = "kubernetes")]
    K8s,
    /// RFC 9457 problem responses and bean validation, handled in one place
    #[value(alias = "errors")]
    Api,
    /// Actuator health, info and metrics, exposed narrowly rather than with `*`
    Actuator,
    /// Caching that is switched on, bounded, and proven by a test
    Cache,
    /// An explicit Spring Security filter chain, shaped for an API
    Security,
    /// Credentialed browser access with explicit origins and all API methods
    Cors,
    /// Server-Sent Events: a concurrent emitter registry, a stream endpoint,
    /// and a heartbeat that cannot stall the rest of the scheduler
    #[value(alias = "events")]
    Sse,
    /// Sending mail, a Mailpit compose service, and an integration test that
    /// reads the message back over POP3 rather than trusting `send()`
    #[value(alias = "smtp")]
    Mail,
    /// Redis: a TTL-enforcing key/value wrapper, a compose service, and a
    /// real-container integration test
    Redis,
    /// Metrics: a Prometheus scrape endpoint, application-tagged meters, and
    /// meter names declared once rather than per call site
    #[value(alias = "metrics")]
    Observability,
    /// Network failure you can switch on: a Toxiproxy container in front of a
    /// dependency, so a test can cut the connection or add latency
    #[value(alias = "faults")]
    Toxiproxy,
}

impl Capability {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Capability::Db => "db",
            Capability::Kafka => "kafka",
            Capability::Csv => "csv",
            Capability::Sqlite => "sqlite",
            Capability::Json => "json",
            Capability::Testkit => "testkit",
            Capability::Fake => "fake",
            Capability::Http => "http",
            Capability::Format => "format",
            Capability::Coverage => "coverage",
            Capability::Loadtest => "loadtest",
            Capability::Ci => "ci",
            Capability::Docker => "docker",
            Capability::K8s => "k8s",
            Capability::Api => "api",
            Capability::Actuator => "actuator",
            Capability::Cache => "cache",
            Capability::Security => "security",
            Capability::Cors => "cors",
            Capability::Sse => "sse",
            Capability::Mail => "mail",
            Capability::Redis => "redis",
            Capability::Observability => "observability",
            Capability::Toxiproxy => "toxiproxy",
        }
    }
}
