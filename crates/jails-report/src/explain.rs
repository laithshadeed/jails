//! `jails explain <kind>`: why a generated artifact is shaped the way it is.
//!
//! The generated Java already carries this reasoning in its Javadoc, which is
//! the right place for a person reading the file. It is the wrong place for
//! someone deciding *whether to generate it*, and for the second reader this
//! tool has: an agent, which sees a class with `@Repository` on one adapter and
//! not the other, reads that as an oversight, and "fixes" it into an ambiguous
//! bean that compiles and cannot start.
//!
//! ## Why a table rather than another derivation
//!
//! Everything else in `commands.rs` is derived, and this deliberately is not:
//! a rationale is prose that has to be *written*, and there is nowhere to
//! derive it from. That makes it a sixth place where "what does kind X mean"
//! is recorded, which is exactly what `plan.md` §6.1 counts — so it is held to
//! `why.rs`'s shape, the one `abstract.md` §2 singles out as the only clean
//! concept in the codebase: **a value in a table, and adding one instance is
//! one edit**. `every_kind_has_an_explanation` fails when a kind is added
//! without one, so the table cannot fall behind the enum the way the editor
//! lists did.
//!
//! The rule for content: say what the artifact *is for* and name the trap.
//! A restatement of the `--help` line earns nothing; `--help` is right there.

use crate::generate::ArtifactKind;
use clap::ValueEnum;
use jails_support::Result;

/// What one kind is for, and the mistake it invites.
struct Explanation {
    kind: ArtifactKind,
    /// One line: what you get.
    summary: &'static str,
    /// The reasoning that is otherwise only in the generated Javadoc.
    body: &'static str,
}

