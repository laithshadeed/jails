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

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, ValueEnum)]
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
    /// H2 in-process database, file-backed, with the browser console wired up
    H2,
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
    pub fn label(self) -> &'static str {
        match self {
            Capability::Db => "db",
            Capability::Kafka => "kafka",
            Capability::Csv => "csv",
            Capability::Sqlite => "sqlite",
            Capability::H2 => "h2",
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

/// Which SQL the generated DDL is written in.
///
/// Two, and the list is closed for the reason `README.md`'s scope bar is: a
/// dialect jails cannot check is a string it passes through. Each entry here
/// exists because a *specific* type name differs and the difference was
/// verified against that database's own source -- not because a database is
/// popular.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Dialect {
    /// The default, and what `README.md` documents.
    Postgres,
    /// `add h2`. In-process, and the only difference that reaches the DDL is
    /// one type name -- see [`Dialect::column_type`].
    H2,
}

impl Dialect {
    /// This dialect's spelling of a column type jails names in Postgres.
    ///
    /// **One entry, and that is the finding, not an oversight.** Every other
    /// type `sql.rs` emits -- `text`, `integer`, `bigint`, `boolean`,
    /// `double precision`, `numeric`, `uuid`, `date`, `timestamp` -- is in
    /// H2's own type table verbatim, checked in
    /// `deps/h2database/h2/src/main/org/h2/value/DataType.java`. `timestamptz`
    /// is not: H2 knows that name only inside its PostgreSQL *wire protocol*
    /// server, so a `create table` using it over JDBC fails to parse. The
    /// standard spelling is what H2 takes, and Postgres takes it too -- but
    /// `timestamptz` is what a Postgres schema is conventionally written in,
    /// and this module does not rewrite a schema people will read.
    pub fn column_type(self, postgres: &str) -> &str {
        match (self, postgres) {
            (Self::H2, "timestamptz") => "timestamp with time zone",
            _ => postgres,
        }
    }
}

/// The HTTP method a generated endpoint answers.
///
/// A `ValueEnum` for the reason every closed vocabulary here is one: it is the
/// only way `clap_complete` can emit `--method <TAB>`. Five verbs and no
/// escape hatch -- an arbitrary string would be a value jails passes through
/// and cannot check, and the two exotic methods a project actually needs are
/// cheaper to write by hand than a passthrough that produces a controller
/// annotated with something Spring has no mapping for.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, ValueEnum)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    /// The canonical spelling, which is also the wire form.
    pub fn label(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Post => "post",
            Self::Put => "put",
            Self::Patch => "patch",
            Self::Delete => "delete",
        }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "get" => Ok(Self::Get),
            "post" => Ok(Self::Post),
            "put" => Ok(Self::Put),
            "patch" => Ok(Self::Patch),
            "delete" => Ok(Self::Delete),
            other => Err(format!(
                "unknown HTTP method `{other}`.\n       fix: one of get, post, put, patch, delete"
            )),
        }
    }

    /// Spring's mapping annotation for this method.
    pub fn mapping(self) -> &'static str {
        match self {
            Self::Get => "GetMapping",
            Self::Post => "PostMapping",
            Self::Put => "PutMapping",
            Self::Patch => "PatchMapping",
            Self::Delete => "DeleteMapping",
        }
    }

    /// The method name a stub handler takes.
    pub fn handler_name(self) -> &'static str {
        self.label()
    }

    /// Whether a request of this method conventionally carries a body, and so
    /// whether `--on <Type>` becomes a `@RequestBody` parameter.
    ///
    /// GET and DELETE are excluded because a body on either is not forbidden
    /// by HTTP but is ignored by most of the stack between the caller and the
    /// handler -- a parameter that silently never binds is worse than none.
    pub fn takes_a_body(self) -> bool {
        matches!(self, Self::Post | Self::Put | Self::Patch)
    }
}

/// Every artifact `jails generate` can write.
///
/// A `clap::ValueEnum`, and that must stay true: it is the only way
/// `clap_complete` can emit a static completion list for `jails g <TAB>`
/// (CLAUDE.md). It lives here beside [`Capability`] rather than with the
/// generators for the same reason that one does -- these are the closed
/// vocabularies the CLI is built from, and the layers below the generators
/// validate against them.
///
/// What each kind *writes* is `jails-generate`'s, and stays there. R1 renames
/// this to `Recipe` at the internal boundary while the clap spelling stays at
/// the CLI edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum ArtifactKind {
    /// A REST resource that runs: record, port, JDBC + in-memory adapters,
    /// DTOs, service, controller, migration, fixture and tests
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
    // `webhook` was an alias here until the inbound kind existed. It is not
    // wrong -- this *sends* one -- but it is ambiguous now, and "webhook"
    // means the endpoint that receives Stripe's far more often than the client
    // that posts yours. `outbound` says which half without the ambiguity.
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
    /// A `<Name>Test` skeleton
    Test,
    /// A disabled `<Name>IT` skeleton for a real boundary test; also splices
    /// the Failsafe plugin, without which no `*IT` ever runs
    #[value(name = "integration-test", alias = "it")]
    IntegrationTest,
}
