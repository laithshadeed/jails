use crate::model::{Artifact, Change, Layer, Project};
use jails_support::Result;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

// The vocabulary below the generator layer. Re-exported so `generate::Field`
// and `generate::main_dir` still resolve for every caller inside this layer;
// what moved is where they are *defined*, and therefore which way the
// dependency points. See `crate::spec`.
pub use crate::spec::kind::ArtifactKind;
pub use crate::spec::layout;
pub use crate::spec::{field::*, paths::*};
// The one parser, which lives with `FieldSpec` a layer up rather than with the
// `Field` it produces -- `pending.md` §6.3. Re-exported here so every generator
// keeps saying `parse_fields`, the same job the facade block in `lib.rs` does
// for the crates below.
pub use jails_protocol::declaration::parse_fields;
// The name a recipe records under, which is an *identity* rule and so belongs
// with the vocabulary rather than with the generators that read it --
// `pending.md` §6.4. Re-exported so every generator keeps its spelling.
pub use jails_protocol::recipe::{kind_suffix, recorded_name, strip_redundant_suffix};

mod migration;
pub use migration::*;

mod web;
pub use web::*;

mod cli;
pub use cli::*;

mod closed;
mod domain;
use closed::*;
pub(crate) use domain::*;

mod repository;
pub(crate) use repository::*;

mod recipes;
mod write;
pub(crate) use recipes::*;
pub use write::*;

mod scaffold;
pub(crate) use scaffold::*;

/// Say what a foreign build cost this generation, before printing the files.
///
/// Not a warning about the project: a statement about *this output*, naming
/// the two shapes that changed and the dependencies jails would have added and
/// cannot. Silence here is the failure `plan.md` §12 calls out -- a tool that
/// half-understands a build reports a dependency the build does not have.
/// Say what shape jails chose, when it could not read the build file.
///
/// `pub` because the V2 route reports it too: a foreign build file changes the
/// Java that gets emitted -- `JdbcClient` becomes plain JDBC, JSpecify goes
/// away -- and a reader who is not told that has no way to find out except by
/// reading the generated code.
/// Move a change's DDL to where this project's schema actually lives.
///
/// **One rule, at the one place a `Change` is complete**, rather than a hook
/// per kind. Nine generators write a migration -- `scaffold`, `association`,
/// `presence`, `idempotency`, `search`, the outbox, a durable job, `g field`,
/// a closed-set widening -- and the first version of this covered `scaffold`
/// alone, so `g association` on the same project wrote a foreign key into
/// `db/migration/` where nothing would ever run it.
///
/// Three cases, and the project decides which:
///
/// - **Flyway is there**: the migration stays a migration.
/// - **`schema.sql` is there**: the DDL becomes a `codemod` marked block in
///   it, which is what makes `destroy` take out exactly the table jails wrote.
/// - **Neither**: the DDL is dropped and said out loud with both fixes. A
///   migration in a directory nothing reads is a table nobody creates,
///   reported as success.
///
/// Keyed by the migration's *description* rather than its `V00n`: the number
/// is assigned by counting what is already there, so keying on it would make a
/// regeneration append a second copy of the same table instead of replacing
/// the block it wrote.
///
/// A `drop_` migration is left out deliberately, and reported. Retiring the
/// marked block is what removes the declaration, which is the whole story for
/// a database `spring.sql.init` recreates -- but an existing one still has the
/// table, and appending `drop table` to a script that runs on every start-up
/// would fail on the second one.
pub fn redirect_ddl_to_schema(project: &Project, change: &mut Change) {
    if project.has_directory("src/main/resources/db/migration") {
        return;
    }
    let to_schema = crate::generate::scaffold::has_schema_sql(project);
    let mut kept = Vec::with_capacity(change.files.len());
    for artifact in std::mem::take(&mut change.files) {
        let Some(description) = migration_description(&artifact) else {
            kept.push(artifact);
            continue;
        };
        if !to_schema {
            println!(
                "note: `{description}` was not written -- this project has neither Flyway \
                 migrations\n      nor a `schema.sql`, so jails has nowhere to put DDL it can \
                 also take back out.\n      fix: run `jails add db` for Flyway, or create \
                 src/main/resources/schema.sql\n           (with spring.sql.init.mode=always) \
                 and generate again."
            );
            continue;
        }
        if description.starts_with("drop_") {
            println!(
                "note: `{description}` was not written -- this project's schema is \
                 `schema.sql`,\n      and removing the declaration is what retires the table \
                 there. An existing\n      database still has it."
            );
            continue;
        }
        change.marked.push(jails_project::model::MarkedBlock {
            path: crate::generate::scaffold::SCHEMA_SQL.to_string(),
            marker: description.replace('_', "-"),
            // Minus the header `create_table` opens with: inside a block jails
            // rewrites in place, "forward-only migration" is false, and the
            // markers already say who wrote it.
            settings: artifact
                .contents
                .lines()
                .skip_while(|line| line.starts_with("-- Forward-only"))
                .map(str::to_string)
                .collect(),
        });
    }
    change.files = kept;
}

