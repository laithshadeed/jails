//! **What the CLI accepts**, and nothing about what happens next.
//!
//! The clap definition: one type per vocabulary the command line has, one arm
//! per subcommand. `pending.md` §8.1 names the seam -- `main.rs` was 1,071
//! lines of which 690 were this, and the two halves answer different questions.
//! This one is read when somebody asks *what can I type*; `main`'s match is
//! read when somebody asks *what does it do*.
//!
//! Two rules the whole file is subject to, both from `CLAUDE.md`:
//! an argument with a closed value set is a `clap::ValueEnum` rather than a
//! `String` matched by hand, because that is the only way `clap_complete` can
//! emit a static completion list; and an alias meant to be typed is a
//! `visible_alias`, because a hidden one is invisible to the bash generator.

use crate::add::Capability;
use crate::app;
use crate::arguments;
use crate::compose::Runtime;
use crate::generate::ArtifactKind;
use crate::pom;
use crate::{generate, kafka};
use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum StoragePolicy {
    Preserve,
    Drop,
}

#[derive(Parser)]
#[command(
    name = "jails",
    version,
    about = "A rails-CLI-inspired tool for Spring Boot / plain Maven projects"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,

    /// Print the mvnw/mvnd/mvn/java/git/curl commands jails executes
    #[arg(long, global = true)]
    pub(crate) debug: bool,

    // `--dry-run` is an alias, not a second flag. It used to be a per-command
    // `bool` that dispatch OR'd with this one -- `dry_run || pretend` at five
    // call sites, two names for one boolean reaching two different
    // implementations. abstract.md §4.2 calls that connascence of meaning
    // crossing a module boundary, and it is why `--pretend` and apply were
    // able to disagree about what would be written. One flag, one value,
    // every command -- and `--dry-run` now works on all of them rather than
    // on the three that happened to declare it.
    /// Run, but write nothing -- print what would change and stop.
    ///
    /// Global on purpose: Rails puts `--pretend` on every generator rather
    /// than on the few that seemed risky, and the value is that you never
    /// have to remember which commands support it.
    #[arg(long, short = 'p', global = true, visible_alias = "dry-run")]
    pub(crate) pretend: bool,

    /// How a mutation reports what it did: readable, or one JSON object
    ///
    /// One projection, two encodings. §R3.4 makes a command's result a
    /// *value* -- the same status, operation list, ledger line and effects
    /// whether the run previewed or committed -- so `--output json` is an
    /// encoding of that value rather than a second description of the work.
    #[arg(long, global = true, value_enum, default_value_t = Output::Human)]
    pub(crate) output: Output,

    /// Expand mutation operations as unified current-to-prepared file diffs
    #[arg(long, global = true)]
    pub(crate) diff: bool,

    /// Show the typed semantic edits and reconciliation operations in the plan
    #[arg(long, global = true)]
    pub(crate) ast: bool,
}

/// How a mutation's [`CommandEnvelope`] is encoded.
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub(crate) enum Output {
    Human,
    Json,
}

#[derive(Subcommand)]
pub(crate) enum ResourceCommand {
    /// Reconcile recorded identity, generated source, and sealed migrations
    Status {
        /// Simple entity name or fully qualified generated Java type
        selector: String,
        /// Also request live catalog evidence from this datasource
        #[arg(long)]
        datasource: Option<String>,
    },
    /// Restore projections for preserved storage without another create migration
    Revive {
        /// Simple entity name or fully qualified generated Java type
        selector: String,
        /// Exact preserved SQL table name
        #[arg(long)]
        table: String,
    },
    /// Evolve one field through a new forward migration
    Field {
        #[command(subcommand)]
        command: ResourceFieldCommand,
    },
}

#[derive(Subcommand)]
pub(crate) enum ResourceFieldCommand {
    /// Add one field and append its migration
    Add {
        entity: String,
        field_spec: String,
        /// Subpackage containing the generated entity
        #[arg(long)]
        package: Option<String>,
    },
}

/// Everything a mutation needs that is not the mutation.
///
/// A parameter object rather than global presentation and execution flags
/// threaded through every arm: they arrive together, are consumed together by
/// [`mutate`], and are easy to swap at a call site.
#[derive(Clone, Copy)]
pub(crate) struct Invocation {
    pub(crate) pretend: bool,
    pub(crate) debug: bool,
    pub(crate) output: Output,
    pub(crate) diff: bool,
    pub(crate) ast: bool,
}