const EXPLANATIONS: &[Explanation] = &[
    Explanation {
        kind: ArtifactKind::Scaffold,
        summary: "A running resource: record, port, two adapters, service, controller, DTOs, tests, and a migration.",
        body: "Exactly one repository adapter carries `@Repository`, and which one depends on \
               the project. With `spring-boot-starter-jdbc` present the JdbcClient adapter is \
               the bean and the in-memory one is an unannotated fake; without it the JDBC \
               adapter takes a Connection the caller owns, so it cannot be a bean, and the \
               in-memory one is.\n\n\
               Do not add `@Repository` to the other one. Two beans then qualify for one \
               injection point: it compiles, and the context fails to start. That ambiguity is \
               what `jails beans` exists to report.",
    },
    Explanation {
        kind: ArtifactKind::Record,
        summary: "An immutable value with validation in its compact constructor, plus a test.",
        body: "A `?` component is emitted as `Optional<T>`, and the compact constructor \
               normalises a null one with `requireNonNullElse(x, Optional.empty())` -- a null \
               Optional being the one thing worse than a null value.\n\n\
               This is a deliberate departure from the usual `Optional` advice. A record \
               component is a field and a return type at once, and the alternative is a \
               nullable component plus a differently *named* Optional-returning method, since \
               an accessor cannot be overridden to change its return type.",
    },
    Explanation {
        kind: ArtifactKind::Field,
        summary: "Add one component to an existing record, and update everything derived from it.",
        body: "It refuses rather than clobbers. The ownership oracle re-renders each derived \
               file and compares bytes; files that still match what jails would have written are \
               rewritten, and for the rest it prints the snippet to paste.\n\n\
               That over-reports after a jails upgrade, which is the correct direction to be \
               wrong in: over-reporting prints something you paste, over-writing destroys work. \
               `--remove` is deliberately absent -- dropping a column is a data decision.",
    },
    Explanation {
        kind: ArtifactKind::Repo,
        summary: "A port interface plus the adapters that implement it.",
        body: "Fields come from the spec if you pass one, otherwise from the record on disk, \
               otherwise it refuses and names the record. It never invents columns.\n\n\
               See `explain scaffold` for which adapter carries `@Repository`; the same rule \
               applies here and the same ambiguity results from breaking it.",
    },
    Explanation {
        kind: ArtifactKind::Strategy,
        summary: "An open port plus one bean per implementation, collected by Spring into a List.",
        body: "The failure mode is silent: an implementation missing `@Component` is simply \
               absent from the injected list, so it never runs and nothing reports a problem.\n\n\
               `destroy strategy` therefore reads the implementations back off disk rather than \
               from a stored list, so an implementation you added by hand is still one of this \
               strategy's classes and is not left behind implementing a deleted interface.",
    },
    Explanation {
        kind: ArtifactKind::Sealed,
        summary: "A closed hierarchy: a sealed interface and its permitted implementations.",
        body: "Sealed rather than open so a `switch` over it needs no `default` branch. That is \
               the whole value: adding a variant breaks the build at every decision site, which \
               is where the decision about the new case belongs. An open hierarchy turns the \
               same addition into a silent fall-through.",
    },
    Explanation {
        kind: ArtifactKind::Enum,
        summary: "A closed set of named constants, stored by name.",
        body: "Worth generating rather than hand-writing because jails can then *sample* it: \
               `is_enum` reads the file, so a generated test can produce a real value for a \
               component of this type instead of being emitted `@Disabled`.",
    },
    Explanation {
        kind: ArtifactKind::Value,
        summary: "A validated wrapper around a single primitive.",
        body: "The point is that the primitive stops being interchangeable with every other \
               one of its type. Two `String` parameters can be swapped at a call site and \
               compile; two distinct value types cannot.",
    },
    Explanation {
        kind: ArtifactKind::Usecase,
        summary: "One create operation: typed command, port, transactional implementation, route, tests.",
        body: "It only infers *conservative* defaults for target components the command does \
               not carry -- identity, timestamp, status default, empty optional or collection, \
               zero counter, false flag. Anything else stops generation and asks for the field, \
               because a guessed business value that compiles is worse than a refusal.\n\n\
               `--yields <Event>` adds the transactional outbox half, which needs `add json` \
               for durable payloads and a generated event type.",
    },
    Explanation {
        kind: ArtifactKind::Transition,
        summary: "An optimistic state change guarded by a version column and, optionally, scope.",
        body: "Required scalar fields only, so match and update semantics stay exact: an \
               optional filter would make \"absent\" and \"null\" the same query, and they are \
               not. It needs `version:long` or `version:int` -- the compare-and-set column is \
               what makes the update safe under concurrency rather than last-write-wins.\n\n\
               Example: `jails g transition RenameLoan id:uuid title:string! version:long --on Loan`.",
    },
    Explanation {
        kind: ArtifactKind::Query,
        summary: "A typed read: query record, port, JDBC adapter, controller, tests.",
        body: "Required scalar equality filters only, for the same reason `transition` refuses \
               optionals: null and list semantics would have to be guessed. Use the scaffold's \
               own list endpoint for an unfiltered read.\n\n\
               Example: `jails g query LoansByMember memberId:uuid --on Loan`.",
    },
    Explanation {
        kind: ArtifactKind::DurableJob,
        summary: "Leased, retried, idempotent background work backed by a PostgreSQL queue.",
        body: "Durable because the queue is a table: a process that dies mid-item leaves the \
               lease to expire rather than losing the work. It needs `add db` -- an in-memory \
               queue would be a different thing wearing the same name. The target use case and \
               job must share the same required `id:uuid` and exact command fields.\n\n\
               Example: `jails g durable-job Nudge id:uuid subject:string --on CloseTicket --yields Ticket`.",
    },
    Explanation {
        kind: ArtifactKind::Association,
        summary: "An explicit relational invariant between two generated records.",
        body: "Explicit beat inferred, and the alternative was tried: inferring a relation from \
               an `author:User` component gave a silent `text` column and lost data. Both \
               records are read, types are checked across the boundary, composite keys are \
               free, and identifier length is checked. No `ON DELETE` behaviour is invented, \
               because that is a data decision. NAME is the association's own name.\n\n\
               Example: `jails g association LoanMember memberId=id --on Loan --yields Member`.",
    },
    Explanation {
        kind: ArtifactKind::Event,
        summary: "A typed Kafka event with its publisher, listener and topic beans.",
        body: "`add kafka` owns everything topic-agnostic -- the error handler, the DLT \
               routing, the deserializer. This owns what needs a payload type: the `NewTopic` \
               bean and the default-type property.\n\n\
               The dead-letter destination is named explicitly, because \
               `DeadLetterPublishingRecoverer` defaults to `<topic>-dlt`, so a project that \
               declares `<topic>.DLT` finds it empty with only a WARN to say so.",
    },
    Explanation {
        kind: ArtifactKind::Socket,
        summary: "A bidirectional WebSocket endpoint: handler, registration and test.",
        body: "`add sse` is the server-to-client half. This is the other one, and the three \
               things it decides are all defaults that are wrong in a way nothing reports.\n\n\
               A `WebSocketSession` is not safe for concurrent sends: two threads on one \
               session produce `IllegalStateException: The remote endpoint was in state \
               [TEXT_PARTIAL_WRITING]`, which is load-dependent and never happens at the desk. \
               Every session is wrapped in `ConcurrentWebSocketSessionDecorator`.\n\n\
               A dead session must leave the registry: `sendMessage` on a closed one throws \
               `IOException`, so letting it out stops the broadcast and swallowing it keeps \
               the corpse forever.\n\n\
               The handshake is same-origin by default, and the registration deliberately \
               does not widen it. A browser client from another origin is refused with a 403 \
               and nothing in the application log, so the config says where to look rather \
               than making a security decision for you.",
    },
    Explanation {
        kind: ArtifactKind::Presence,
        summary: "Who is connected, shared across nodes rather than held in one process.",
        body: "An in-memory presence map is silently correct on one node and silently wrong on \
               two. It does not throw and it does not warn -- it answers a question about the \
               cluster using one process's memory. The Django original this came from says so \
               in a comment: \"works because InMemoryChannelLayer = single Daphne \
               process\".\n\n\
               So the state is a PostgreSQL table keyed by (scope, member, node), and the \
               generated IT is what keeps it there: two adapters with different node ids, one \
               joins, the other is asked. A map fails that test; a table passes it.\n\n\
               A row per node, not per member, because a member connected twice is present \
               until both claims are gone. And a `seen_at` window rather than a leave-only \
               protocol, because a process that dies never sends `leave` -- presence built on \
               explicit departure is permanently wrong after the first crash.",
    },
    Explanation {
        kind: ArtifactKind::Dto,
        summary: "Request and response records at the HTTP boundary, with validation.",
        body: "Separate from the domain record on purpose: the wire shape belongs to whoever \
               calls you and changes on their schedule. Letting a domain type reach the wire \
               directly turns an external rename into a refactor here.",
    },
    Explanation {
        kind: ArtifactKind::Client,
        summary: "A declarative HTTP client interface Spring implements for you.",
        body: "An interface and nothing else -- no base URL in the code, because the client \
               belongs to a group whose URL is configuration. Pointing it at a stub, staging or \
               production is then a property rather than a code change.\n\n\
               It splices `spring-boot-starter-restclient`, which is the non-obvious part: \
               `@ImportHttpServices` builds the proxies without it, so the project compiles and \
               starts, and the first call fails with `URI with undefined scheme`.",
    },
    Explanation {
        kind: ArtifactKind::Fetcher,
        summary: "A bounded, SSRF-safe outbound fetch.",
        body: "Bounds and policy are configuration, not arguments: it takes only a name. The \
               limits exist because an unbounded fetch of an attacker-supplied URL is the whole \
               SSRF class, and a limit you have to remember to pass is one you will forget.",
    },
    Explanation {
        kind: ArtifactKind::HttpWorkflow,
        summary: "A bounded traversal over fetched documents, with a durable frontier.",
        body: "Parses with the JDK's own `HTMLEditorKit`, and treats the RFC 9309 policy file \
               as an ordinary frontier entry rather than a special case. **Zero new \
               dependencies** -- the obvious HTML and traversal libraries were considered and \
               are not needed, which is why there is no capability for this.\n\n\
               The frontier is durable, so a process that dies mid-traversal resumes rather \
               than starting over.",
    },
    Explanation {
        kind: ArtifactKind::HttpSink,
        summary: "Outbound delivery of an outbox event to an HTTP endpoint.",
        body: "The inverse direction from a webhook you receive. It is idempotent on the \
               event's own id, which is why the outbox requires that id to be a required UUID.",
    },
    Explanation {
        kind: ArtifactKind::Idempotency,
        summary: "At-most-once execution with a retained result: receipt store, guard, table.",
        body: "A `@unique` column on the key already gives you one row per key. What it does \
               not give you is the *retained result*, and that is the whole gap: a retry finds \
               the row, fails the insert, and gets 409 Conflict -- which tells a caller that \
               never saw the first response that the work happened, while still withholding \
               what happened.\n\n\
               Four outcomes, and each is a case something gets wrong: first call runs it; a \
               retry with the same request replays the stored response; the same key with a \
               *different* request is refused, because replaying would silently discard the \
               second request; and a retry while the first attempt is still running is told to \
               retry, because there is no answer yet and blocking would tie up a request \
               thread.\n\n\
               The claim is one `insert ... on conflict do nothing returning`. Select-then-\
               insert leaves a window where two callers both see nothing and both proceed -- \
               the race the whole mechanism exists to close, reintroduced by the obvious \
               implementation.",
    },
    Explanation {
        kind: ArtifactKind::Auth,
        summary: "A JWT issuer for this service's own tokens, and the default it undoes.",
        body: "Two facts, both read out of the Spring source rather than remembered, and both \
               the kind that surprise people.\n\n\
               **Spring Boot auto-configures no `JwtEncoder`.** Not one occurrence of the type \
               exists in the whole of Boot: the resource-server starter hands you a decoder for \
               *someone else's* tokens and stops there. A service issuing its own has to \
               declare the encoder, which is what the generated config is.\n\n\
               **A token with no `exp` passes the default decoder.** \
               `JwtTimestampValidator` ships with `allowEmptyExpiryClaim = true`, so every \
               out-of-the-box configuration accepts a token that never expires, and nothing \
               warns. One line closes it, and the generated test is what keeps that line \
               there: deleting it changes no behaviour any other test can observe.\n\n\
               The key is symmetric and read from configuration, which fits a service that \
               both issues and verifies. Two services that must verify each other's tokens \
               want a key pair and a published JWK set -- never one shared secret, since every \
               holder of it can mint tokens for every other.\n\n\
               Needs `jails add security`: without a filter chain reading the token, the \
               encoder and decoder are beans nothing consumes.",
    },
    Explanation {
        kind: ArtifactKind::Webhook,
        summary: "An inbound webhook endpoint you can believe: raw bytes, constant time, bounded window.",
        body: "The inbound counterpart to `http-sink`, and every one of its failure modes is a \
               rejection or an acceptance that should have gone the other way, showing up as an \
               error nowhere.\n\n\
               **Signed over the raw bytes.** Two JSON documents can mean the same thing and \
               hash differently -- key order, whitespace, `1.0` against `1`. A verifier that \
               binds the body to a record and re-serialises to check rejects good deliveries, \
               intermittently, depending on the sender's formatting. The controller takes \
               `@RequestBody byte[]`, which reads like a shortcut and is the whole design.\n\n\
               **Compared with `MessageDigest.isEqual`.** `Arrays.equals` returns at the first \
               differing byte, so how long a rejection takes says how much of the signature was \
               right -- and a signature can be recovered a byte at a time from that.\n\n\
               **The timestamp is checked in both directions, and it is inside the \
               signature.** Five minutes, which is Stripe's tolerance. Rejecting only stale \
               timestamps leaves a far-future one accepted, the same replay window with its \
               sign flipped; leaving the timestamp out of the signed bytes makes it a header \
               anyone in the middle can rewrite, at which point there is no window at all.\n\n\
               The endpoint answers 200 before doing the work. Senders retry on anything else \
               and time out in seconds, so a handler that processes inline is retried while it \
               is still running and the same event arrives twice. Hand it to `g durable-job`, \
               or make it idempotent with `g idempotency`.",
    },
    Explanation {
        kind: ArtifactKind::Search,
        summary: "PostgreSQL full-text search: a generated tsvector column, a GIN index, a port.",
        body: "The `tsvector` is a **generated column**, not a trigger, and that is the whole \
               kind.\n\n\
               The trigger recipe is older and still widely copied, and it has one silent \
               failure: somebody adds an UPDATE path that does not fire it -- a bulk fixup, a \
               migration, a second service writing the same table -- the row's text changes, \
               the vector does not, and the row stops matching a search it used to match. \
               Nothing errors. `generated always as (...) stored` cannot drift from its \
               inputs, because PostgreSQL maintains it.\n\n\
               Every column is wrapped in `coalesce(x, '')`: `||` with a NULL operand yields \
               NULL, so one null column would blank the whole vector and the row would match \
               nothing at all. The text search configuration is named in the expression rather \
               than left to `default_text_search_config`, so the stemming a row was indexed \
               under does not change when a session setting does.\n\n\
               The adapter uses `websearch_to_tsquery`, the syntax in which unformatted text \
               is a valid query. `to_tsquery` throws a syntax error on a bare two-word \
               phrase -- which is what a search box produces, so a search endpoint built on \
               it 500s on an apostrophe.\n\n\
               The components to index are named rather than inferred: a vector over every \
               text column indexes ids and status codes as prose, and a search for \"active\" \
               then returns everything.",
    },
    Explanation {
        kind: ArtifactKind::Migration,
        summary: "An empty, correctly numbered Flyway migration.",
        body: "Numbered numerically rather than lexically: `V10` sorts before `V9` as a string, \
               which would apply migrations in an order nobody has tested.\n\n\
               Forward-only. `destroy` refuses, and rewriting an applied migration is how a \
               schema and its history stop agreeing.",
    },
    Explanation {
        kind: ArtifactKind::Command,
        summary: "A CLI command, registered in the project's dispatcher.",
        body: "Dispatchers are found by *shape*, not filename -- the registry type plus the \
               `return commands;` anchor -- so both `App.java` and a generated `<Name>Cli.java` \
               qualify. With more than one, pass `--on <Dispatcher>` to say which; with none, \
               the Javadoc tells you how to wire it by hand.",
    },
    Explanation {
        kind: ArtifactKind::Cli,
        summary: "A dispatcher: the registry a `command` registers itself into.",
        body: "A project can legitimately have two -- `new-cli` writes one into `App.java` -- \
               and jails will not guess between them. That is why `command` takes `--on`.",
    },
    Explanation {
        kind: ArtifactKind::Factory,
        summary: "A test data builder with sensible defaults for every component.",
        body: "Defaults come from the same `sample_value` the generated tests use. A component \
               jails cannot sample starts `null` and `build()` throws naming it -- never a \
               guessed default, because a silently wrong fixture makes a passing test lie.",
    },
    Explanation {
        kind: ArtifactKind::Cases,
        summary: "A test class from a markdown brief's acceptance bullets.",
        body: "The name is a *path* to the brief, not a Java class. Each bullet becomes a \
               disabled test with the bullet as its display name, so the brief and the test \
               list stay legible against each other.",
    },
    Explanation {
        kind: ArtifactKind::Controller,
        summary: "A stub HTTP controller and its slice test.",
        body: "A stub, not a resource: `scaffold` is what produces something that runs. Use \
               this when the route exists for a reason jails cannot infer.",
    },
    Explanation {
        kind: ArtifactKind::Service,
        summary: "A stub service class and its test.",
        body: "Deliberately empty of opinion. Where the shape *is* knowable -- a create \
               operation, a read, a state change -- `usecase`, `query` and `transition` \
               generate the real thing instead.",
    },
    Explanation {
        kind: ArtifactKind::Handler,
        summary: "A framework-free HTTP handler over the JDK's own server.",
        body: "The plain-Java counterpart to `controller`, for a project with no Spring. Its \
               resource path goes through the same pluraliser the SQL table name does, because \
               a second pluraliser drifts -- that is how `/categorys` once got served over a \
               table called `categories`.",
    },
    Explanation {
        kind: ArtifactKind::Class,
        summary: "A plain final class.",
        body: "No framework, no annotations, no inheritance hook. The smallest thing jails \
               will write, for when you want a file in the right package with the right header.",
    },
    Explanation {
        kind: ArtifactKind::Interface,
        summary: "A plain interface.",
        body: "For a port you intend to implement yourself. `strategy` is the version that \
               also wires the implementations up as beans.",
    },
    Explanation {
        kind: ArtifactKind::Job,
        summary: "A scheduled task and the configuration that enables scheduling.",
        body: "Note that `spring.task.scheduling.pool.size` defaults to **1**: one job that \
               blocks stalls every other scheduled job in the application. For work that must \
               survive a restart, `durable-job` is the one backed by a table.",
    },
    Explanation {
        kind: ArtifactKind::Test,
        summary: "A unit test class for an existing type.",
        body: "Named `*Test`, which is what Surefire runs. `integration-test` is the other \
               half of that rule.",
    },
    Explanation {
        kind: ArtifactKind::IntegrationTest,
        summary: "An integration test class, and the Failsafe wiring that makes it run.",
        body: "`*IT` belongs to Failsafe, which is **not** in the Spring Boot parent's default \
               build. jails generated integration tests for months that never executed once -- \
               `mvn verify` completed and reported success -- which is worse than having no \
               test, because the green build claims it passed. The write path now configures \
               Failsafe so a new generator cannot forget.",
    },
];