/// The `create_users` half of `V001__create_users.sql`, for an artifact bound
/// for the migrations directory.
///
/// **Matched on the path, not on `kind`.** Nine generators write a migration
/// and each labels it differently -- "association migration", "presence
/// migration", "closed-set migration" -- so a rule keyed on the label covered
/// exactly the one that says `"migration"` and silently missed the other
/// eight. Where the file is going is the thing that actually decides this.
fn migration_description(artifact: &Artifact) -> Option<String> {
    let name = artifact.path.file_name()?.to_str()?;
    if !artifact
        .path
        .parent()?
        .ends_with("src/main/resources/db/migration")
    {
        return None;
    }
    let (_, rest) = name.split_once("__")?;
    Some(rest.strip_suffix(".sql")?.to_string())
}

/// Say that the architecture suite jails is about to write will fail on code
/// the reader already had, and how to accept that.
///
/// Printed here rather than in `architecture.rs` because this module owns the
/// terminal -- the boundary
/// `only_deliberate_output_modules_print_to_the_terminal` holds. That module
/// decides what to write and returns the sentence.
pub fn report_adoption_note(project: &Project) {
    if let Some(note) = crate::architecture::adoption_note(project) {
        println!("{note}");
    }
}

pub fn report_degraded_shape(project: &Project, change: &Change) {
    let crate::build::Build::Foreign(tool) = project.build() else {
        return;
    };
    println!("note: this is a {tool} project, and jails does not read {tool} build files.");
    println!("      Generated code therefore assumes plain JDBC (no Spring `JdbcClient`)");
    println!("      and no JSpecify, because those are read off a pom.xml that is not here.");
    // Deduplicated, because the write path applies its rules independently:
    // AssertJ is required by anything writing a test *and* by the scaffold's
    // own companion tests, so a scaffold listed it twice and the reader was
    // left wondering which of the two they had missed.
    let mut said = BTreeSet::new();
    for dep in &change.deps {
        let coordinate = format!("{}:{}", dep.group_id, dep.artifact_id);
        if said.insert(coordinate.clone()) {
            println!("      Add yourself: {coordinate}");
        }
    }
    // Named by what it *does*, not by the Maven artifact that would have done
    // it. A Gradle project cannot add `maven-failsafe-plugin`, so printing its
    // name is an instruction the reader cannot carry out -- and the thing they
    // actually have to arrange is that `*IT` classes get run at all, which
    // every build spells differently.
    for (feature, _) in &change.plugins {
        if !said.insert(feature.to_string()) {
            continue;
        }
        match feature {
            jails_protocol::feature::BuildFeature::IntegrationTests => println!(
                "      Arrange yourself: your build must run `*IT` classes. Maven needs \
                 Failsafe for that; whatever {tool} calls it, an integration test nothing \
                 executes is a green build that proves nothing."
            ),
            other => println!(
                "      Arrange yourself: your build must {}. Maven uses `{}`; {tool} spells \
                 it differently.",
                other.purpose(),
                other.maven_artifact_id()
            ),
        }
    }
}

/// Spring-only generator kinds refuse politely rather than writing code that
/// cannot compile.
fn require_spring_project(project: &Project, kind: &str) -> Result<()> {
    crate::spring::require_spring(project.flavor(), kind)
}

