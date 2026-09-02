//! The one table of "how do I invoke kind X", shared by every test that
//! needs to run a generator.
//!
//! The golden snapshots (`tests/golden.rs`) and the generate/destroy
//! agreement check (`tests/agreement.rs`) both read these scenarios, and the
//! coverage test derives *which* kinds and capabilities are exercised from the
//! steps themselves rather than from a list somebody maintains by hand, so a
//! new test never costs a second answer to "what does kind X produce".

use super::{TARGET_RELEASE, temp_dir, write_plain_fixture, write_spring_fixture};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Which fixture a scenario starts from.
#[derive(Copy, Clone)]
pub enum Fixture {
    Plain,
    Spring,
}

pub struct Scenario {
    /// Directory under `tests/golden/`.
    pub name: &'static str,
    pub fixture: Fixture,
    /// Files the scenario needs on disk before jails runs -- `g cases` reads
    /// a markdown file it did not write. Subtracted from the snapshot, since
    /// they are input rather than jails' output.
    pub seed: &'static [(&'static str, &'static str)],
    /// jails invocations, run in order.
    pub steps: &'static [&'static [&'static str]],
}

/// Every artifact kind and every capability, in the smallest invocation that
/// exercises it.
///
/// `every_kind_and_capability_has_a_golden_scenario` in `tests/golden.rs`
/// holds it: it reads the kinds and capabilities out of the binary's own help
/// and fails when one has no scenario here, so a kind cannot be added without
/// its snapshot.
pub const SCENARIOS: &[Scenario] = &[
    // ---- the canonical compiler ----
    //
    // **The scenario seeded with a whole model rather than built by `g`
    // commands**, so a canonical emitter that changes what it writes fails a
    // byte snapshot here.
    //
    // Seeded with a model rather than driven by `g` commands, because the
    // model *is* the input on this path -- and one model reaching many
    // emitters is a better snapshot than many models reaching one each: it is
    // where the packages, the imports and the shared files have to agree with
    // each other.
    Scenario {
        name: "canonical-tree",
        fixture: Fixture::Spring,
        seed: &[(
            ".jails/model.jdl",
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
             platform spring\n  build maven\n  storage postgres\n}\n\ncap json\n\n\
             entity Note @id(ent_note) {\n  use repo\n  use factory\n  \
             id: uuid @id(fld_note_id) @pk\n  title: string @id(fld_note_title) @notBlank\n  \
             status: string @id(fld_note_status)\n\n  index [status] @id(idx_note_status)\n\n  \
             command Create(title, status) @id(op_note_create) {\n    emit Created\n  }\n\n  \
             query Open(status) @id(op_note_open) {\n    limit 20\n  }\n\n  \
             transition Rename(title) @id(op_note_rename) {\n    update [title]\n  }\n\n  \
             event Created(id, title) @id(op_note_created)\n}\n\n\
             component sealed Outcome @id(cmp_outcome) {\n  variant Accepted\n  \
             variant Rejected\n}\n\ncomponent service Notifier @id(cmp_notifier) {\n}\n",
        )],
        steps: &[&["sync"]],
    },
    // ---- generators, plain Maven ----
    Scenario {
        name: "record",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[&[
            "g",
            "record",
            "Note",
            "title:string!",
            "body:string?",
            "at:instant",
        ]],
    },
    // `g field` has two halves and they are not the same command. On a
    // source-only kind it adds a Java component and nothing else; on a
    // scaffold it also appends one forward migration for the column. One
    // scenario covers both, because a snapshot of only the second lets
    // `alter table notes` be written for a `record` that owns no table.
    Scenario {
        name: "field",
        fixture: Fixture::Plain,
        seed: &[("src/main/resources/db/migration/.gitkeep", "")],
        steps: &[
            &["g", "record", "Note", "id:uuid", "title:string!"],
            &["g", "field", "Note", "createdAt:instant"],
        ],
    },
    Scenario {
        name: "field-storage",
        fixture: Fixture::Spring,
        seed: &[("src/main/resources/db/migration/.gitkeep", "")],
        steps: &[
            &["add", "db", "--no-start"],
            &["g", "scaffold", "Note", "id:uuid@pk", "title:string!"],
            &[
                "g",
                "field",
                "Note",
                "createdAt:instant",
                "--default-literal",
                "2026-08-25T12:00:00Z",
            ],
        ],
    },
    Scenario {
        name: "factory",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[
            &[
                "g",
                "record",
                "Note",
                "id:uuid",
                "title:string!",
                "createdAt:instant",
            ],
            &["g", "factory", "Note"],
        ],
    },
    Scenario {
        name: "value",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[&["g", "value", "Money", "amount:long", "currency:string"]],
    },
    Scenario {
        name: "enum-and-sealed",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[
            &["g", "enum", "Status", "ACTIVE", "CLOSED"],
            &["g", "sealed", "Outcome", "Accepted", "Rejected"],
        ],
    },
    Scenario {
        name: "strategy",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[
            &["g", "record", "Transaction", "id:uuid", "amount:long"],
            &["g", "record", "Reward", "id:uuid", "amount:long"],
            &[
                "g",
                "strategy",
                "RewardRule",
                "Coffee",
                "Large",
                "--on",
                "Transaction",
                "--yields",
                "Reward",
            ],
            &[
                "g",
                "strategy",
                "Eligibility",
                "Domestic",
                "--on",
                "Transaction",
            ],
        ],
    },
    Scenario {
        name: "class-interface-test",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[
            &["g", "class", "RingBuffer"],
            &["g", "interface", "Clock"],
            &["g", "test", "Parser"],
            &["g", "integration-test", "Checkout"],
        ],
    },
    Scenario {
        name: "command-and-cli",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[&["g", "cli", "Admin"], &["g", "command", "Greet"]],
    },
    Scenario {
        name: "repo",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[
            &["g", "record", "Note", "id:uuid@pk", "title:string"],
            &["g", "repo", "Note"],
        ],
    },
    Scenario {
        name: "migration",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[&["g", "migration", "add_note_index"]],
    },
    // ---- generators that need Spring ----
    Scenario {
        name: "scaffold-spring",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &[
                "g",
                "scaffold",
                "Note",
                "id:uuid@pk",
                "title:string!",
                "createdAt:instant@default(now())",
                "--index",
                "title, created_at desc",
            ],
        ],
    },
    Scenario {
        name: "controller-service",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&["g", "controller", "Health"], &["g", "service", "Billing"]],
    },
    Scenario {
        // The other three quarters of a route: a verb that is not GET, a
        // request body, and a response type jails cannot sample. `Verification`
        // is generated first because the controller imports it, and an import
        // of a type that is not there is exactly the failure this covers.
        name: "controller-post",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["g", "record", "Verification", "success:boolean"],
            &[
                "g",
                "controller",
                "Verify",
                "--method",
                "post",
                "--on",
                "Verification",
                "--returns",
                "Verification",
            ],
        ],
    },
    Scenario {
        name: "dto-client-job",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["g", "record", "Payout", "id:uuid", "amount:long"],
            &["g", "dto", "Payout"],
            &["g", "client", "Ledger"],
            &["g", "job", "Reconcile"],
        ],
    },
    // The call a real project makes, rather than a REST collection to delete.
    // The plain `g client` scenario above keeps the collection shape, which
    // is what a caller who names nothing still gets.
    Scenario {
        name: "client-call",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &[
                "g",
                "record",
                "ChatRequest",
                "model:string!",
                "prompt:string!",
            ],
            &["g", "record", "ChatReply", "id:string!", "text:string!"],
            &[
                "g",
                "client",
                "OpenAiChat",
                "--method",
                "post",
                "--on",
                "ChatRequest",
                "--returns",
                "ChatReply",
                "--path",
                "/v1/chat/completions",
            ],
        ],
    },
    Scenario {
        name: "socket",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&["g", "socket", "Chat"]],
    },
    Scenario {
        // `add kafka` first, because `g event` requires it: all four files it
        // writes import `org.springframework.kafka`, and without the starter
        // the project cannot compile.
        name: "event",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "kafka", "--no-start"],
            &["g", "record", "Transaction", "id:uuid", "amount:long"],
            &[
                "g",
                "event",
                "TransactionSettled",
                "id:uuid",
                "--on",
                "Transaction",
            ],
        ],
    },
    // ---- capabilities, plain Maven ----
    Scenario {
        name: "cap-csv",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[&["add", "csv", "--no-start"]],
    },
    Scenario {
        name: "cap-json",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[&["add", "json", "--no-start"]],
    },
    Scenario {
        name: "cap-sqlite",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[&["add", "sqlite", "--no-start"]],
    },
    Scenario {
        name: "cap-h2",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&["add", "h2", "--no-start"]],
    },
    Scenario {
        name: "cap-testkit-fake",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[&["add", "testkit", "fake", "--no-start"]],
    },
    Scenario {
        name: "cap-http",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[
            &["add", "http", "--no-start"],
            &["g", "handler", "WorkItem"],
        ],
    },
    Scenario {
        name: "cap-coverage",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[&["add", "coverage", "--no-start"]],
    },
    Scenario {
        name: "cap-loadtest",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["g", "controller", "Health"],
            &["add", "loadtest", "--no-start"],
        ],
    },
    Scenario {
        name: "cap-k8s",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "actuator", "observability", "docker", "--no-start"],
            &["add", "k8s", "--no-start"],
        ],
    },
    // `add format` is deliberately absent: it shells out to spotless:apply
    // as a best-effort last step, and whether that succeeds depends on the
    // JDK this machine has. A golden target has to be hermetic, and a
    // scenario whose output depends on the toolchain is not a snapshot of
    // jails. `add_format_*` under tests/cli/ covers it instead.
    // ---- capabilities that need Spring ----
    Scenario {
        name: "cap-db",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&["add", "db", "--no-start"]],
    },
    Scenario {
        name: "cap-kafka",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&["add", "kafka", "--no-start"]],
    },
    Scenario {
        name: "cap-api-actuator",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&["add", "api", "actuator", "--no-start"]],
    },
    Scenario {
        name: "cap-cache-security",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&["add", "cache", "security", "cors", "--no-start"]],
    },
    Scenario {
        name: "auth",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&["add", "security", "--no-start"], &["g", "auth", "Api"]],
    },
    Scenario {
        name: "search",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &[
                "g",
                "scaffold",
                "Article",
                "id:uuid@pk",
                "title:string!",
                "body:string",
            ],
            &["g", "search", "Article", "title", "body"],
        ],
    },
    Scenario {
        name: "webhook",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&["g", "webhook", "Provider"]],
    },
    Scenario {
        name: "cap-mail",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&["add", "mail", "--no-start"]],
    },
    Scenario {
        name: "cap-sse",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&["add", "sse", "--no-start"]],
    },
    Scenario {
        name: "cap-redis",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&["add", "redis", "--no-start"]],
    },
    Scenario {
        name: "cap-observability",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&["add", "observability", "--no-start"]],
    },
    Scenario {
        name: "cap-toxiproxy",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&["add", "toxiproxy", "--no-start"]],
    },
    // ---- generators over an existing scaffold (Spring, no database) ----
    Scenario {
        name: "usecase-query-transition",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["g", "enum", "PayoutStatus", "PENDING", "SETTLED", "FAILED"],
            &[
                "g",
                "scaffold",
                "Payout",
                "id:uuid@pk",
                "amount:long@positive",
                "status:PayoutStatus@index",
                "version:long@nonnegative",
                "createdAt:instant@default(now())",
            ],
            &[
                "g",
                "usecase",
                "RequestPayout",
                "id:uuid",
                "amount:long",
                "--on",
                "Payout",
            ],
            &[
                "g",
                "query",
                "PayoutsByStatus",
                "status:PayoutStatus",
                "--on",
                "Payout",
            ],
            &[
                "g",
                "transition",
                "ChangePayoutStatus",
                "id:uuid",
                "status:PayoutStatus",
                "version:long@nonnegative",
                "--on",
                "Payout",
            ],
        ],
    },
    // ---- generators that need a database, so `add db` is a prerequisite
    // rather than the subject: each refuses without it, naming the fix ----
    Scenario {
        name: "association-durable-job",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &["add", "json", "--no-start"],
            &[
                "g",
                "scaffold",
                "Owner",
                "id:uuid@pk",
                "name:string!",
                "createdAt:instant@default(now())",
            ],
            &[
                "g",
                "scaffold",
                "Item",
                "id:uuid@pk",
                "ownerId:uuid@index",
                "name:string!",
                "createdAt:instant@default(now())",
            ],
            &[
                "g",
                "association",
                "ItemOwner",
                "ownerId=id",
                "--on",
                "Item",
                "--yields",
                "Owner",
            ],
            &[
                "g",
                "usecase",
                "AddItem",
                "id:uuid",
                "ownerId:uuid",
                "name:string!",
                "--on",
                "Item",
            ],
            &[
                "g",
                "durable-job",
                "ItemDispatcher",
                "id:uuid",
                "ownerId:uuid",
                "name:string!",
                "--on",
                "AddItem",
                "--yields",
                "Item",
            ],
        ],
    },
    Scenario {
        name: "fetcher-workflow",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &["g", "fetcher", "Page"],
            &["g", "http-workflow", "SiteWalk", "--on", "Page"],
        ],
    },
    Scenario {
        name: "outbox-http-sink",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &["add", "json", "--no-start"],
            // `g event` requires Kafka: every file it writes imports
            // `org.springframework.kafka`, and this scenario's whole point is
            // an outbox that publishes.
            &["add", "kafka", "--no-start"],
            &[
                "g",
                "scaffold",
                "Message",
                "id:uuid@pk",
                "body:string!",
                "createdAt:instant@default(now())",
            ],
            // `--on Message` is what makes the topic ordered per message
            // rather than per event: the partition key becomes `messageId`.
            &[
                "g",
                "event",
                "MessageReceived",
                "id:uuid",
                "messageId:uuid",
                "occurredAt:instant",
                "--on",
                "Message",
            ],
            &[
                "g",
                "usecase",
                "ReceiveMessage",
                "id:uuid",
                "body:string!",
                "--on",
                "Message",
                "--yields",
                "MessageReceived",
            ],
            &[
                "g",
                "http-sink",
                "Provider",
                "--on",
                "ReceiveMessage",
                "--yields",
                "MessageReceived",
            ],
        ],
    },
    // `--via` reads a second table so a filter can name a column the target
    // does not own, which is the shape most real reads need.
    Scenario {
        name: "query-via",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &[
                "g",
                "scaffold",
                "Owner",
                "id:uuid@pk",
                "email:string!@unique",
                "createdAt:instant@default(now())",
            ],
            &[
                "g",
                "scaffold",
                "Item",
                "id:uuid@pk",
                "ownerId:uuid@index",
                "name:string!",
                "createdAt:instant@default(now())",
            ],
            &[
                "g",
                "query",
                "ItemsByOwnerEmail",
                "email:string!",
                "--on",
                "Item",
                "--via",
                "Owner",
                // The order and the ceiling, stated rather than left to the
                // adapter's default.
                "--order-by",
                "createdAt desc, name",
                "--limit",
                "20",
            ],
        ],
    },
    // Get-or-create: the first line of most real use cases.
    Scenario {
        name: "usecase-on-conflict",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &[
                "g",
                "scaffold",
                "Person",
                "id:uuid@pk",
                "email:string!@unique",
                "createdAt:instant@default(now())",
            ],
            &[
                "g",
                "usecase",
                "RegisterPerson",
                "email:string!",
                "--on",
                "Person",
                "--on-conflict",
                "email",
            ],
        ],
    },
    Scenario {
        name: "seed",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &["add", "json"],
            &["g", "scaffold", "Widget", "id:uuid@pk", "name:string!"],
            &["g", "seed", "Widget"],
        ],
    },
    // Three independent filters, any subset. The generated `IT` is what
    // proves the cast is right, because it runs against a real PostgreSQL.
    Scenario {
        name: "query-optional-filters",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &[
                "g",
                "scaffold",
                "Ticket",
                "id:long@pk",
                "status:string!",
                "category:string?",
            ],
            &[
                "g",
                "query",
                "TicketsByStatus",
                "status:string!",
                "category:string?",
                "--on",
                "Ticket",
            ],
        ],
    },
    // The three closed-set spellings a real project needs: lowercase,
    // TitleCase, and two that are not identifiers in any casing.
    Scenario {
        name: "enum-wire-values",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&[
            "g",
            "enum",
            "IssuePriority",
            "NONE=-",
            "HIGH=!",
            "URGENT=!!",
        ]],
    },
    // `--consumes form`, on the recipe that needs it most. A form post is what
    // every jQuery page sends and what a `@RequestBody` endpoint answers 415
    // to.
    Scenario {
        name: "usecase-form",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &["g", "scaffold", "Ticket", "id:long@pk", "subject:string!"],
            &[
                "g",
                "usecase",
                "OpenTicket",
                "subject:string!",
                "--on",
                "Ticket",
                "--consumes",
                "form",
                "--path",
                "/customer_api/open",
            ],
        ],
    },
    // The same value under two names on two wires. The brief's own customer
    // page reads `message.id` out of the response and posts `message_id` back,
    // and neither name follows from the other -- so the derivation that covers
    // `userId` -> `user_id` cannot cover this, and it is a name the reader
    // types.
    Scenario {
        name: "transition-bound-name",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &[
                "g",
                "scaffold",
                "Note",
                "id:long@pk",
                "body:string!",
                "seen:boolean",
                "version:long@version",
            ],
            &[
                "g",
                "transition",
                "MarkNoteSeen",
                "id:long",
                "version:long",
                "--on",
                "Note",
                "--set",
                "seen=true",
                "--if-match",
                "optional",
                "--consumes",
                "form",
                "--method",
                "post",
                "--bind",
                "id=note_id",
                "--path",
                "/customer_api/seen",
            ],
        ],
    },
    // The customer's reply: the caller sends the email they logged in with and
    // the row needs a `user_id`. `g query --via` reads across that reference
    // and nothing wrote across it, so the only expressible endpoint was one
    // that trusts the caller for a key that is not theirs to choose.
    Scenario {
        name: "usecase-resolved-key",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &["g", "enum", "SenderType", "CUSTOMER", "ADMIN"],
            &[
                "g",
                "scaffold",
                "Author",
                "id:long@pk",
                "email:string!@unique",
            ],
            &[
                "g",
                "scaffold",
                "Note",
                "id:long@pk",
                "authorId:long@index",
                "body:string!",
                "senderType:SenderType",
            ],
            &[
                "g",
                "usecase",
                "PostNote",
                "email:string!",
                "body:string!",
                "--on",
                "Note",
                "--via",
                "Author",
                "--set",
                "senderType=CUSTOMER",
                "--consumes",
                "form",
                "--path",
                "/customer_api/notes",
            ],
        ],
    },
    // A mark-as-read route, which is the shape a browser page actually sends:
    // one form field, no conditional header, and the column it sets decided by
    // the endpoint. Both halves are needed -- Spring answers 400 for a missing
    // required `If-Match` before any generated code runs, and the request does
    // not carry the flag either.
    Scenario {
        name: "transition-unconditional",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &[
                "g",
                "scaffold",
                "Note",
                "id:long@pk",
                "body:string!",
                "seen:boolean",
                "version:long@version",
            ],
            &[
                "g",
                "transition",
                "MarkSeen",
                "id:long",
                "version:long",
                "--on",
                "Note",
                "--set",
                "seen=true",
                "--if-match",
                "optional",
                "--consumes",
                "form",
                "--path",
                "/customer_api/seen",
            ],
        ],
    },
    // The component the *endpoint* decides rather than the caller. Two
    // endpoints write the same table and each must stamp its own sender; with
    // the component in the request either can forge the other's rows, and a
    // well-formed request is exactly what the forgery looks like.
    Scenario {
        name: "usecase-pinned",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &["g", "enum", "SenderType", "CUSTOMER", "ADMIN"],
            &[
                "g",
                "scaffold",
                "Note",
                "id:long@pk",
                "authorId:long",
                "body:string!",
                "senderType:SenderType",
            ],
            &[
                "g",
                "usecase",
                "PostAdminNote",
                "authorId:long",
                "body:string!",
                "--on",
                "Note",
                "--set",
                "senderType=ADMIN",
                "--consumes",
                "form",
                "--path",
                "/admin_api/notes",
            ],
        ],
    },
    // A resource whose collection URL is a fixed external contract:
    // `g scaffold User` serves `/users` and the admin frontend calls
    // `/admin_api/users`. Without `--path` the only repair is hand-editing
    // the controller jails just wrote.
    Scenario {
        name: "scaffold-path",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&[
            "g",
            "scaffold",
            "Operator",
            "id:long@pk",
            "email:string!@unique",
            "--path",
            "/admin_api/operators",
        ]],
    },
    // The other half of `--consumes form`: on a `query` it also decides the
    // verb, because `@ModelAttribute` binds from request *parameters* and on a
    // GET those are the query string. `GET /admin_api/tickets?status=open` is
    // what a browser sends.
    Scenario {
        name: "query-form",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &[
                "g",
                "scaffold",
                "Ticket",
                "id:long@pk",
                "subject:string!",
                "status:string?",
            ],
            &[
                "g",
                "query",
                "OpenTickets",
                "status:string?",
                "--on",
                "Ticket",
                "--consumes",
                "form",
                "--path",
                "/admin_api/tickets",
            ],
        ],
    },
    // The key in the URL: `PATCH /admin_api/topics/{userId}/status`, which is
    // the shape every admin frontend sends.
    // The command record loses the selector, the port takes it beside the
    // command, and the generated proof expands the variable -- three things
    // that have to move together or the route mounts a variable and drops it.
    Scenario {
        name: "transition-path",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &[
                "g",
                "scaffold",
                "Topic",
                "id:long@pk",
                "userId:long",
                "subject:string!",
                "version:long@version",
            ],
            &[
                "g",
                "transition",
                "SetTopicSubject",
                "userId:long",
                "subject:string!",
                "version:long",
                "--on",
                "Topic",
                "--select",
                "userId",
                "--method",
                "patch",
                "--path",
                "/admin_api/topics/{userId}/subject",
            ],
        ],
    },
    Scenario {
        name: "presence",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[&["add", "db", "--no-start"], &["g", "presence", "Room"]],
    },
    Scenario {
        name: "idempotency",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "--no-start"],
            &["g", "idempotency", "Request"],
        ],
    },
    Scenario {
        name: "cases",
        fixture: Fixture::Plain,
        seed: &[("docs/behaviour.md", CASES_MARKDOWN)],
        steps: &[&["g", "cases", "docs/behaviour.md"]],
    },
    Scenario {
        name: "cap-ci",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[&["add", "ci", "--no-start"]],
    },
    Scenario {
        name: "cap-docker",
        fixture: Fixture::Plain,
        seed: &[],
        steps: &[&["add", "docker", "--no-start"]],
    },
    // An operation served over HTTP by the `api` capability: the one place
    // `emit_http::proof` renders, which no other scenario reached -- the
    // proof-app manifests found it emitting `{{` for `{`, and goldens compare
    // bytes, so a renderer without a scenario is a renderer nothing checks.
    Scenario {
        name: "usecase-api-proof",
        fixture: Fixture::Spring,
        seed: &[],
        steps: &[
            &["add", "db", "api", "--no-start"],
            &["g", "scaffold", "Ticket", "id:long@pk", "subject:string!"],
            &[
                "g",
                "usecase",
                "OpenTicket",
                "subject:string!",
                "--on",
                "Ticket",
            ],
        ],
    },
];

