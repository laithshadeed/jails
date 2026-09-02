//! The closed capability vocabulary, and the linking rules for a declaration
//! of one.
//!
//! [`CapabilityKind`] is the closed set itself; [`link`] resolves a
//! declaration that names one. The set lives here rather than beside the
//! capability planners because the layers *below* those planners validate
//! against it -- `jails.toml`'s `[project] capabilities`, `app.toml`'s
//! manifest, the JDL `cap` declaration -- and deriving the valid names from
//! one enum instead of restating them is the point.
//!
//! Only the *names* are here. What a capability installs is the compiler's,
//! and stays there.

use crate::id::CapabilityId;
use crate::linker::Linker;
use crate::model::Capability;
use crate::source;
use std::collections::BTreeMap;

/// Every optional capability a project can carry.
///
/// A `clap::ValueEnum` under the `cli` feature, and that must stay true: it is
/// the only way `clap_complete` can emit a static completion list for
/// `jails add <TAB>`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum CapabilityKind {
    /// PostgreSQL + Flyway + Testcontainers + a compose service; raw SQL only, never an ORM
    #[cfg_attr(feature = "cli", value(alias = "postgres"))]
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
    #[cfg_attr(feature = "cli", value(alias = "image"))]
    Docker,
    /// Helm deployment with isolated management probes and SLO burn-rate alerts
    #[cfg_attr(feature = "cli", value(name = "k8s", alias = "kubernetes"))]
    K8s,
    /// RFC 9457 problem responses and bean validation, handled in one place
    #[cfg_attr(feature = "cli", value(alias = "errors"))]
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
    #[cfg_attr(feature = "cli", value(alias = "events"))]
    Sse,
    /// Sending mail, a Mailpit compose service, and an integration test that
    /// reads the message back over POP3 rather than trusting `send()`
    #[cfg_attr(feature = "cli", value(alias = "smtp"))]
    Mail,
    /// Redis: a TTL-enforcing key/value wrapper, a compose service, and a
    /// real-container integration test
    Redis,
    /// Metrics: a Prometheus scrape endpoint, application-tagged meters, and
    /// meter names declared once rather than per call site
    #[cfg_attr(feature = "cli", value(alias = "metrics"))]
    Observability,
    /// Network failure you can switch on: a Toxiproxy container in front of a
    /// dependency, so a test can cut the connection or add latency
    #[cfg_attr(feature = "cli", value(alias = "faults"))]
    Toxiproxy,
    /// JUnit's console launcher, so `jails test --fast` can run the compiled
    /// classes without Maven.
    ///
    /// **Not a `jails add` value**, which is what `value(skip)` says: it is
    /// declared by `jails test --fast` when it needs the dependency and
    /// retired by `jails remove fast-test`, so offering it as a completion
    /// would advertise a command that does not exist.
    #[cfg_attr(feature = "cli", value(skip))]
    FastTest,
}

impl CapabilityKind {
    /// Declaration order, which is the order `jails add --help` lists them in.
    pub const ALL: [CapabilityKind; 26] = [
        Self::Db,
        Self::Kafka,
        Self::Csv,
        Self::Sqlite,
        Self::H2,
        Self::Json,
        Self::Testkit,
        Self::Fake,
        Self::Http,
        Self::Format,
        Self::Coverage,
        Self::Loadtest,
        Self::Ci,
        Self::Docker,
        Self::K8s,
        Self::Api,
        Self::Actuator,
        Self::Cache,
        Self::Security,
        Self::Cors,
        Self::Sse,
        Self::Mail,
        Self::Redis,
        Self::Observability,
        Self::Toxiproxy,
        Self::FastTest,
    ];

    /// The canonical name: what `jails.toml` stores, what a JDL `cap`
    /// declaration spells and what a refusal prints -- never a clap alias.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Db => "db",
            Self::Kafka => "kafka",
            Self::Csv => "csv",
            Self::Sqlite => "sqlite",
            Self::H2 => "h2",
            Self::Json => "json",
            Self::Testkit => "testkit",
            Self::Fake => "fake",
            Self::Http => "http",
            Self::Format => "format",
            Self::Coverage => "coverage",
            Self::Loadtest => "loadtest",
            Self::Ci => "ci",
            Self::Docker => "docker",
            Self::K8s => "k8s",
            Self::Api => "api",
            Self::Actuator => "actuator",
            Self::Cache => "cache",
            Self::Security => "security",
            Self::Cors => "cors",
            Self::Sse => "sse",
            Self::Mail => "mail",
            Self::Redis => "redis",
            Self::Observability => "observability",
            Self::Toxiproxy => "toxiproxy",
            Self::FastTest => "fast-test",
        }
    }

    /// Whether `jails add` and `jails.toml` name this kind.
    ///
    /// [`Self::FastTest`] is the one that is not: `jails test --fast`
    /// declares it and `jails remove fast-test` retires it, so it is a
    /// capability the tool owns rather than one a reader asks for.
    pub const fn addable(self) -> bool {
        !matches!(self, Self::FastTest)
    }

    /// Whether a JDL `cap` declaration may name this kind.
    ///
    /// [`Self::Db`] and [`Self::H2`] are not: JDL v1 §12 makes them
    /// selections of `app.storage` rather than cap declarations, so a source
    /// file spelling `cap db` is refused with the twenty-four the registry
    /// does close.
    pub const fn declarable_in_source(self) -> bool {
        !matches!(self, Self::Db | Self::H2)
    }

    /// The kind a canonical label names, `None` for anything outside the set.
    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.label() == label)
    }

    /// The kind a canonical label names, when a JDL `cap` declaration may
    /// name it at all.
    pub fn declared_in_source(label: &str) -> Option<Self> {
        Self::from_label(label).filter(|kind| kind.declarable_in_source())
    }
}

pub(crate) fn link(
    declarations: BTreeMap<String, source::Capability>,
    base_package: &str,
    linker: &mut Linker,
) -> BTreeMap<CapabilityId, Capability> {
    let mut capabilities = BTreeMap::new();
    let mut kinds = BTreeMap::<String, String>::new();
    for (label, capability) in declarations {
        let path = format!("$.capabilities.{label}");
        linker.label(&label, &path);
        linker.label(&capability.kind, &format!("{path}.kind"));
        linker.register_id(&capability.id, &format!("{path}.id"));
        let Some(id) = linker.capability_id(&capability.id, &format!("{path}.id")) else {
            continue;
        };
        if let Some(first) = kinds.insert(capability.kind.clone(), path.clone()) {
            linker.problem(
                "model-capability-collision",
                format!("{path}.kind"),
                format!(
                    "capability kind `{}` is already declared at {first}",
                    capability.kind
                ),
                "keep one declaration for each capability kind",
            );
        }
        if let Some(name) = &capability.name {
            linker.java_type(name, &format!("{path}.name"));
        }
        let java_package = capability.package.map(|package| {
            let resolved = if package.is_empty() {
                base_package.to_string()
            } else {
                format!("{base_package}.{package}")
            };
            linker.java_package(&resolved, &format!("{path}.package"));
            resolved
        });
        capabilities.insert(
            id.clone(),
            Capability {
                id,
                label,
                kind: capability.kind,
                name: capability.name,
                java_package,
            },
        );
    }
    capabilities
}