/// A name that becomes a Java type must read like one.
///
/// plan.md P3.4, from modern.md 3.2: `g association Message_user` wrote
/// `Message_userAssociationIT.java` -- a class name with a word starting
/// mid-identifier in lowercase, which reads as machine output rather than as
/// code somebody wrote. `recorded_name` capitalises the first letter and
/// stops, so the underscore travelled all the way into the file name.
///
/// Refused rather than normalised, on the rule the field spec already
/// follows: `Message_user` could mean `MessageUser` or `MessageBelongsToUser`
/// and jails cannot tell, so the reader picks. `migration` and `cases` are
/// exempt because their name is not a Java class -- `recorded_name` already
/// says so and this reads the same condition through it.
fn require_java_type_name(kind: ArtifactKind, name: &str) -> Result<()> {
    if matches!(kind, ArtifactKind::Cases | ArtifactKind::Migration) || !name.contains('_') {
        return Ok(());
    }
    let suggestion: String = name
        .split('_')
        .map(jails_spec::spec::field::capitalize)
        .collect();
    Err(format!(
        "`{name}` becomes a Java type name, and `_` does not belong in one.\n       \
         fix: name it `{suggestion}`, or spell out what the `_` stands for."
    )
    .into())
}

/// What a persistent `generate` intends, computed without writing anything.
///
/// plan.md §R6.2 turns a generator into an `IntentSpec` that becomes a
/// `DesiredChange` -- "the same direct-owner semantics as an equivalent
/// manifest row" -- and that is only possible if the intent can be computed
/// separately from being carried out. This is that half: it renders every
/// artifact, plans the `package-info.java` files, and adds the dependencies
/// and plugins the emitted code needs, and it touches nothing.
///
/// The one-shot kinds are not here. `field`, `migration` and `cases` each have
/// their own policy row in §R6.2 -- an active overlay, a serial allocation, a
/// source-hash receipt -- and folding them into the persistent path is what
/// made an edited `fields` line arrive as a new intent against files that
/// already existed.
///
/// The parameters that describe *what* to generate arrive as one [`Recipe`],
/// which is the value `artifacts_for` already takes. They were eight
/// positional arguments here and grew to nine the first time an endpoint
/// needed a verb; a group of values computed together and consumed together is
/// a parameter object, which is the same call this file makes one line down.
pub fn plan_recipe(
    project: &Project,
    recipe: &Recipe<'_>,
    package: Option<&str>,
) -> Result<Change> {
    let kind = recipe.kind;
    // Asked, not listed. `recipe::is_persistent` is the one owner of which half
    // a kind is in -- see `pending.md` §6.4 for the three copies this replaces.
    if !jails_protocol::recipe::is_persistent(kind) {
        return Err(format!(
            "`{}` is a one-shot, not a persistent artifact, so it has no recipe plan.\n       \
             fix: plan it through its own policy -- an overlay, a serial allocation or a \
             source-hash receipt. See plan.md §R6.2.",
            format!("{kind:?}").to_lowercase()
        )
        .into());
    }
    let root = project.root().to_path_buf();
    let name = recorded_name(kind, recipe.name);
    require_java_type_name(kind, &name)?;
    let artifacts = artifacts_for(
        project,
        &Recipe {
            name: &name,
            ..*recipe
        },
        package,
    )?;

    // Every write this command performs, in one list, before any of it is
    // previewed or applied. `package-info.java` used to be created as a side
    // effect of writing a class, so `--pretend` named two files and the real
    // run wrote three.
    let mut artifacts = artifacts;
    let mut planned = planned_package_infos(&root, project.pom(), &artifacts);
    if !planned.is_empty() {
        planned.append(&mut artifacts);
        artifacts = planned;
    }

    let mut change = Change {
        files: artifacts,
        ..Change::default()
    };
    if writes_a_test(&change.files)
        && project.lacks_dependency("org.assertj", "assertj-core")
        && !project.pom().contains("spring-boot-starter-test")
        && !project.pom().contains("spring-boot-starter-webmvc-test")
    {
        change.deps.push(crate::pom::assertj(project.flavor()));
    }
    match kind {
        ArtifactKind::Dto => change
            .deps
            .push(*crate::spring::validation_dependency(project.flavor())),
        ArtifactKind::Scaffold => change.deps.extend([
            *crate::spring::validation_dependency(project.flavor()),
            crate::architecture::ARCHUNIT_JUNIT5,
        ]),
        ArtifactKind::Client => {
            change.deps.push(crate::spring::RESTCLIENT_STARTER);
            change
                .properties
                .extend(crate::spring::http::client_properties(
                    &crate::spring::http::client_group(recipe.name),
                ));
        }
        ArtifactKind::Fetcher => change.deps.extend([
            crate::spring::APACHE_HTTPCLIENT,
            crate::spring::ACTUATOR_STARTER,
        ]),
        ArtifactKind::Socket => change.deps.push(crate::spring::WEBSOCKET_STARTER),
        ArtifactKind::Event => change.deps.extend([
            crate::spring::TESTCONTAINERS_KAFKA,
            crate::spring::SPRING_TESTCONTAINERS,
        ]),
        _ => {}
    }
    if writes_an_it(&change.files) {
        change.plugins.push((
            jails_protocol::feature::BuildFeature::IntegrationTests,
            crate::spring::failsafe_plugin(project.flavor()).to_string(),
        ));
    }
    if writes_a_webmvc_test(&change.files)
        && project.lacks_dependency(
            "org.springframework.boot",
            "spring-boot-starter-webmvc-test",
        )
    {
        change.deps.push(crate::pom::WEBMVC_TEST_STARTER);
    }
    // The other two things a recipe contributes to files it does not own: the
    // dispatch line that makes a generated command reachable, and one durable
    // job's limits in the app-wide test property source. Both were `std::fs`
    // calls after the plan, so the routes wrote the class and left it
    // unreachable, and wrote the job and left the file unwritten.
    if kind == ArtifactKind::Command
        && let Some(registration) =
            crate::generate::cli::planned_registration(project, &name, recipe.strategy_on)
    {
        change.registrations.push(registration);
    }
    if kind == ArtifactKind::Cli {
        change.main_class = crate::generate::cli::planned_entry_point(
            project,
            &project.package_named(layout::CLI, package),
            &name,
        );
    }
    if kind == ArtifactKind::DurableJob {
        change
            .marked
            .push(crate::spring::durable_job_test_properties(&name));
    }
    // The DDL, when this project's schema is `schema.sql` rather than Flyway.
    redirect_ddl_to_schema(project, &mut change);

    Ok(change)
}