/// `g cases` reads scenarios out of a markdown file, so the file is input,
/// not output -- the only scenario that needs anything on disk first.
const CASES_MARKDOWN: &str = "\
# Behaviour

## a payout is settled

- given a payout that is pending
- when the provider confirms it
- then it is settled

## a payout is rejected

- given a payout that is pending
- when the provider declines it
- then it is failed
";

/// The model a scenario starts from, when it does not seed one of its own.
///
/// **Every scenario is canonical.** The fixtures are hand-written poms with no
/// `.jails/` in them, which is the shape a *reader's* project has. Seeding the
/// app block here is exactly what `jails model init` writes for a foreign
/// project, so the table measures the path a reader is on after the on-ramp.
///
/// `storage none`: storage is a capability in JDL v1, and every scenario that
/// wants a database says so with `add db`, `add h2` or `add sqlite`. Declaring
/// `postgres` here would refuse on the plain-Maven fixtures and fight the
/// scenario that installs h2 over the same `spring.datasource.url`.
fn starting_model(fixture: Fixture) -> String {
    let platform = match fixture {
        Fixture::Spring => "spring",
        Fixture::Plain => "plain",
    };
    format!(
        "jdl 1\napp Demo @id(project_demo) {{\n  pkg com.example.demo\n  java {TARGET_RELEASE}\n  \
         platform {platform}\n  build maven\n  storage none\n}}\n"
    )
}