impl Invocation {
    /// The same invocation, writing nothing.
    ///
    /// `app plan` is `app apply --pretend` under another name, and this is
    /// where that is said once rather than at both call sites.
    pub(crate) fn pretending(self) -> Self {
        Self {
            pretend: true,
            ..self
        }
    }

    pub(crate) fn review(self) -> jails_prepare::review::ReviewSelection {
        jails_prepare::review::ReviewSelection {
            diff: self.diff,
            ast: self.ast,
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Describe the current Maven reactor and active module
    #[command(visible_alias = "info")]
    About {
        /// Emit a stable machine-readable project description
        #[arg(long)]
        json: bool,
    },
    /// Create a new Spring Boot project via start.spring.io
    New {
        name: String,
        /// Maven groupId, e.g. `com.intercom`
        ///
        /// Without `--package`, the base package becomes `<group>.<name>`.
        #[arg(long)]
        group: Option<String>,
        /// Base package, e.g. `com.intercom.spring`
        ///
        /// Outranks `--group`: it states the whole answer. An existing service
        /// already has a package, and it is never `com.example`.
        #[arg(long)]
        package: Option<String>,
        #[arg(long, default_value = "web")]
        deps: String,
        #[arg(long, default_value = pom::TARGET_RELEASE)]
        java: String,
        /// Skip `git init` and the .gitignore it normally sets up
        #[arg(long)]
        no_git: bool,
        /// Skip adding spring-boot-devtools (needed for `run --watch`)
        #[arg(long)]
        no_devtools: bool,
        /// Create from jails' vendored Spring fixture without contacting
        /// start.spring.io
        #[arg(long)]
        offline: bool,
        /// Write a Groovy Gradle build instead of fetching a Maven project
        ///
        /// jails writes every file itself on this path and never contacts
        /// start.spring.io, which is what makes `--boot` able to name a
        /// version Initializr no longer serves -- and what makes `--pretend`
        /// honest here when it cannot be for Maven.
        #[arg(long)]
        gradle: bool,
        /// Spring Boot version to pin, e.g. `2.7.18`. Gradle only
        ///
        /// A 2.x version selects the `buildscript {}` build file, which is the
        /// only shape that applies the Boot 2 Gradle plugin. Anything later
        /// gets `plugins {}` and Gradle's native bom support.
        #[arg(long, value_name = "VERSION")]
        boot: Option<String>,
        /// Gradle distribution the wrapper pins. Gradle only
        ///
        /// Defaults from the Boot version rather than to one number: Boot 4's
        /// plugin throws below Gradle 8.14, and Boot 2.7 does not run on 9.x.
        #[arg(long, value_name = "VERSION")]
        gradle_version: Option<String>,
        /// `bootJar` archive base name. Gradle only
        ///
        /// Omitted, there is no `bootJar` block and Gradle names the jar after
        /// the project.
        #[arg(long, value_name = "NAME")]
        jar_name: Option<String>,
        /// `bootJar` archive version. Gradle only, and only with `--jar-name`
        #[arg(long, value_name = "VERSION")]
        jar_version: Option<String>,
        /// Apply this application manifest to the new project immediately
        ///
        /// Equivalent to `new`, then `mkdir .jails`, then copying the manifest
        /// in, then `jails app apply` -- four steps that only ever appear
        /// together. The path is read relative to where you are standing.
        #[arg(long, value_name = "MANIFEST")]
        app: Option<std::path::PathBuf>,
    },
    /// Create a new plain Maven CLI project
    NewCli {
        name: String,
        /// Maven groupId, e.g. `com.intercom`
        ///
        /// Without `--package`, the base package becomes `<group>.<name>`.
        #[arg(long)]
        group: Option<String>,
        /// Base package, e.g. `com.intercom.spring`
        ///
        /// Outranks `--group`: it states the whole answer.
        #[arg(long)]
        package: Option<String>,
        /// Java release to compile against (>= 21)
        #[arg(long, default_value = pom::TARGET_RELEASE)]
        release: String,
        /// Skip `git init` and the .gitignore it normally sets up
        #[arg(long)]
        no_git: bool,
        /// Apply this application manifest to the new project immediately
        ///
        /// Equivalent to `new`, then `mkdir .jails`, then copying the manifest
        /// in, then `jails app apply` -- four steps that only ever appear
        /// together. The path is read relative to where you are standing.
        #[arg(long, value_name = "MANIFEST")]
        app: Option<std::path::PathBuf>,
    },
    /// Plan or apply a generic declarative application manifest
    App {
        #[command(subcommand)]
        command: app::AppCommand,
    },
    /// Generate a scaffold or one small Java/SQL artifact
    ///
    /// FIELDS are `name:type`, with an optional suffix:
    ///
    ///   name:string      required, must not be null
    ///   name:string!     required and must not be blank (text only)
    ///   name:string?     optional -- becomes an Optional<T> component
    ///
    /// Case is the rule. A lowercase type is one jails knows and can build a
    /// sample of: string, int, long, double, boolean, uuid, instant, date,
    /// datetime, bigdecimal, duration, uri, path, zoneid. A capitalised one
    /// is a type your project owns -- passed through verbatim, no import,
    /// same package. `id:String` still works; jails recognises the Java
    /// spelling of its own built-ins.
    ///
    /// Collections are `list<string>` and `map<string,long>`.
    ///
    /// Examples:
    ///   jails g scaffold Payout id:uuid amount:bigdecimal paidAt:instant
    ///   jails g record Money amount:long currency:Currency
    ///   jails g scaffold Note title:string! body:string?    # ! non-blank, ? optional
    ///   jails g sealed Outcome Accepted Rejected
    ///   jails g dto Payout                                  # reads the record on disk
    ///   jails g mig add_payout_index
    ///
    /// A kind whose fields jails cannot sample still gets its test, emitted
    /// whole and @Disabled naming the type -- a guess would not compile, and
    /// silence would drop the coverage.
    // verbatim_doc_comment: clap reflows a doc comment into one
    // paragraph by default, which turns the field-syntax table and the
    // examples into an unreadable run-on.
    #[command(visible_alias = "g", verbatim_doc_comment)]
    Generate {
        kind: ArtifactKind,
        name: String,
        fields: Vec<String>,
        /// Add conventional `createdAt` and `updatedAt` instant components.
        /// The generated create path supplies both; transitions advance
        /// `updated_at` in the same optimistic SQL statement.
        #[arg(long)]
        timestamps: bool,
        /// Subpackage to place the generated code in, relative to the base
        /// package -- overrides the conventional one for the kind. Pass an
        /// empty string to write straight into the base package.
        #[arg(long)]
        package: Option<String>,
        /// A composite or ordered index for the generated migration, as the
        /// column list Postgres wants. Repeatable.
        ///
        /// Per-column `@index` covers the single-column case; this is for the
        /// ones it cannot spell:
        ///   --index 'customer_id, created_at desc'
        #[arg(long = "index", value_name = "COLUMNS")]
        indexes: Vec<String>,
        /// For `strategy`, the type each implementation examines. For
        /// `usecase`, the existing scaffolded resource the operation creates;
        /// for `query`, the scaffolded resource it reads; for `durable-job`,
        /// the existing generated use case it invokes. For `command`, the
        /// dispatcher to register it in, when the project has more than one.
        ///
        ///   jails g strategy RewardRule Coffee Large --on Transaction --yields Reward
        #[arg(long = "on", value_name = "TYPE")]
        strategy_on: Option<String>,
        /// For `strategy`: what a matching implementation produces. Omit and
        /// the strategy is a predicate returning `boolean`. For
        /// `durable-job`, the resource whose stable id proves completion.
        #[arg(long = "yields", visible_alias = "returns", value_name = "TYPE")]
        strategy_yields: Option<String>,
        /// For `controller`, the HTTP method the generated route answers.
        /// Defaults to `get`.
        ///
        ///   jails g controller Verify --method post --returns Verification
        ///
        /// `--on <Type>` becomes the `@RequestBody` parameter on a verb that
        /// carries one; `--returns <Type>` is what the handler returns.
        #[arg(long, value_name = "METHOD")]
        method: Option<jails_spec::spec::kind::HttpMethod>,
    },
    /// Add one or more capabilities to an existing project: dependencies, code and tests
    ///
    /// A capability is a whole slice, not a dependency line: the artifact in
    /// pom.xml, the code that uses it, a test that proves it works, and where
    /// relevant a compose service and the properties that make it behave.
    /// Re-running one reports what is already there and changes nothing else.
    ///
    /// Examples:
    ///   jails add db                 # postgres, Flyway, Testcontainers, compose
    ///   jails add api                # RFC 9457 problem responses + validation
    ///   jails add db kafka redis     # several at once
    ///   jails add csv --name Dataset  # name the generated class
    ///   jails add security --pretend # see the plan, write nothing
    ///
    /// `jails remove <capability>` is the exact inverse.
    ///
    /// `jails add dependency <group:artifact>` is the escape hatch for a
    /// library jails has never heard of.
    #[command(
        visible_alias = "a",
        verbatim_doc_comment,
        args_conflicts_with_subcommands = true,
        subcommand_negates_reqs = true
    )]
    Add {
        #[arg(required = true, num_args = 1..)]
        capabilities: Vec<Capability>,
        /// Base name for the generated class (default: the capability's own)
        #[arg(long)]
        name: Option<String>,
        /// Write compose.yaml but do not run `docker compose up`
        #[arg(long)]
        no_start: bool,
        /// Subpackage to place the generated code in, relative to the base
        /// package -- overrides the conventional one for the kind. Pass an
        /// empty string to write straight into the base package.
        #[arg(long)]
        package: Option<String>,
        #[command(subcommand)]
        declare: Option<Declare>,
    },
    /// Apply every capability `jails.toml` declares that is not there yet
    ///
    /// `jails add` records what it applies, so `jails.toml` describes what the
    /// project is made of. `sync` reads it back and makes the project match --
    /// one command for a fresh clone, or for taking a newer jails' output.
    ///
    /// Every capability is idempotent, so a sync over a correct project
    /// changes nothing and says so. `--pretend` answers "what is missing?".
    Sync {
        /// Write compose.yaml but do not run `docker compose up`
        #[arg(long)]
        no_start: bool,
    },
    /// Remove what a matching add call would have created
    #[command(visible_alias = "rm")]
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    Remove {
        #[arg(required = true, num_args = 1..)]
        capabilities: Vec<Capability>,
        /// Base name for the generated class (default: the capability's own)
        #[arg(long)]
        name: Option<String>,
        /// Skip the confirmation prompt
        #[arg(long)]
        force: bool,
        /// Subpackage the generated code was placed in, relative to the base
        /// package -- must match the `--package` passed to `add`.
        #[arg(long)]
        package: Option<String>,
        #[command(subcommand)]
        undeclare: Option<Undeclare>,
    },
    /// Set one property in `application.properties`, as an owned setting
    ///
    /// The point of routing this through jails rather than a text editor is
    /// that jails then knows it owns the key: `jails unset` takes exactly it
    /// back out, and a later capability that wants the same key collides
    /// visibly instead of overwriting in silence.
    ///
    /// Examples:
    ///   jails set server.port=3000
    ///   jails set spring.datasource.url=jdbc:h2:mem:test --tests
    ///
    /// `--tests` writes to `src/test/resources/config/application.properties`
    /// instead. That path and not the obvious one: `classpath:/config/`
    /// outranks `classpath:/` and is *additive*, so one key there overrides
    /// one key here. `src/test/resources/application.properties` shadows the
    /// main file wholesale and silently unsets everything else.
    #[command(verbatim_doc_comment)]
    Set {
        /// key=value
        setting: String,
        /// Write the test overlay rather than the application's own config
        #[arg(long)]
        tests: bool,
    },
    /// Take one setting `jails set` wrote back out
    Unset {
        /// The property key
        key: String,
        /// The setting was written to the test overlay
        #[arg(long)]
        tests: bool,
    },
    /// Start compose services (`docker compose up -d`). No args starts all.
    Start {
        #[arg(num_args = 0..)]
        services: Vec<Runtime>,
    },
    /// Stop compose services. No args stops all.
    Stop {
        #[arg(num_args = 0..)]
        services: Vec<Runtime>,
    },
    /// Print where a Java type's source is, fully qualified
    ///
    /// The project's own sources first, then whatever `JAILS_SOURCE_PATH`
    /// names (or `deps/` when it does not). Instant, and works on a project
    /// that does not compile -- which is when you most need it and when a
    /// language server can least help. Lists every match rather than picking.
    Src {
        /// The simple type name, e.g. `JdbcClient`
        #[arg(value_name = "TYPE")]
        type_name: String,
        /// Emit the matches as JSON
        #[arg(long)]
        json: bool,
    },
    /// Run the k6 load test (`jails add loadtest`) and report what it measured
    ///
    /// jails does not parse k6's output: k6 prints p95 and p99 itself and its
    /// own thresholds decide pass or fail. What jails adds is the profile,
    /// stated before the run so the number is reproducible.
    Bench {
        /// Concurrent virtual users
        #[arg(long, default_value_t = 10)]
        vus: usize,
        /// How long to hold that load, in k6's notation (30s, 2m)
        #[arg(long, default_value = "30s")]
        duration: String,
        /// Also write k6's machine-readable summary here
        #[arg(long, value_name = "FILE")]
        export: Option<String>,
    },
    /// Write a [layout] table matching where this project already keeps things
    ///
    /// For a codebase jails did not create. Reads the directories under the
    /// base package, maps the ones it recognises onto jails' layers, and
    /// reports the ones it does not rather than guessing. Never touches
    /// [project] capabilities -- `jails sync` acts on that list.
    Adopt,
    /// Check everything that has to be true before the app can start
    Doctor {
        /// Emit the checks as JSON: {version, failures, warnings, checks[]}
        #[arg(long)]
        json: bool,
    },
    /// Explain a failure: pass a log file, pipe one in, or run it bare to start the app
    Why {
        /// A file holding the failure output. Omit to read stdin, or to
        /// start the app and read what it prints.
        log: Option<PathBuf>,
        /// Emit machine-readable diagnoses
        #[arg(long)]
        json: bool,
    },
    /// Count files and lines per layer, and the test-to-code ratio
    Stats {
        /// Emit the per-layer counts as JSON
        #[arg(long)]
        json: bool,
    },
    /// List TODO/FIXME/HACK/XXX comments across the project
    Notes {
        /// Only this tag (e.g. `jails notes fixme`)
        tag: Option<String>,
        /// Emit the notes as JSON: file/line/tag/text, ready for a quickfix list
        #[arg(long)]
        json: bool,
    },
    /// Find stale APIs and architecture violations without compiling
    Lint,
    /// List the HTTP routes this project's source declares
    Routes {
        /// Emit machine-readable output for editor integrations
        #[arg(long)]
        json: bool,
    },
    /// List the Spring beans this project's source registers, and what they inject
    Beans {
        /// Only show beans whose type or stereotype contains this text
        pattern: Option<String>,
        /// Emit machine-readable output for editor integrations
        #[arg(long)]
        json: bool,
    },
    /// Apply the migrations to a scratch database and report the first failure
    ///
    /// Not a `doctor` check: doctor is read-only by contract, and applying
    /// migrations writes. Doctor can say whether anything will run them;
    /// this says whether they work.
    Migrate {
        /// Apply to a scratch database and drop it. The only mode there is --
        /// jails does not run migrations against a real database, Flyway
        /// does. Accepted so the documented `jails migrate --check` keeps
        /// working; `--check=false` is refused rather than quietly checking
        /// anyway.
        #[arg(long, default_value_t = true)]
        check: bool,
        /// Do not `docker compose up` postgres first
        #[arg(long)]
        no_start: bool,
    },
    /// Send messages to the compose broker and inspect what is on it
    Kafka {
        #[command(subcommand)]
        command: kafka::KafkaCommand,
        /// Do not `docker compose up` kafka first
        #[arg(long)]
        no_start: bool,
    },
    /// Open a database client (`psql` against compose postgres, or sqlite3)
    #[command(visible_alias = "dbconsole")]
    Db {
        /// A SQLite file; omit this to use the compose postgres from `add db`
        file: Option<PathBuf>,
        /// Do not `docker compose up` postgres first
        #[arg(long)]
        no_start: bool,
        /// Extra arguments forwarded to psql/sqlite3
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// jshell with the project's classpath (not a Spring-booted REPL)
    #[command(visible_alias = "c")]
    Console {
        /// Skip `mvn compile` -- use whatever is already in target/
        #[arg(long)]
        no_build: bool,
        /// Extra arguments forwarded to jshell
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Rename a type and every reference to it (files, companions, call sites)
    Rename {
        /// The type's current simple name
        old: String,
        /// The name to give it
        new: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        force: bool,
    },
    /// Delete the file(s) a matching generate call would have created
    #[command(visible_alias = "d")]
    Destroy {
        kind: ArtifactKind,
        name: String,
        #[arg(long)]
        force: bool,
        /// Subpackage to place the generated code in, relative to the base
        /// package -- overrides the conventional one for the kind. Pass an
        /// empty string to write straight into the base package.
        #[arg(long)]
        package: Option<String>,
        /// Required when a scaffold has published table migration history.
        #[arg(long, value_enum)]
        storage: Option<StoragePolicy>,
        /// Exact generated table name; required with `--storage drop`.
        #[arg(long, requires = "storage")]
        confirm_table: Option<String>,
    },
    /// Inspect or change a generated resource by its durable identity
    Resource {
        #[command(subcommand)]
        command: ResourceCommand,
    },
    /// Run tests; bare names become *Test and *IT names use Failsafe
    ///
    /// FILTER accepts four shapes, in the order a reader reaches for them:
    ///
    ///   jails test Payout                 # PayoutTest
    ///   jails test PayoutIT               # Failsafe, not Surefire
    ///   jails test 'Payout#settles'       # one method
    ///   jails test src/test/java/.../PayoutTest.java:42
    ///
    /// The last resolves the `@Test` enclosing that line, which is what an
    /// editor keybinding has to hand: JUnit itself never resolves a file and
    /// a line, so something has to.
    Test {
        filter: Option<String>,
        /// Rerun only what failed last time, read from the reports on disk
        #[arg(long)]
        failed: bool,
        /// Stop at the first failing test class
        #[arg(long)]
        fail_fast: bool,
        /// After the run, print the slowest N tests from the reports
        #[arg(long, value_name = "N", num_args = 0..=1, default_missing_value = "10")]
        slowest: Option<usize>,
        /// Emit the run as JSON instead of Maven's output, read from the reports
        #[arg(long)]
        json: bool,
        /// Skip Maven and run the already-compiled classes through JUnit's
        /// console launcher. Falls back to the full path, loudly, whenever a
        /// source file is newer than the classes.
        #[arg(long)]
        fast: bool,
    },
    /// Run the tests against a resident JVM, started on demand
    ///
    /// The measurement this exists for is in `plan.md` §19.2: the *first*
    /// JUnit session in a JVM costs 464 ms where the warm ones cost 20 ms, and
    /// a cold `java` pays it every run. The daemon pays it once.
    ///
    /// It runs what is compiled and refuses when a source is newer -- the same
    /// gate as `test --fast`. It does not compile, because the editor's
    /// language server already writes `target/classes` on save (§19.5).
    Testd {
        filter: Option<String>,
        /// Run only the tests reachable from what has changed in the working
        /// tree, via a reverse-dependency index built from the constant pools
        /// already in `target/`. Widens to everything, loudly, whenever it
        /// cannot know -- no git, a source with no compiled class, nothing
        /// compiled yet
        #[arg(long, conflicts_with_all = ["filter", "stop", "status"])]
        affected: bool,
        /// Stop this project's daemon
        #[arg(long, conflicts_with_all = ["status", "filter"])]
        stop: bool,
        /// Say whether one is running, and where
        #[arg(long, conflicts_with = "filter")]
        status: bool,
    },
    /// Build the project (mvn package)
    Build,
    /// Delete Maven's `target/` directory (`mvn clean`)
    Clean,
    /// Reformat every source file in place (needs `jails add format`)
    Fmt,
    /// Format check + compile + tests (`mvn clean verify`)
    Check,
    /// Pass uncommon arguments through to the project's Maven wrapper
    Mvn {
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Forward arguments to this project's Gradle wrapper
    Gradle {
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Find, compile and run the project's main class
    Run {
        /// Skip compiling/building -- run whatever's already in target/
        #[arg(long)]
        no_build: bool,
        /// Recompile on source changes and keep the app running (Spring
        /// Boot + spring-boot-devtools only -- devtools restarts itself
        /// once target/classes changes)
        #[arg(long)]
        watch: bool,
        /// Everything after `--` is forwarded to the program itself:
        /// `jails run -- normalise input.json`. `last` rather than
        /// `trailing_var_arg` (clap rejects both together) so that a forwarded
        /// `--help` reaches the program instead of being eaten by jails.
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Prepare this machine for fast runs: container reuse, and what else it needs
    ///
    /// Machine-level, not project-level. The one setting jails cannot write
    /// into a project is the Testcontainers reuse flag: it is read from
    /// `~/.testcontainers.properties` or the environment, and a file on the
    /// classpath does nothing at all -- so a generated `withReuse(true)` is
    /// ignored until this has run, and every test run pays for a fresh
    /// PostgreSQL.
    Setup {},
    /// Print a shell-completion script: source <(jails completion bash)
    /// Explain what a generator kind is for, and the trap it invites
    ///
    /// The generated Javadoc carries this reasoning for whoever reads the file.
    /// This is for whoever is deciding whether to generate it -- and for an
    /// agent, which otherwise "fixes" the deliberate asymmetries.
    Explain {
        kind: generate::ArtifactKind,
    },
    /// Print every subcommand, generator kind, capability and flag jails accepts
    ///
    /// Derived from the same clap definition that parses the arguments, so it
    /// cannot drift from what the binary actually takes. `--json` is what the
    /// editor plugin reads instead of keeping its own tables.
    Commands {
        #[arg(long)]
        json: bool,
    },
    Completion {
        shell: Shell,
    },
}

/// The one resource `add` takes that is not a capability.
///
/// A separate subcommand rather than another `Capability` value, because a
/// capability is a closed vocabulary jails knows the meaning of and this is
/// the opposite: an artifact jails has never heard of, named by the reader.
/// Nesting it under `add` keeps one verb for "put this in the project" --
/// `args_conflicts_with_subcommands` is what lets `jails add db` and
/// `jails add dependency com.h2database:h2` both parse.
#[derive(Subcommand)]
pub(crate) enum Declare {
    /// Splice one artifact into the build file, and record who asked
    ///
    /// For a library jails has no capability for. It writes the dependency
    /// and nothing else -- no wiring, no test, no `jails.toml` entry -- and
    /// `jails remove dependency <coordinate>` takes it back out.
    ///
    /// Examples:
    ///   jails add dependency com.h2database:h2 --scope runtime
    ///   jails add dependency org.jsoup:jsoup --version 1.18.3
    ///
    /// Omit `--version` when the project's parent or an imported BOM manages
    /// it. Maven refuses to read a pom whose dependency has no version and
    /// nothing manages it, so jails asks rather than guessing.
    #[command(verbatim_doc_comment)]
    Dependency {
        /// group:artifact
        coordinate: String,
        /// Pin the version. Omit when a parent or BOM manages it.
        #[arg(long)]
        version: Option<String>,
        /// compile (default), runtime, or test
        #[arg(long, default_value = "compile")]
        scope: arguments::DependencyScope,
    },
}

/// The exact inverse, under `remove`.
#[derive(Subcommand)]
pub(crate) enum Undeclare {
    /// Take one declared artifact back out of the build file
    Dependency {
        /// group:artifact
        coordinate: String,
    },
    /// Take JUnit's console launcher back off the test classpath
    ///
    /// `jails test --fast` puts it there, and records it as an entity jails
    /// owns rather than as a side effect of how the tests were run. This is
    /// the other half: a dependency nothing can name and nothing can remove is
    /// the failure the ownership model exists to prevent.
    #[command(name = "fast-test")]
    FastTest,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use std::collections::BTreeSet;

    #[test]
    fn feature_inventory_covers_the_live_clap_tree_exactly_once() {
        let source = include_str!("../docs/feature-inventory.tsv");
        let mut inventoried = BTreeSet::new();
        for (number, line) in source.lines().enumerate() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(
                fields.len(),
                4,
                "inventory line {} is not four columns",
                number + 1
            );
            assert!(!fields[1].is_empty(), "line {} has no owner", number + 1);
            assert!(
                !fields[2].is_empty(),
                "line {} has no side-effect class",
                number + 1
            );
            assert!(
                !fields[3].is_empty(),
                "line {} has no entry point",
                number + 1
            );
            assert!(
                inventoried.insert(fields[0].to_string()),
                "{} is inventoried twice",
                fields[0]
            );
        }

        let mut accepted = BTreeSet::new();
        collect_commands(&Cli::command(), "", &mut accepted);
        assert_eq!(inventoried, accepted);
    }

    fn collect_commands(command: &clap::Command, prefix: &str, paths: &mut BTreeSet<String>) {
        for child in command.get_subcommands() {
            let path = match prefix.is_empty() {
                true => child.get_name().to_string(),
                false => format!("{prefix} {}", child.get_name()),
            };
            paths.insert(path.clone());
            collect_commands(child, &path, paths);
        }
    }
}