/// `--timestamps` as the two components it means.
///
/// Expanded once, here, because a recipe never sees the flag: by the time
/// there is a spec the two extra components are ordinary ones, and recording
/// the flag as well would make one request two facts. Both the CLI and a
/// manifest row go through this, so the two cannot disagree about what the
/// flag expands to.
pub fn with_timestamps(kind: ArtifactKind, fields: &[String]) -> Result<Vec<String>> {
    if !matches!(kind, ArtifactKind::Scaffold) {
        return Err(
            jails_support::Failure::Told("--timestamps belongs to scaffold, where the record, DDL, adapter, and HTTP contracts can evolve together.\n       \
             fix: use `jails g scaffold <Name> ... --timestamps`."
                .to_string()),
        );
    }
    let parsed = parse_fields(fields)?;
    for conventional in ["createdAt", "updatedAt"] {
        if parsed.iter().any(|field| field.name == conventional) {
            return Err(format!(
                "--timestamps would duplicate `{conventional}`.\n       \
                 fix: remove the hand-declared timestamp or omit --timestamps."
            )
            .into());
        }
    }
    Ok(fields
        .iter()
        .cloned()
        .chain([
            "createdAt:instant".to_string(),
            "updatedAt:instant".to_string(),
        ])
        .collect())
}