/// The explanation for one kind, or a refusal listing what is explained.
pub fn explain(kind: ArtifactKind) -> Result<()> {
    let entry = EXPLANATIONS
        .iter()
        .find(|entry| entry.kind == kind)
        .ok_or_else(|| {
            format!(
                "no explanation recorded for `{}`.\n       fix: add one to src/explain.rs -- \
                 `every_kind_has_an_explanation` should have caught this.",
                name_of(kind)
            )
        })?;

    println!("{}  {}", name_of(kind), entry.summary);
    println!();
    for line in entry.body.lines() {
        println!("  {line}");
    }
    Ok(())
}

fn name_of(kind: ArtifactKind) -> String {
    kind.to_possible_value()
        .expect("every ArtifactKind has a clap value")
        .get_name()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One edit per kind, enforced.
    ///
    /// This is the rule that keeps a hand-written table from becoming the
    /// editor lists: a kind added to the enum without an explanation fails the
    /// build rather than silently having none.
    #[test]
    fn every_kind_has_an_explanation() {
        let missing: Vec<String> = ArtifactKind::value_variants()
            .iter()
            .filter(|kind| !EXPLANATIONS.iter().any(|entry| entry.kind == **kind))
            .map(|kind| name_of(*kind))
            .collect();
        assert!(
            missing.is_empty(),
            "{} kind(s) have no entry in src/explain.rs: {}\n\
             Say what the artifact is for and name the trap; restating the --help line \
             earns nothing.",
            missing.len(),
            missing.join(", ")
        );
    }

    #[test]
    fn no_explanation_is_empty_or_a_restatement_of_the_summary() {
        for entry in EXPLANATIONS {
            let name = name_of(entry.kind);
            assert!(!entry.summary.trim().is_empty(), "{name} has no summary");
            assert!(
                entry.body.len() > entry.summary.len(),
                "{name}'s body adds nothing to its summary"
            );
        }
    }

    #[test]
    fn the_table_has_no_duplicate_kinds() {
        let mut seen: Vec<ArtifactKind> = Vec::new();
        for entry in EXPLANATIONS {
            assert!(
                !seen.contains(&entry.kind),
                "{} is explained twice",
                name_of(entry.kind)
            );
            seen.push(entry.kind);
        }
    }

    #[test]
    fn composite_generators_include_a_worked_invocation() {
        for kind in [
            ArtifactKind::Association,
            ArtifactKind::Transition,
            ArtifactKind::Query,
            ArtifactKind::DurableJob,
        ] {
            let entry = EXPLANATIONS
                .iter()
                .find(|entry| entry.kind == kind)
                .unwrap();
            assert!(
                entry.body.contains("Example: `jails g"),
                "{}",
                name_of(kind)
            );
        }
    }
}
