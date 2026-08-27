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

mod generate_args;
pub(crate) use generate_args::GenerateArgs;

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

mod schema;
pub(crate) use schema::*;
mod editor;
pub(crate) use editor::*;
mod tools;
pub(crate) use tools::*;
mod sql;
pub(crate) use sql::*;
mod rename;
pub(crate) use rename::*;
mod history;
pub(crate) use history::*;
mod testing;
pub(crate) use testing::*;
mod command_path;
pub(crate) use command_path::command_path_from_env;
mod output;
pub(crate) use output::Output;
mod project_args;
pub(crate) use project_args::{NewArgs, NewCliArgs};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum TestScopeArg {
    Unit,
    Integration,
    All,
}

impl From<TestScopeArg> for jails_protocol::testing::TestScope {
    fn from(value: TestScopeArg) -> Self {
        match value {
            TestScopeArg::Unit => Self::Unit,
            TestScopeArg::Integration => Self::Integration,
            TestScopeArg::All => Self::All,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum TestCompileArg {
    Auto,
    Ide,
    Build,
    None,
}

impl From<TestCompileArg> for jails_protocol::testing::TestCompilePolicy {
    fn from(value: TestCompileArg) -> Self {
        match value {
            TestCompileArg::Auto => Self::Auto,
            TestCompileArg::Ide => Self::Ide,
            TestCompileArg::Build => Self::Build,
            TestCompileArg::None => Self::None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum TestEngineArg {
    Auto,
    Build,
    Warm,
}

impl From<TestEngineArg> for jails_protocol::testing::TestEnginePolicy {
    fn from(value: TestEngineArg) -> Self {
        match value {
            TestEngineArg::Auto => Self::Auto,
            TestEngineArg::Build => Self::Build,
            TestEngineArg::Warm => Self::Warm,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum TestDatabaseArg {
    Off,
    Schema,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum RunLauncherArg {
    Auto,
    Classpath,
    BuildTool,
    Jar,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum RunCompileArg {
    Auto,
    Ide,
    Build,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum RunServicesArg {
    Existing,
    Start,
    None,
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

    /// How a command reports: readable, current JSON, or frozen v1 JSON
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

    /// Write the exact authenticated prepared transaction to this file
    #[arg(long, global = true, conflicts_with = "plan_in")]
    pub(crate) plan_out: Option<std::path::PathBuf>,

    /// Apply only this authenticated prepared transaction, without replanning
    #[arg(long, global = true, conflicts_with_all = ["plan_out", "pretend"])]
    pub(crate) plan_in: Option<std::path::PathBuf>,
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
    /// Restore sealed history and reconcile owned projections
    Repair {
        /// Simple entity name or fully qualified generated Java type
        selector: String,
        /// The only automatic repair policy: preserve history and move forward
        #[arg(long, value_enum)]
        strategy: RepairStrategy,
        /// Include evidence from this datasource when available
        #[arg(long)]
        datasource: Option<String>,
    },
    /// Evolve one field through a new forward migration
    Field {
        #[command(subcommand)]
        command: ResourceFieldCommand,
    },
    /// Add an index to a table that already exists
    Index {
        #[command(subcommand)]
        command: ResourceIndexCommand,
    },
}

#[derive(Subcommand)]
pub(crate) enum ResourceIndexCommand {
    /// Append one composite or ordered index and its migration
    ///
    ///   jails resource index add Message 'customer_id, created_at desc'
    ///
    /// The columns are the ones the table has, each optionally `asc`/`desc`
    /// and nothing else -- arbitrary SQL is refused rather than recorded as
    /// trusted generated SQL, the same rule `--index` follows at creation.
    Add {
        entity: String,
        columns: String,
        /// Subpackage containing the generated entity
        #[arg(long)]
        package: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub(crate) enum RepairStrategy {
    RollForward,
}

#[derive(Subcommand)]
pub(crate) enum ResourceFieldCommand {
    /// Add one field and append its migration
    Add {
        entity: String,
        field_spec: String,
        /// Typed value used to backfill rows before a required field is enforced
        #[arg(long, conflicts_with = "backfill_file")]
        default_literal: Option<String>,
        /// Project-relative reader-owned SQL used to backfill existing rows
        #[arg(long, conflicts_with = "default_literal")]
        backfill_file: Option<String>,
        /// Subpackage containing the generated entity
        #[arg(long)]
        package: Option<String>,
    },
    /// Rename a field with an explicit physical-column policy
    Rename {
        entity: String,
        field: String,
        new_name: String,
        #[arg(long, value_enum)]
        column: ColumnRenamePolicy,
        #[arg(long)]
        package: Option<String>,
    },
    /// Change a field's type through a checked strategy
    Type {
        entity: String,
        field: String,
        #[arg(long)]
        to: String,
        #[arg(long, value_enum)]
        strategy: TypeChangeStrategy,
        #[arg(long)]
        package: Option<String>,
    },
    /// Change whether a field accepts null values
    Nullability {
        entity: String,
        field: String,
        #[arg(
            long,
            conflicts_with = "required",
            required_unless_present = "required"
        )]
        nullable: bool,
        #[arg(
            long,
            conflicts_with = "nullable",
            required_unless_present = "nullable"
        )]
        required: bool,
        /// Project-relative SQL that removes nulls before `--required`
        #[arg(long)]
        backfill_file: Option<String>,
        #[arg(long)]
        package: Option<String>,
    },
    /// Drop a field after confirming the exact physical column
    Drop {
        entity: String,
        field: String,
        #[arg(long)]
        confirm_column: String,
        #[arg(long)]
        package: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub(crate) enum ColumnRenamePolicy {
    Preserve,
    SingleCutover,
    Rolling,
}

impl From<ColumnRenamePolicy> for jails_protocol::request::ColumnRenamePolicy {
    fn from(value: ColumnRenamePolicy) -> Self {
        match value {
            ColumnRenamePolicy::Preserve => Self::Preserve,
            ColumnRenamePolicy::SingleCutover => Self::SingleCutover,
            ColumnRenamePolicy::Rolling => Self::Rolling,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub(crate) enum TypeChangeStrategy {
    Safe,
    ExpandContract,
}

impl From<TypeChangeStrategy> for jails_protocol::request::TypeChangeStrategy {
    fn from(value: TypeChangeStrategy) -> Self {
        match value {
            TypeChangeStrategy::Safe => Self::Safe,
            TypeChangeStrategy::ExpandContract => Self::ExpandContract,
        }
    }
}

/// Everything a mutation needs that is not the mutation.
///
/// A parameter object rather than global presentation and execution flags
/// threaded through every arm: they arrive together, are consumed together by
/// [`mutate`], and are easy to swap at a call site.
#[derive(Clone)]
pub(crate) struct Invocation {
    pub(crate) pretend: bool,
    pub(crate) debug: bool,
    pub(crate) output: Output,
    pub(crate) diff: bool,
    pub(crate) ast: bool,
    pub(crate) plan_out: Option<std::path::PathBuf>,
    pub(crate) plan_in: Option<std::path::PathBuf>,
    pub(crate) command_path: Vec<String>,
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

    pub(crate) fn review(&self) -> jails_prepare::review::ReviewSelection {
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
    /// List committed project transactions from authenticated receipts
    History(HistoryArgs),
    /// Inspect one committed transaction and its exact before/after images
    Show(ShowArgs),
    /// Restore an eligible receipt's file preimages as a new forward transaction
    Undo(UndoArgs),
    /// Create a new Spring Boot project via start.spring.io
    New(NewArgs),
    /// Create a new plain Maven CLI project
    NewCli(NewCliArgs),
    /// Plan or apply a generic declarative application manifest
    App {
        #[command(subcommand)]
        command: app::AppCommand,
    },
    /// Check and generate typed named-SQL contracts
    Sql {
        #[command(subcommand)]
        command: SqlCommand,
    },
    /// Observe a live system without changing project or service state
    Introspect {
        #[command(subcommand)]
        command: IntrospectCommand,
    },
    /// Render a canonical import proposal from live schema evidence
    Pull {
        #[arg(long, value_name = "NAME")]
        datasource: String,
        #[arg(long, default_value = "public")]
        schema: String,
        #[arg(long, value_name = "GLOB")]
        table: Option<String>,
        #[arg(long, value_name = "SLICE")]
        into_slice: Option<String>,
        #[arg(long, value_enum, default_value = "existing")]
        services: RunServicesArg,
    },
    /// Compare declared, migration-derived, and live schema authorities
    Schema {
        #[command(subcommand)]
        command: SchemaCommand,
    },
    /// Versioned, read-only protocol for editor adapters
    Editor {
        #[command(subcommand)]
        command: EditorCommand,
    },
    /// Emit and check portable HTTP contracts
    Contract {
        #[command(subcommand)]
        command: ContractCommand,
    },
    /// Resolve a route and delegate an HTTP request to curl
    Request {
        #[command(flatten)]
        request: HttpRequestArgs,
    },
    /// Run a trusted project-relative JShell script noninteractively
    Runner {
        #[command(flatten)]
        runner: RunnerArgs,
    },
    /// Read bounded logs from declared Compose services
    Logs {
        #[command(flatten)]
        logs: LogsArgs,
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
    /// datetime, decimal, duration, uri, path, zone-id. The aliases timestamp,
    /// bigdecimal, and zoneid are also accepted. A capitalised one
    /// is a type your project owns -- passed through verbatim, no import,
    /// same package. `id:String` still works; jails recognises the Java
    /// spelling of its own built-ins.
    ///
    /// Collections are `list<string>` and `map<string,long>`.
    ///
    /// Examples:
    ///   jails g scaffold Payout id:uuid@pk amount:decimal paidAt:instant
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
    Generate(GenerateArgs),
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
    /// Upgrade the build to the Spring Boot and JDK jails generates against
    ///
    /// One commit, five edits. A Boot 2.7 project on JDK 21 is not one change
    /// away from Boot 4 on JDK 26: the Gradle wrapper, the plugin version, the
    /// Java release (which Gradle 9 only accepts as a toolchain), the test
    /// task's JUnit platform and H2 2.x's type names all move together, and
    /// four of the five fail naming something other than the cause. What the
    /// upgrade breaks in code you own -- Jackson 2, `javax.*` -- is reported
    /// rather than rewritten.
    #[command(visible_alias = "upgrade")]
    Modernize,
    /// Check everything that has to be true before the app can start
    Doctor {
        /// Emit the checks as JSON: {version, failures, warnings, checks[]}
        #[arg(long)]
        json: bool,
    },
    /// Explain a failure: pass a log file, pipe one in, or run it bare to start the app
    Why {
        /// A failure log path, or `bean`, `migration`, or `query`.
        log: Option<PathBuf>,
        /// The bean type, migration version, or managed query name.
        name: Option<String>,
        /// Read `.jails/last-run.log` without starting the application.
        #[arg(long)]
        last: bool,
        /// Include bounded evidence and limitations (always present in JSON).
        #[arg(long)]
        evidence: bool,
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
        #[command(subcommand)]
        command: Option<MigrateCommand>,
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
    /// Open a database client (H2, `psql` against compose postgres, or sqlite3)
    #[command(visible_alias = "dbconsole")]
    Db {
        #[command(subcommand)]
        command: Option<DbCommand>,
        /// A SQLite file; omit this to use the project's own datasource
        file: Option<PathBuf>,
        /// Open H2's browser console instead of a terminal prompt
        ///
        /// H2's own web server, not Spring's `/h2-console`: it works whether
        /// or not the application is running.
        #[arg(long)]
        web: bool,
        /// Do not `docker compose up` postgres first
        #[arg(long)]
        no_start: bool,
        /// Extra arguments forwarded to psql/sqlite3
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Boot the Spring application in JShell with context helpers
    #[command(visible_alias = "c")]
    Console {
        #[command(flatten)]
        console: ConsoleArgs,
    },
    /// Rename a managed resource, or use the legacy two-name type spelling
    Rename {
        #[command(subcommand)]
        command: Option<RenameCommand>,
        /// Legacy current simple type name
        old: Option<String>,
        /// Legacy replacement simple type name
        new: Option<String>,
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
        /// Apply the committed migration history as a post-commit effect.
        #[arg(long, requires = "datasource")]
        migrate: bool,
        /// Already available datasource used only by an explicit migrate effect.
        #[arg(long, requires = "migrate", value_name = "NAME")]
        datasource: Option<String>,
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
    #[command(
        args_conflicts_with_subcommands = true,
        subcommand_precedence_over_arg = true
    )]
    Test {
        /// Test classes or `Class#method` selectors; repeat for a union
        #[arg(value_name = "TEST_OR_METHOD")]
        requested: Vec<String>,
        /// Select unit tests, integration tests, or both
        #[arg(long, value_enum, default_value_t = TestScopeArg::Unit)]
        scope: TestScopeArg,
        /// Choose who may compile stale sources
        #[arg(long, value_enum, default_value_t = TestCompileArg::Auto)]
        compile: TestCompileArg,
        /// Choose build-tool execution, strict warm execution, or safe auto partitioning
        #[arg(long, value_enum, default_value_t = TestEngineArg::Auto)]
        engine: TestEngineArg,
        /// Keep running when source or compiled outputs change
        #[arg(long)]
        watch: bool,
        /// Select tests reachable from changed inputs, widening on uncertainty
        #[arg(long)]
        affected: bool,
        /// Rerun only what failed last time, read from the reports on disk
        #[arg(long)]
        failed: bool,
        /// Select tests carrying this tag; repeat for a union
        #[arg(long = "tag", value_name = "TAG")]
        tags: Vec<String>,
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
        /// Run repeatedly until the first failure
        #[arg(long)]
        until_fail: bool,
        /// Run the selection this many times
        #[arg(long, default_value_t = 1, value_name = "N")]
        repeat: usize,
        /// Refuse a run that exceeds this duration (for example `30s` or `2m`)
        #[arg(long, value_name = "DURATION")]
        timeout: Option<String>,
        /// Database isolation for eligible integration tests
        #[arg(long, value_enum, default_value_t = TestDatabaseArg::Off)]
        db: TestDatabaseArg,
        /// Print the canonical partitions and reasons before execution
        #[arg(long)]
        explain_selection: bool,
        #[command(subcommand)]
        command: Option<TestCommand>,
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
        /// Compatibility alias for --compile none --launcher auto
        #[arg(long, conflicts_with_all = ["launcher", "compile"])]
        no_build: bool,
        /// Select direct classpath launch, build-tool diagnosis, or a current jar
        #[arg(long, value_enum, default_value = "auto")]
        launcher: RunLauncherArg,
        /// Select the one owner allowed to compile stale output
        #[arg(long, value_enum, default_value = "auto")]
        compile: RunCompileArg,
        /// Check existing services, explicitly start them, or skip checks
        #[arg(long, value_enum, default_value = "existing")]
        services: RunServicesArg,
        /// Activate a Spring profile; repeatable
        #[arg(long = "profile")]
        profiles: Vec<String>,
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
    /// Explain what a generator kind is for, and the trap it invites
    ///
    /// The generated Javadoc carries this reasoning for whoever reads the file.
    /// This is for whoever is deciding whether to generate it -- and for an
    /// agent, which otherwise "fixes" the deliberate asymmetries.
    Explain { kind: generate::ArtifactKind },
    /// Print every subcommand, generator kind, capability and flag jails accepts
    ///
    /// Derived from the same clap definition that parses the arguments, so it
    /// cannot drift from what the binary actually takes. `--json` is what the
    /// editor plugin reads instead of keeping its own tables.
    Commands {
        #[arg(long)]
        json: bool,
    },
    /// Print a shell-completion script: source <(jails completion bash)
    Completion { shell: Shell },
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

    #[test]
    fn explain_and_completion_keep_their_own_descriptions() {
        let command = Cli::command();
        let explain = command
            .get_subcommands()
            .find(|child| child.get_name() == "explain")
            .unwrap();
        let completion = command
            .get_subcommands()
            .find(|child| child.get_name() == "completion")
            .unwrap();

        assert_eq!(
            explain.get_about().unwrap().to_string(),
            "Explain what a generator kind is for, and the trap it invites"
        );
        assert_eq!(
            completion.get_about().unwrap().to_string(),
            "Print a shell-completion script: source <(jails completion bash)"
        );
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