/// One `generate` invocation, as a value.
///
/// The loose arguments this replaces were `abstract.md` §2's Long Parameter
/// List at its worst: `generate`, `destroy` and `app apply` each passed the
/// same ones in the same order, so two `Option<&str>` slots swapped by mistake
/// still compiled.
pub struct Recipe<'a> {
    pub kind: ArtifactKind,
    pub name: &'a str,
    pub fields: &'a [String],
    pub indexes: &'a [String],
    pub strategy_on: Option<&'a str>,
    pub strategy_yields: Option<&'a str>,
    /// For `query`, the second resource read alongside `--on`, so a filter may
    /// name a component of either. plan.md P8.1.
    pub via: Option<&'a str>,
    /// For `query`, the declared result order and row ceiling. plan.md P8.2.
    pub order_by: Option<&'a str>,
    pub limit: Option<u32>,
    /// For `usecase`, the target component whose unique constraint turns the
    /// create into a get-or-create. plan.md P8.3.
    pub on_conflict: Option<&'a str>,
    /// The route a generated endpoint answers, instead of the derived one.
    /// plan.md P8.7.
    pub path: Option<&'a str>,
    /// The HTTP verb an endpoint answers, when the recipe has one.
    ///
    /// `None` is "not asked", never "GET": the default belongs at the one
    /// place that renders a mapping annotation, not spread across every
    /// caller that has no endpoint to describe.
    pub method: Option<jails_spec::spec::kind::HttpMethod>,
    /// How this recipe's endpoint reads its request, when it has one.
    ///
    /// `None` is "not asked", never "JSON", for the same reason `method`'s is.
    pub consumes: Option<jails_spec::spec::kind::WireFormat>,
    /// Which component identifies the row a `transition` updates.
    ///
    /// `None` is "not asked", and [`Recipe::selector`] applies the default --
    /// `id`, which is what every transition selected on before this existed.
    pub select: Option<&'a str>,
}