/// A scratch project with the scenario's fixture and seed files in place,
/// before any jails command has run.
pub fn prepare(scenario: &Scenario) -> PathBuf {
    let root = temp_dir(&format!("scenario-{}", scenario.name));
    match scenario.fixture {
        Fixture::Plain => write_plain_fixture(&root),
        Fixture::Spring => write_spring_fixture(&root),
    }
    let seeds_model = scenario
        .seed
        .iter()
        .any(|(rel, _)| *rel == ".jails/model.jdl");
    if !seeds_model {
        let path = root.join(".jails/model.jdl");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, starting_model(scenario.fixture)).unwrap();
    }
    for (rel, contents) in scenario.seed {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
    }
    root
}

/// Run one step, failing with the command and its stderr rather than a bare
/// exit code -- a scenario that stops working is usually a refusal with a
/// `fix:` line in it, and that line is the whole diagnosis.
pub fn run_step(root: &Path, scenario_name: &str, step: &[&str]) {
    let output = Command::new(super::bin())
        .current_dir(root)
        .args(step)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "scenario `{scenario_name}` step `jails {}` failed:\n{}{}",
        step.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn run_steps(root: &Path, scenario: &Scenario) {
    for step in scenario.steps {
        run_step(root, scenario.name, step);
    }
}

/// Every file under `dir`, as paths relative to it. `target/` is build
/// output, not something jails wrote.
pub fn file_set(dir: &Path) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    walk(dir, dir, &mut found);
    let architecture = dir.join(".jails/architecture.toml");
    if architecture.is_file() {
        found.insert(".jails/architecture.toml".to_string());
    }
    found
}

fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|n| n == "target" || n == ".jails")
            {
                continue;
            }
            walk(root, &path, out);
            continue;
        }
        out.insert(
            path.strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/"),
        );
    }
}

/// The `generate` steps of a scenario, as (kind, name).
///
/// Derived from the steps rather than declared beside them: a second list of
/// "which kinds does this scenario cover" drifts.
pub fn generate_steps(scenario: &Scenario) -> Vec<(&'static str, &'static str)> {
    scenario
        .steps
        .iter()
        .filter(|s| matches!(s.first(), Some(&"g") | Some(&"generate")))
        .map(|s| (s[1], s[2]))
        .collect()
}

/// Which artifact kinds the scenario table exercises, by canonical name.
///
/// A scenario that spells a kind with a clap alias (`fk`, `uc`, `mig`) will
/// read as *uncovered* here, and the coverage test will say so. Spell the
/// canonical name in `SCENARIOS`; the aliases are for humans at a terminal.
pub fn covered_kinds() -> BTreeSet<&'static str> {
    SCENARIOS
        .iter()
        .flat_map(generate_steps)
        .map(|(kind, _)| kind)
        .collect()
}

/// Which capabilities the scenario table exercises. `add a b --no-start`
/// installs two, so every argument up to the first flag counts.
pub fn covered_capabilities() -> BTreeSet<&'static str> {
    let mut found = BTreeSet::new();
    for scenario in SCENARIOS {
        for step in scenario.steps {
            if step.first() != Some(&"add") {
                continue;
            }
            for arg in &step[1..] {
                if arg.starts_with('-') {
                    break;
                }
                found.insert(*arg);
            }
        }
    }
    found
}

