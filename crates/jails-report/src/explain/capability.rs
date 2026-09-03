//! `jails explain <capability>`: what `jails add` installs, and the trap.
//!
//! The same table shape as the generator kinds next door, for the same
//! reason: a rationale is prose that has to be written, there is nowhere to
//! derive it from, and `every_capability_has_an_explanation` is what stops
//! the table falling behind [`CapabilityKind`].
//!
//! The rule for content is the kinds' rule. Say what the capability is *for*
//! and name the trap; a restatement of the `--help` line earns nothing,
//! because `--help` is right there. Where the trap is a version fact -- a
//! Boot major that moved a package, a Jackson line that must not be mixed --
//! it belongs here, because that is the thing a reader cannot see from the
//! generated file.

use jails_model::CapabilityKind;
use jails_support::Result;

struct Explanation {
    capability: CapabilityKind,
    /// One line: what you get.
    summary: &'static str,
    /// The reasoning, and the trap.
    body: &'static str,
}

const EXPLANATIONS: &[Explanation] = &[
    Explanation {
        capability: CapabilityKind::Db,
        summary: "PostgreSQL: Flyway migrations, a compose service, and a Testcontainers-backed test context.",
        body: "The container is a `@Bean` carrying `@ServiceConnection` in `TestcontainersConfig`, \
               not a `@Container` static field: Spring caches a test context past the lifetime a \
               static field manages, and the second suite then talks to a container that is \
               gone.\n\n\
               That configuration is `@Import`ed into the tests that need it rather than \
               registered globally. Registered globally, every slice test starts a PostgreSQL \
               it never queries. Once the JDBC starter is present auto-configuration demands a \
               DataSource for every `@SpringBootTest`, which is why `add db` splices the import \
               into the ones already on disk.",
    },
    Explanation {
        capability: CapabilityKind::Kafka,
        summary: "An Apache Kafka client and a KRaft broker in compose, with no topic assumed.",
        body: "The capability owns everything topic-agnostic and nothing else, because a topic \
               name is a fact about your application and a guessed one is worse than none. \
               `jails g event <Type>` is where a payload type and a topic arrive.\n\n\
               The dead-letter destination is named explicitly in the recoverer. \
               `DeadLetterPublishingRecoverer` defaults to `<topic>-dlt`, and a default that \
               only shows up when a record fails is a default nobody has tested.",
    },
    Explanation {
        capability: CapabilityKind::Csv,
        summary: "Reading CSV into records, on Apache Commons CSV.",
        body: "The builder's terminal call is `get()`, not `build()`, from Commons CSV 1.13 on. \
               A unit test beside the pack holds the pinned version and the generated call \
               together, so the two cannot drift apart silently.",
    },
    Explanation {
        capability: CapabilityKind::Sqlite,
        summary: "SQLite persistence: JDBC connections and a migration runner, no server.",
        body: "A file, so there is nothing to start and nothing in compose. The SQL projection \
               is the model's storage axis, so the DDL is generated for the dialect you \
               declared rather than translated at runtime.\n\n\
               It is not a small PostgreSQL. Ask for it when the project genuinely wants a \
               file, not as a way to avoid running a container in tests -- `add db` already \
               starts and stops its own.",
    },
    Explanation {
        capability: CapabilityKind::H2,
        summary: "An in-process, file-backed H2 with the browser console wired up.",
        body: "One SQL name is rewritten for H2 and exactly one: `timestamptz` becomes \
               `timestamp with time zone`. Every other type in the builtin table is in H2's own \
               type table verbatim, so a second translation layer would be a second place for \
               the dialect to be wrong.",
    },
    Explanation {
        capability: CapabilityKind::Json,
        summary: "Reading and writing JSON on Jackson 3, as one artifact.",
        body: "Jackson 3 is `tools.jackson`, and java.time support is built in. Adding \
               `jackson-datatype-jsr310` drags the 2.x line in beside it, nothing warns, and \
               half the code lands on a mapper nobody configured -- which is why `jails doctor` \
               reports two Jackson majors as a failure.\n\n\
               The annotations are the one exception and stayed on the 2.x coordinates. That is \
               upstream's own arrangement, not an oversight here.",
    },
    Explanation {
        capability: CapabilityKind::Testkit,
        summary: "Deterministic test helpers: clocks, ids, fixtures and in-process CLI runs.",
        body: "The point is a test that fails for one reason. A fixed clock and a seeded id \
               source remove the two commonest sources of a test that passes locally and fails \
               in CI at midnight.\n\n\
               It is test-scoped by construction. Reaching for a testkit clock from main source \
               is how a fixed clock ships to production.",
    },
    Explanation {
        capability: CapabilityKind::Fake,
        summary: "A scripted test double for any interface, driven by a lambda.",
        body: "A hand-written stub grows a field per call and then a mode flag; a mock framework \
               answers a different question, about interactions rather than behaviour. This is \
               the middle: one implementation whose behaviour is the lambda you pass at the \
               call site, so the expectation is next to the assertion that depends on it.",
    },
    Explanation {
        capability: CapabilityKind::Http,
        summary: "An HTTP server on the JDK's own httpserver, with no framework.",
        body: "For a project that is not Spring and should not become Spring to answer one \
               route. Nothing here is auto-configured, which is the trade: no starters, and no \
               conventions either.",
    },
    Explanation {
        capability: CapabilityKind::Format,
        summary: "Formatting on `mvn verify`, through Spotless and palantir-java-format.",
        body: "Generated imports are already written in the order palantir produces -- static \
               first, a blank line, then the rest sorted -- so adding this leaves a project that \
               passes `jails check` rather than one with a diff on every generated file.\n\n\
               Line *wrapping* cannot be predicted from a template, so `add format` runs \
               `spotless:apply` once, best effort. It refuses on Gradle by name: Spotless needs \
               its plugin inside `plugins { }`, which is legal only as the first statement of \
               the script, and the Gradle adapter's contract is that it appends one marked \
               block and touches nothing else.",
    },
    Explanation {
        capability: CapabilityKind::Coverage,
        summary: "JaCoCo line coverage with a minimum enforced during `mvn verify`.",
        body: "A coverage report nobody fails on is a number in a directory. The minimum is \
               explicit and in the build file, so raising it is a reviewed change rather than a \
               conversation.\n\n\
               A build plugin is claimed by what it does, not by its coordinate: this is \
               `BuildFeature::Coverage`, because `jacoco-maven-plugin` is not a name Gradle \
               resolves.",
    },
    Explanation {
        capability: CapabilityKind::Loadtest,
        summary: "k6 load tests derived from the project's own routes, with payload helpers.",
        body: "The script is generated from the routes that exist, so it exercises the \
               application rather than a URL somebody remembered.\n\n\
               `jails bench` runs it and deliberately does not parse k6's output. k6's own \
               thresholds decide pass or fail, because a second opinion about a threshold is a \
               second place for it to be wrong.",
    },
    Explanation {
        capability: CapabilityKind::Ci,
        summary: "A GitHub Actions workflow with least privilege and immutable action pins.",
        body: "Actions are pinned by commit, not by tag: a tag is a moving pointer somebody else \
               controls, and the workflow has your repository's token.\n\n\
               The workflow file is substituted with plain text replacement rather than the \
               template renderer, because GitHub's own syntax is `${{ ... }}` and a renderer \
               that treats braces as placeholders reads the file's own syntax as keys.",
    },
    Explanation {
        capability: CapabilityKind::Docker,
        summary: "A multi-stage, non-root OCI image on the Java release the project declares.",
        body: "Non-root by default, and the release comes from the project rather than from the \
               image's tag -- an image built on a newer JDK than the code targets is a class \
               file the runtime rejects.\n\n\
               The Dockerfile is substituted with plain text replacement, not the template \
               renderer: `docker image inspect` format strings are `{{.Config.User}}`, which a \
               renderer would read as a placeholder.",
    },
    Explanation {
        capability: CapabilityKind::K8s,
        summary: "A Helm chart with management probes on their own port and burn-rate alerts.",
        body: "Health probes are exposed on a management port separate from traffic, so a \
               readiness check cannot be answered by a thread pool that is busy serving \
               requests.\n\n\
               The alerting rules are burn-rate rules over an error budget rather than \
               thresholds on raw counters. The chart's PromQL is written with plain text \
               replacement: rendering it through `format!` would turn `{{` into `{` and \
               silently change the query.",
    },
    Explanation {
        capability: CapabilityKind::Api,
        summary: "RFC 9457 problem responses and bean validation, handled in one place.",
        body: "One handler, so a validation failure and a constraint violation answer in the \
               same shape rather than in whatever shape the controller that caught them \
               chose.\n\n\
               Its plan is a pure function of the project, which is why order matters: the \
               `DuplicateKeyException` arm is rendered only when the JDBC starter is already \
               present. `jails add api db` in that order leaves the arm out, and `jails sync` \
               is the repair.",
    },
    Explanation {
        capability: CapabilityKind::Actuator,
        summary: "Actuator health, info and metrics, exposed narrowly rather than with `*`.",
        body: "`*` exposes every endpoint the classpath happens to contribute, including ones \
               added by a dependency upgrade nobody read. The list here is what you asked \
               for.\n\n\
               Two capabilities own the exposure property, so the compiler unions the value \
               rather than letting whichever was added last win.",
    },
    Explanation {
        capability: CapabilityKind::Cache,
        summary: "Caching that is switched on, bounded, and proven by a test.",
        body: "An unbounded cache is a memory leak with a good reputation. The generated \
               configuration carries a size and an expiry, and a test proves the second call \
               does not reach the underlying method -- which is the thing that quietly stops \
               being true when a call moves inside the same bean and self-invocation skips the \
               proxy.",
    },
    Explanation {
        capability: CapabilityKind::Security,
        summary: "An explicit Spring Security filter chain, shaped for an API.",
        body: "The chain is written out rather than left to defaults, because the defaults are \
               shaped for a browser session and an API is not one.\n\n\
               This is also what makes `@scope` fields work: the compiler refuses a scoped \
               operation unless a `ScopeAuthorizer` has been declared, so a request-boundary \
               field proved against a JWT claim cannot be generated with nothing to prove it \
               against.",
    },
    Explanation {
        capability: CapabilityKind::Cors,
        summary: "Credentialed browser access with explicit origins and all API methods.",
        body: "Origins are listed, never `*`. A wildcard origin and credentials are mutually \
               exclusive in the specification, so the combination fails at the browser rather \
               than at the server, which is the hardest place to read an error.",
    },
    Explanation {
        capability: CapabilityKind::Sse,
        summary: "Server-Sent Events: an emitter registry, a stream endpoint, and a heartbeat.",
        body: "The registry is concurrent because a client disconnecting and a broadcast \
               arriving are genuinely simultaneous events.\n\n\
               The heartbeat runs on its own scheduler. Sharing the application's means one \
               slow send holds up every other scheduled task in the project, which presents as \
               an unrelated job that stopped running.",
    },
    Explanation {
        capability: CapabilityKind::Mail,
        summary: "Sending mail, a Mailpit compose service, and a test that reads the message back.",
        body: "The integration test retrieves the delivered message over POP3 rather than \
               asserting that `send()` returned. A mail send that silently goes nowhere returns \
               exactly the same way as one that arrives, so the assertion has to be on the \
               other side of the wire.",
    },
    Explanation {
        capability: CapabilityKind::Redis,
        summary: "Redis: a TTL-enforcing key/value wrapper, a compose service, and a container test.",
        body: "Every write goes through a wrapper that requires a time to live. A key set \
               without one is the entry that is still there a year later, and Redis will not \
               remind you.\n\n\
               The test runs against a real container rather than an embedded stand-in, because \
               the behaviours worth testing -- eviction, expiry -- are the ones a stand-in \
               approximates.",
    },
    Explanation {
        capability: CapabilityKind::Observability,
        summary: "A Prometheus scrape endpoint, application-tagged meters, and meter names declared once.",
        body: "The application tag is applied by a generated `MeterRegistryCustomizer` rather \
               than by `management.metrics.tags.*` properties. It is code the project owns, and \
               it does not depend on which actuator modules happen to be present.\n\n\
               Meter names are declared in one place. A metric name spelled twice is two \
               metrics, and the dashboard that reads the other spelling shows a flat line \
               rather than an error.",
    },
    Explanation {
        capability: CapabilityKind::Toxiproxy,
        summary: "Network failure you can switch on: a proxy in front of a dependency, in tests.",
        body: "Retry and timeout code is written from a guess about what a failing network does \
               and is then never exercised. This puts a proxy in the path so a test can cut the \
               connection or add latency deliberately, which is the only way to find out \
               whether the timeout you configured is the one that fires.",
    },
    Explanation {
        capability: CapabilityKind::FastTest,
        summary: "JUnit's console launcher, so `jails test --fast` runs compiled classes without Maven.",
        body: "Not a capability you add: `jails test --fast` declares it when it needs the \
               dependency, and `jails remove fast-test` retires it.\n\n\
               The console artifact's version must equal the project's JUnit version, which the \
               JUnit BOM constrains to one number. `--fast` is the path that avoids the Maven \
               daemon and the substrate for `jails testd`; it is not a faster way to run the \
               same thing.",
    },
];