impl Recipe<'_> {
    /// What this recipe's endpoint answers, with the default applied.
    ///
    /// Stated once. `GET` returning the resource name is what `g controller`
    /// emitted before `--method` existed, and it stays the default -- but it
    /// is now a default rather than the only shape, which is the whole of
    /// missing.md §2.
    pub fn http_method(&self) -> jails_spec::spec::kind::HttpMethod {
        self.method
            .unwrap_or(jails_spec::spec::kind::HttpMethod::Get)
    }

    /// The component a `transition` selects its row by, with the default
    /// applied.
    ///
    /// `id` unless the caller named another. A resource whose natural key is
    /// not called `id` -- a conversation keyed by `user_id`, a row a URL
    /// addresses by something else -- could not be updated at all before,
    /// because the name was a literal at four sites and in the SQL predicate.
    pub fn selector(&self) -> &str {
        self.select.unwrap_or("id")
    }

    /// How this recipe's endpoint reads its request, with the default applied.
    ///
    /// JSON, because that is what every endpoint jails wrote before
    /// `--consumes` existed and what an API client sends. Stated once, here,
    /// so no template has to remember it.
    pub fn request_format(&self) -> jails_spec::spec::kind::WireFormat {
        self.consumes
            .unwrap_or(jails_spec::spec::kind::WireFormat::Json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use jails_testkit::CWD_LOCK;

    fn scratch(label: &str) -> PathBuf {
        jails_support::scratch::ScratchDir::in_temp(&format!("jails-generate-test-{label}"))
            .unwrap()
            .keep()
    }

    /// Every built-in field type has a JSON sample, in both tables that write
    /// one.
    ///
    /// `pending.md` §1.3: the field-type vocabulary and the JSON sample tables
    /// are two spellings of one set, and they had drifted five apart -- which
    /// is how a `uri` component came to document a request its own record
    /// refuses. `path` had no sample in `scaffold`, and `currency` and `bytes`
    /// had none in `spring::workflow`, so a scaffold or a use-case over one of
    /// those emitted a request body with the field silently absent.
    ///
    /// The vocabulary is `jails_spec::spec::BUILTIN_FIELD_TYPES` and nothing
    /// else, so a type added there fails this until it has a sample. That is
    /// the relationship the two tables should have had all along: one of them
    /// is the list, the others answer to it.
    #[test]
    fn every_builtin_type_has_a_json_sample() {
        let dir = scratch("json-samples");
        let project = crate::model::Project::inspect(&dir).unwrap();
        let slice = crate::model::Slice::new(&project, None);
        let mut missing = Vec::new();
        for java_type in jails_spec::spec::builtin_java_types() {
            // The token is not what these tables read -- they match on the
            // resolved Java type -- so the field is built directly.
            let field = Field {
                name: "sample".to_string(),
                column: "sample".to_string(),
                java_type: java_type.to_string(),
                imports: Vec::new(),
                optionality: Optionality::Required,
                owned: false,
                collection: false,
                constraints: Default::default(),
            };
            if crate::generate::scaffold::json_sample(&project, "com.example.demo.domain", &field)
                .is_none()
            {
                missing.push(format!("scaffold: {java_type}"));
            }
            if crate::spring::json_sample(&slice, &field).is_none() {
                missing.push(format!("spring::workflow: {java_type}"));
            }
        }
        std::fs::remove_dir_all(&dir).ok();
        assert!(
            missing.is_empty(),
            "these built-in field types have no JSON sample, so a generated request body \
             documents a field it then omits:\n  {}",
            missing.join("\n  ")
        );
    }

    /// Typing the name the type will actually have is the obvious thing to
    /// do, and it used to produce `RewardHistoryServiceService.java`. A real
    /// project renamed four generated files by hand because of this.
    #[test]
    fn a_name_that_already_carries_its_kinds_suffix_does_not_get_it_twice() {
        for (kind, given, want) in [
            (
                ArtifactKind::Service,
                "RewardHistoryService",
                "RewardHistory",
            ),
            (ArtifactKind::Controller, "RewardController", "Reward"),
            (ArtifactKind::Repo, "RewardRepository", "Reward"),
            (ArtifactKind::Test, "MoneyTest", "Money"),
            (ArtifactKind::IntegrationTest, "QueueIT", "Queue"),
            (ArtifactKind::Job, "CleanupJob", "Cleanup"),
            (ArtifactKind::Client, "CatalogClient", "Catalog"),
            (ArtifactKind::Cli, "AdminCli", "Admin"),
        ] {
            assert_eq!(strip_redundant_suffix(kind, given), want, "{given}");
        }
    }

    #[test]
    fn a_name_without_the_suffix_is_left_alone() {
        assert_eq!(
            strip_redundant_suffix(ArtifactKind::Service, "RewardHistory"),
            "RewardHistory"
        );
        // `Repository` is matched whole -- `Rewards` does not lose its `s`.
        assert_eq!(
            strip_redundant_suffix(ArtifactKind::Repo, "Rewards"),
            "Rewards"
        );
    }

    /// Stripping the whole name would leave nothing to name the file after,
    /// so `g service Service` means a type called `Service`.
    #[test]
    fn a_name_that_is_only_the_suffix_survives() {
        assert_eq!(
            strip_redundant_suffix(ArtifactKind::Service, "Service"),
            "Service"
        );
        assert_eq!(strip_redundant_suffix(ArtifactKind::Test, "Test"), "Test");
    }

    /// `scaffold` spans Controller, Service and Repository at once; stripping
    /// any one of them would corrupt the other two.
    #[test]
    fn scaffold_and_record_use_the_name_verbatim() {
        assert_eq!(
            strip_redundant_suffix(ArtifactKind::Scaffold, "RewardService"),
            "RewardService"
        );
        assert_eq!(
            strip_redundant_suffix(ArtifactKind::Record, "RewardResponse"),
            "RewardResponse"
        );
    }

    #[test]
    fn capitalize_uppercases_first_letter_only() {
        assert_eq!(capitalize("post"), "Post");
        assert_eq!(capitalize("Post"), "Post");
        assert_eq!(capitalize(""), "");
    }

    #[test]
    fn find_project_root_walks_up_to_pom_xml() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("project-root");
        fs::write(root.join("pom.xml"), "<project/>").unwrap();
        let nested = root.join("src/main/java/com/example");
        fs::create_dir_all(&nested).unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&nested).unwrap();
        let found = find_project_root();
        std::env::set_current_dir(original_cwd).unwrap();

        assert_eq!(found.unwrap(), root);
    }

    #[test]
    fn base_package_reads_the_application_class_package() {
        let root = scratch("base-package");
        let src = root.join("src/main/java/com/example/blog");
        fs::create_dir_all(&src).unwrap();
        fs::write(
            src.join("BlogApplication.java"),
            "package com.example.blog;\n\npublic class BlogApplication {}\n",
        )
        .unwrap();

        assert_eq!(base_package(&root).unwrap(), "com.example.blog");
    }

    #[test]
    fn base_package_errors_without_an_application_class() {
        let root = scratch("no-application");
        fs::create_dir_all(root.join("src/main/java")).unwrap();
        assert!(base_package(&root).is_err());
    }
}