/// The canonical value names clap accepts, read out of the binary's own
/// long help.
///
/// A hand-copied list of the enum variants drifts. Parsing help means the
/// oracle is the CLI a user actually types at.
fn possible_values(subcommand: &str) -> BTreeSet<String> {
    let output = Command::new(super::bin())
        .args([subcommand, "--help"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "`jails {subcommand} --help` failed"
    );
    let help = String::from_utf8_lossy(&output.stdout);
    let mut values = BTreeSet::new();
    for line in help.lines() {
        let trimmed = line.trim_start();
        // Indentation matters: clap indents the value list further than the
        // surrounding prose, and a bullet in a doc comment would otherwise
        // read as a value.
        if line.len() - trimmed.len() < 8 {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("- ") else {
            continue;
        };
        let Some((value, _)) = rest.split_once(':') else {
            continue;
        };
        if !value.is_empty()
            && value
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            values.insert(value.to_string());
        }
    }
    values
}

pub fn cli_kinds() -> BTreeSet<String> {
    let kinds = possible_values("generate");
    // A parse that silently returns nothing would make the coverage test
    // pass while covering nothing, which is the failure mode it exists to
    // catch in the first place.
    assert!(
        kinds.contains("scaffold") && kinds.len() > 20,
        "could not read the artifact kinds out of `jails generate --help`: {kinds:?}"
    );
    kinds
}

pub fn cli_capabilities() -> BTreeSet<String> {
    let caps = possible_values("add");
    assert!(
        caps.contains("db") && caps.len() > 10,
        "could not read the capabilities out of `jails add --help`: {caps:?}"
    );
    caps
}

/// One `g` invocation from the scenario table, as arguments a planner takes.
#[derive(Debug, Default)]
pub struct Invocation {
    pub fields: Vec<String>,
    pub indexes: Vec<String>,
    pub package: Option<String>,
    pub on: Option<String>,
    pub yields: Option<String>,
    pub method: Option<jails_model::EndpointMethod>,
    pub consumes: Option<jails_model::RequestFormat>,
    pub timestamps: bool,
}

/// Read a scenario step rather than restating it.
///
/// A new kind adds a `Scenario` and not a second list, so this parity check
/// reads the same steps the golden snapshots and the destroy-agreement check
/// read. A flag it does not know is a reason to skip the step, never to guess
/// at it.
pub fn invocation(step: &[&str]) -> Option<Invocation> {
    let mut parsed = Invocation::default();
    let mut rest = step[3..].iter();
    while let Some(argument) = rest.next() {
        match *argument {
            "--timestamps" => parsed.timestamps = true,
            "--package" => parsed.package = Some((*rest.next()?).to_string()),
            "--on" => parsed.on = Some((*rest.next()?).to_string()),
            "--yields" | "--returns" => parsed.yields = Some((*rest.next()?).to_string()),
            "--method" => parsed.method = jails_model::EndpointMethod::parse(rest.next()?).ok(),
            "--consumes" => parsed.consumes = jails_model::RequestFormat::parse(rest.next()?).ok(),
            "--index" => parsed.indexes.push((*rest.next()?).to_string()),
            other if other.starts_with('-') => return None,
            other => parsed.fields.push(other.to_string()),
        }
    }
    Some(parsed)
}