/// Print the entry for one capability.
pub(super) fn explain(capability: CapabilityKind) -> Result<()> {
    let entry = EXPLANATIONS
        .iter()
        .find(|entry| entry.capability == capability)
        .ok_or_else(|| {
            format!(
                "no explanation recorded for `{}`.\n       fix: add one to \
                 explain/capability.rs -- `every_capability_has_an_explanation` should have \
                 caught this.",
                capability.label()
            )
        })?;

    println!("{}  {}", capability.label(), entry.summary);
    println!();
    super::print_body(entry.body);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ValueEnum;

    /// One edit per capability, enforced -- the kinds' rule, for the other
    /// half of the vocabulary.
    #[test]
    fn every_capability_has_an_explanation() {
        let missing: Vec<&str> = CapabilityKind::value_variants()
            .iter()
            .filter(|capability| {
                !EXPLANATIONS
                    .iter()
                    .any(|entry| entry.capability == **capability)
            })
            .map(|capability| capability.label())
            .collect();
        assert!(
            missing.is_empty(),
            "{} capability(ies) have no entry in explain/capability.rs: {}\n\
             Add one saying what it installs and naming the trap.",
            missing.len(),
            missing.join(", ")
        );
    }

    /// A summary that restates the `--help` line earns nothing, and a body
    /// with no trap in it is a summary written twice.
    #[test]
    fn every_entry_says_something_and_says_it_once() {
        for entry in EXPLANATIONS {
            assert!(
                entry.summary.len() < 110,
                "`{}`'s summary is a paragraph: {}",
                entry.capability.label(),
                entry.summary
            );
            assert!(
                entry.body.len() > entry.summary.len(),
                "`{}`'s body says less than its summary",
                entry.capability.label()
            );
        }
    }
}
