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
use repository::*;

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
    for (artifact_id, _) in &change.plugins {
        if !said.insert(artifact_id.to_string()) {
            continue;
        }
        match *artifact_id {
            crate::spring::FAILSAFE_ARTIFACT => println!(
                "      Arrange yourself: your build must run `*IT` classes. Maven needs \
                 Failsafe for that; whatever {tool} calls it, an integration test nothing \
                 executes is a green build that proves nothing."
            ),
            other => println!("      Arrange yourself: what `{other}` does for a Maven build."),
        }
    }
}

/// Spring-only generator kinds refuse politely rather than writing code that
/// cannot compile.
fn require_spring_project(project: &Project, kind: &str) -> Result<()> {
    crate::spring::require_spring(project.flavor(), kind)
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
        ArtifactKind::Dto | ArtifactKind::Scaffold => change
            .deps
            .push(*crate::spring::validation_dependency(project.flavor())),
        ArtifactKind::Client => change.deps.push(crate::spring::RESTCLIENT_STARTER),
        ArtifactKind::Fetcher => change.deps.extend([
            crate::spring::APACHE_HTTPCLIENT,
            crate::spring::ACTUATOR_STARTER,
        ]),
        ArtifactKind::Event => change.deps.extend([
            crate::spring::TESTCONTAINERS_KAFKA,
            crate::spring::SPRING_TESTCONTAINERS,
        ]),
        _ => {}
    }
    if writes_an_it(&change.files) {
        change.plugins.push((
            crate::spring::FAILSAFE_ARTIFACT,
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

// ---------------------------------------------------------------------------
// Names, per kind
// ---------------------------------------------------------------------------
//
// These are about `ArtifactKind`, not about a field, so they stayed at this
// layer when the field spec moved down to `crate::spec`.

/// The suffix each kind appends to the name it is given.
///
/// `None` for kinds that use the name verbatim (`record`, `enum`, `scaffold`
/// -- which spans several suffixes and cannot have one of them stripped).
pub(crate) fn kind_suffix(kind: ArtifactKind) -> Option<&'static str> {
    match kind {
        ArtifactKind::Controller => Some("Controller"),
        ArtifactKind::Service => Some("Service"),
        ArtifactKind::Repo => Some("Repository"),
        ArtifactKind::Cli => Some("Cli"),
        ArtifactKind::Job => Some("Job"),
        ArtifactKind::DurableJob => Some("Job"),
        ArtifactKind::HttpWorkflow => Some("Workflow"),
        ArtifactKind::HttpSink => None,
        ArtifactKind::Client => Some("Client"),
        ArtifactKind::Fetcher => Some("Fetcher"),
        ArtifactKind::Usecase => Some("UseCase"),
        ArtifactKind::Query => Some("Query"),
        ArtifactKind::Test => Some("Test"),
        ArtifactKind::IntegrationTest => Some("IT"),

        // Exhaustive, deliberately. This match ended in `_ => None` for a long
        // time, and `pending.md` §6.4 is the entry about what that cost: a kind
        // added to the enum got no suffix and nothing said so, and
        // `recorded_name` and `strip_redundant_suffix` -- which both read this
        // -- inherited the silence. A kind whose name is not normalised is a
        // kind whose `destroy` rebuilds different paths from the ones
        // `generate` wrote.
        //
        // So the kinds that genuinely add nothing are listed. Adding a variant
        // to `ArtifactKind` now fails to compile until somebody decides which
        // half it is in, which is the whole point.
        ArtifactKind::Scaffold
        | ArtifactKind::Class
        | ArtifactKind::Interface
        | ArtifactKind::Record
        | ArtifactKind::Field
        | ArtifactKind::Factory
        | ArtifactKind::Value
        | ArtifactKind::Enum
        | ArtifactKind::Sealed
        | ArtifactKind::Strategy
        | ArtifactKind::Migration
        | ArtifactKind::Handler
        | ArtifactKind::Command
        | ArtifactKind::Cases
        | ArtifactKind::Association
        | ArtifactKind::Idempotency
        | ArtifactKind::Auth
        | ArtifactKind::Webhook
        | ArtifactKind::Search
        | ArtifactKind::Dto
        | ArtifactKind::Transition
        | ArtifactKind::Event => None,
    }
}

/// Drop the suffix a kind is about to add, when the name already carries it.
///
/// `jails g service RewardHistoryService` should write
/// `RewardHistoryService.java`, not `RewardHistoryServiceService.java`. Naming
/// the type the way it will appear in the source is the obvious thing to type
/// -- it is what the file is called, and what every other reference to it
/// says -- and jails punished it with a rename.
///
/// Only a *whole* trailing suffix counts, and never the entire name: `g
/// service Service` means a type called `Service`, and stripping it would
/// leave nothing to name the file after. `g repo Rewards` keeps its `s`
/// because `Repository` is matched, not `y`.
///
/// **This has to run in `destroy` too.** `destroy` rebuilds the paths that
/// `generate` wrote, so a normalisation applied to one and not the other
/// leaves files behind that the tool then claims to have deleted.
/// The name a recipe is **recorded** under, which is not always the name the
/// caller typed.
///
/// `generate` normalises before it writes -- capitalised, and with a suffix the
/// kind already implies removed -- and records the files it wrote under the
/// result. `app apply` records the manifest's *spec* onto the same row, so it
/// has to normalise identically or the two writers key one entity two ways.
///
/// That is exactly what `fetcher AcquirerFetcher` was: a row named `Acquirer`
/// holding four files and no spec, beside a row named `AcquirerFetcher` holding
/// a spec and no files. `doctor` then reported the empty half as an entity with
/// "0 file(s) ... and no recorded owner" and offered an adopt command for
/// nothing.
///
/// `cases` and `migration` are addressed by a path rather than by a class, so
/// they pass through untouched -- the exemption `generate` already makes by
/// returning before it normalises.
pub fn recorded_name(kind: ArtifactKind, name: &str) -> String {
    if matches!(kind, ArtifactKind::Cases | ArtifactKind::Migration) {
        return name.to_string();
    }
    strip_redundant_suffix(kind, &capitalize(name))
}

pub fn strip_redundant_suffix(kind: ArtifactKind, name: &str) -> String {
    match kind_suffix(kind) {
        Some(suffix) => match name.strip_suffix(suffix) {
            Some(stem) if !stem.is_empty() => stem.to_string(),
            _ => name.to_string(),
        },
        None => name.to_string(),
    }
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
    /// The HTTP verb an endpoint answers, when the recipe has one.
    ///
    /// `None` is "not asked", never "GET": the default belongs at the one
    /// place that renders a mapping annotation, not spread across every
    /// caller that has no endpoint to describe.
    pub method: Option<jails_spec::spec::kind::HttpMethod>,
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

    /// The invariant that keeps a scaffold able to *start*: exactly one
    /// adapter is a bean. Two makes Spring refuse to choose; zero leaves the
    /// service with no repository at all.
    #[test]
    fn exactly_one_repository_adapter_carries_the_bean_annotation() {
        let columns = crate::sql::columns(
            &parse_fields(&["id:string!".to_string(), "title:string".to_string()]).unwrap(),
            &crate::model::Project::inspect(Path::new("/tmp/does-not-matter")).unwrap(),
            "com.example.app.domain",
            "note",
        );
        let jdbc_bean = jdbc_client_repository(
            "com.example.app.adapters",
            "Note",
            "",
            &columns,
            "com.example.app.domain",
        );
        let in_memory_fake = crate::spring::in_memory_repository_java(
            "com.example.app.adapters",
            "Note",
            "",
            Some("id"),
            false,
        );
        // The annotation on the declaration, not the word in the Javadoc.
        assert!(
            jdbc_bean.contains("@Component\npublic final class"),
            "{jdbc_bean}"
        );
        assert!(
            !in_memory_fake.contains("@Component\npublic class"),
            "the JDBC adapter is the bean here, so this one must not be: {in_memory_fake}"
        );
        assert!(
            !in_memory_fake.contains("import org.springframework.stereotype.Component;"),
            "an unused import would fail a strict build: {in_memory_fake}"
        );

        // ...and the other way round, before `add db` has run.
        let in_memory_bean = crate::spring::in_memory_repository_java(
            "com.example.app.adapters",
            "Note",
            "",
            Some("id"),
            true,
        );
        assert!(
            in_memory_bean.contains("@Component\npublic class"),
            "{in_memory_bean}"
        );
    }

    /// `spring.md` calls a positional `?` list in a multi-column insert a
    /// silent-swap bug waiting for a schema change, and the generator used to
    /// emit exactly that.
    #[test]
    fn the_spring_adapter_binds_by_name_and_shares_one_column_list() {
        let columns = crate::sql::columns(
            &parse_fields(&[
                "id:uuid".to_string(),
                "amount:long".to_string(),
                "currency:string".to_string(),
            ])
            .unwrap(),
            &crate::model::Project::inspect(Path::new("/tmp/does-not-matter")).unwrap(),
            "com.example.app.domain",
            "reward",
        );
        let src = jdbc_client_repository(
            "com.example.app.adapters",
            "Reward",
            "",
            &columns,
            "com.example.app.domain",
        );
        assert!(src.contains("JdbcClient"), "{src}");
        assert!(!src.contains("PreparedStatement"), "{src}");
        // Named, not positional.
        assert!(src.contains(".param(\"amount\""), "{src}");
        assert!(src.contains(":amount"), "{src}");
        assert!(!src.contains("setObject("), "{src}");
        // One column list, interpolated into the reads.
        assert!(src.contains("private static final String COLUMNS"), "{src}");
        assert!(src.contains(".formatted(COLUMNS)"), "{src}");
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
    fn field_type_maps_known_tokens() {
        assert_eq!(field_type("string").unwrap().0, "String");
        assert_eq!(field_type("text").unwrap(), ("String", None));
        assert_eq!(field_type("int").unwrap().0, "Integer");
        assert_eq!(field_type("integer").unwrap().0, "Integer");
        assert_eq!(field_type("long").unwrap().0, "Long");
        assert_eq!(field_type("boolean").unwrap().0, "Boolean");
        assert_eq!(field_type("double").unwrap().0, "Double");
        assert_eq!(
            field_type("uuid").unwrap(),
            ("UUID", Some("java.util.UUID"))
        );
        assert_eq!(
            field_type("currency").unwrap(),
            ("Currency", Some("java.util.Currency"))
        );
        assert_eq!(
            field_type("date").unwrap(),
            ("LocalDate", Some("java.time.LocalDate"))
        );
        assert_eq!(
            field_type("datetime").unwrap(),
            ("LocalDateTime", Some("java.time.LocalDateTime"))
        );
    }

    #[test]
    fn field_type_rejects_unknown_tokens() {
        assert!(field_type("nope").is_err());
    }

    #[test]
    fn column_markers_parse_in_any_order_and_combine() {
        let fields = parse_fields(&[
            "transactionId:uuid@pk".to_string(),
            "amount:long@positive@index".to_string(),
            "email:string!@unique".to_string(),
            "workspaceId:uuid@scope@index".to_string(),
        ])
        .unwrap();
        assert!(fields[0].constraints.primary_key);
        assert_eq!(fields[1].constraints.check, Some(NumericCheck::Positive));
        assert!(fields[1].constraints.indexed);
        assert!(fields[2].constraints.unique);
        assert!(fields[3].constraints.scoped);
        assert!(fields[3].constraints.indexed);
        // The markers do not disturb the type or the optionality suffix.
        assert_eq!(fields[0].java_type, "UUID");
        assert_eq!(fields[2].java_type, "String");
        assert_eq!(fields[2].optionality, Optionality::NonBlank);
    }

    /// A marker typo that parsed as "no constraint" would produce a schema
    /// quietly missing the primary key someone thought they had asked for --
    /// the exact failure this feature exists to prevent.
    #[test]
    fn an_unknown_column_marker_is_an_error_listing_the_real_ones() {
        let err = parse_fields(&["id:uuid@primary".to_string()]).unwrap_err();
        assert!(err.contains("@primary"), "{err}");
        assert!(err.contains("@pk"), "{err}");
    }

    /// `check (name > 0)` on a text column fails at `flyway migrate`, which is
    /// a slow and remote way to learn about a typo.
    #[test]
    fn a_numeric_check_on_a_non_numeric_column_is_rejected() {
        let err = parse_fields(&["name:string@positive".to_string()]).unwrap_err();
        assert!(err.contains("numeric"), "{err}");
        assert!(parse_fields(&["amount:long@positive".to_string()]).is_ok());
        assert!(parse_fields(&["price:decimal@nonnegative".to_string()]).is_ok());
    }

    #[test]
    fn a_nullable_primary_key_is_rejected() {
        let err = parse_fields(&["id:uuid?@pk".to_string()]).unwrap_err();
        assert!(err.contains("nullable"), "{err}");
    }

    #[test]
    fn a_field_with_no_markers_has_no_constraints() {
        let fields = parse_fields(&["title:string".to_string()]).unwrap();
        assert_eq!(fields[0].constraints, Constraints::default());
    }

    #[test]
    fn parse_fields_splits_name_and_type() {
        let fields = parse_fields(&["title:string".to_string(), "body:Text".to_string()]).unwrap();
        assert_eq!(fields[0].name, "title");
        assert_eq!(fields[0].java_type, "String");
        // Capitalised means "a type this project owns", so `Text` is no longer
        // the built-in -- that is the whole point of the rule.
        assert_eq!(fields[1].java_type, "Text");
        assert!(fields[1].owned);
        assert_eq!(
            parse_fields(&["body:text".to_string()]).unwrap()[0].java_type,
            "String"
        );
    }

    /// The Java spellings of the built-in types stay built-in: `id:String`
    /// must not be read as an unknown project type.
    #[test]
    fn parse_fields_treats_java_type_names_as_builtins() {
        let fields = parse_fields(&["id:String".to_string(), "on:LocalDate".to_string()]).unwrap();
        assert!(!fields[0].owned);
        assert_eq!(fields[0].java_type, "String");
        assert!(!fields[1].owned);
        assert!(fields[1].imports.contains(&"java.time.LocalDate"));
    }

    #[test]
    fn resource_path_is_kebab_case_and_plural() {
        assert_eq!(resource_path("WorkItem"), "/work-items");
        assert_eq!(resource_path("Import"), "/imports");
    }

    /// A handler binds, routes and maps outcomes to status codes -- and holds
    /// no rules, so the same service can be driven from the CLI.
    #[test]
    fn handler_maps_outcomes_to_status_codes() {
        let src = handler_java("com.example.demo.api", "WorkItem", "");

        assert!(src.contains("implements HttpHandler"), "{src}");
        assert!(src.contains(r#"PATH = "/work-items""#), "{src}");
        assert!(
            src.contains("private final Service service"),
            "the service is a dependency: {src}"
        );
        assert!(src.contains("error(404"), "{src}");
        assert!(
            src.contains("error(422"),
            "well-formed but rejected is not a 400: {src}"
        );
        assert!(
            src.contains("ApiError"),
            "failures share one envelope: {src}"
        );
        assert!(!src.contains("java.sql"), "no storage in a handler: {src}");
    }

    #[test]
    fn handler_test_drives_it_over_a_real_socket() {
        let test = handler_test("com.example.demo.api", "WorkItem");

        assert!(test.contains("java.net.http.HttpClient"), "{test}");
        assert!(
            test.contains("new InetSocketAddress(0)"),
            "an ephemeral port: {test}"
        );
        assert!(test.contains("isEqualTo(422)"), "{test}");
    }

    /// The whole point of a port: application code must be able to depend on
    /// it without dragging JDBC along -- including in the prose, since a
    /// reader grepping for java.sql should find only the adapter.
    #[test]
    fn repository_port_is_free_of_jdbc() {
        let src = repository_port(
            "com.example.demo.app",
            "Transaction",
            "import com.example.demo.domain.Transaction;\n",
        );

        assert!(
            src.contains("public interface TransactionRepository"),
            "{src}"
        );
        assert!(
            src.contains("Optional<Transaction> findById(String id)"),
            "{src}"
        );
        assert!(src.contains("List<Transaction> findAll()"), "{src}");
        assert!(!src.contains("java.sql"), "not even in a comment: {src}");
    }

    #[test]
    fn jdbc_adapter_uses_plain_jdbc_and_no_orm() {
        let src = jdbc_repository(
            "com.example.demo.adapters",
            "Transaction",
            "",
            &[],
            "com.example.demo.domain",
        );

        assert!(src.contains("implements TransactionRepository"), "{src}");
        assert!(src.contains("connection.prepareStatement"), "{src}");
        assert!(src.contains("try (var query"), "try-with-resources: {src}");
        assert!(
            src.contains("order by id"),
            "unordered findAll would flake a test: {src}"
        );
        assert!(
            src.contains("\"\"\""),
            "SQL should be visible in text blocks: {src}"
        );
        {
            let forbidden = "org.springframework";
            assert!(!src.contains(forbidden), "{forbidden} should not appear");
        }
    }

    /// jails cannot know the columns, so map/bind are TODOs -- and a test that
    /// asserts on a TODO is noise until they are written.
    #[test]
    fn jdbc_adapter_test_is_disabled_until_the_mapping_is_written() {
        let test = jdbc_repository_test("com.example.demo.adapters", "Transaction");

        assert!(test.contains("@Disabled"), "{test}");
        assert!(test.contains("class JdbcTransactionRepositoryIT"), "{test}");
        assert!(test.contains("roundTripsThroughTheRealDatabase"), "{test}");
    }

    #[test]
    fn sealed_emits_a_permits_clause_and_a_record_per_variant() {
        let variants = parse_variants(&["verified".to_string(), "timeout".to_string()]).unwrap();
        let src = sealed_java("com.example.demo", "VerificationResult", &variants);

        // Nested variants have to be named qualified in the permits clause.
        assert!(
            src.contains("permits VerificationResult.Verified, VerificationResult.Timeout"),
            "{src}"
        );
        assert!(
            src.contains("record Verified() implements VerificationResult"),
            "{src}"
        );
        assert!(
            src.contains("record Timeout() implements VerificationResult"),
            "{src}"
        );
    }

    /// The companion test switches without a `default`, so adding a variant
    /// breaks it at compile time -- which is the entire reason to seal a type.
    #[test]
    fn sealed_test_switches_exhaustively_without_a_default() {
        let variants = parse_variants(&["ok".to_string(), "failed".to_string()]).unwrap();
        let test = sealed_test("com.example.demo", "Result", &variants);

        assert!(test.contains("switch (result)"), "{test}");
        assert!(test.contains("case Result.Ok v ->"), "{test}");
        assert!(
            !test.contains("default ->"),
            "an exhaustive switch must not have a default: {test}"
        );
    }

    /// Typing the name the class will actually have is the obvious thing to
    /// do, and `g service RewardHistoryService` writing
    /// `RewardHistoryServiceService.java` is the bug that taught jails not to
    /// punish it. The same rule applies to a strategy's variants.
    #[test]
    fn a_strategy_variant_does_not_repeat_the_interface_name() {
        assert_eq!(strategy_class("Coffee", "RewardRule"), "CoffeeRewardRule");
        assert_eq!(
            strategy_class("CoffeeRewardRule", "RewardRule"),
            "CoffeeRewardRule"
        );
        // Never the whole name away: `g strategy Rule Rule` means a class
        // called `Rule`, not the empty string.
        assert_eq!(strategy_class("RewardRule", "RewardRule"), "RewardRule");
    }

    /// `--yields` is what decides the shape: with it the strategy answers
    /// "what does this earn?" and declines with an empty `Optional`, which is
    /// what lets every implementation see every input. Without it it is a
    /// predicate.
    #[test]
    fn a_strategy_yields_an_optional_and_a_bare_one_is_a_predicate() {
        let (ret, method, param) = strategy_method("Transaction", Some("Reward"));
        assert_eq!(ret, "Optional<Reward>");
        assert_eq!(method, "apply");
        assert_eq!(param, "Transaction transaction");

        let (ret, method, _) = strategy_method("Transaction", None);
        assert_eq!(ret, "boolean");
        assert_eq!(method, "matches");
    }

    /// The annotation is the whole reason the pattern works, and its absence
    /// is silent: without it the class is simply not in the `List<Port>`, so
    /// it never runs and nothing reports a problem. The generated Javadoc
    /// says so, because that is the only place a reader will find it.
    #[test]
    fn a_spring_strategy_implementation_is_a_bean_and_says_why() {
        let spring = strategy_impl_java(
            "com.example.demo.domain",
            "RewardRule",
            "CoffeeRewardRule",
            "Transaction",
            Some("Reward"),
            true,
        );
        assert!(spring.contains("@Component"), "{spring}");
        assert!(
            spring.contains("import org.springframework.stereotype.Component;"),
            "{spring}"
        );
        assert!(spring.contains("its absence is silent"), "{spring}");

        // A plain Maven project has no Spring on the classpath, so the
        // annotation would not resolve and the import would not compile.
        let plain = strategy_impl_java(
            "com.example.demo.domain",
            "RewardRule",
            "CoffeeRewardRule",
            "Transaction",
            Some("Reward"),
            false,
        );
        assert!(!plain.contains("@Component"), "{plain}");
        assert!(!plain.contains("springframework"), "{plain}");
    }

    /// `apply` + `s` reads `applys`. A generated test whose name is
    /// misspelled is the first thing anyone sees of the pattern.
    #[test]
    fn generated_strategy_test_names_are_english() {
        let yielding = strategy_impl_test(
            "d",
            "RewardRule",
            "CoffeeRewardRule",
            "Transaction",
            Some("Reward"),
        );
        assert!(
            yielding.contains("void grantsWhenTheTransactionQualifies()"),
            "{yielding}"
        );
        assert!(!yielding.contains("applys"), "{yielding}");

        let predicate =
            strategy_impl_test("d", "RewardRule", "CoffeeRewardRule", "Transaction", None);
        assert!(
            predicate.contains("void matchesWhenTheTransactionQualifies()"),
            "{predicate}"
        );

        // @Disabled, not a passing assertion over an unwritten class: it is
        // reported as skipped rather than counted green.
        assert!(yielding.contains("@Disabled"), "{yielding}");
    }

    #[test]
    fn parse_variants_rejects_unusable_names() {
        assert!(parse_variants(&[]).is_err());
        assert!(
            parse_variants(&["ok".to_string(), "Ok".to_string()]).is_err(),
            "duplicate after capitalising"
        );
        assert!(parse_variants(&["not a name".to_string()]).is_err());
    }

    #[test]
    fn a_generated_zero_component_sealed_variant_is_a_complete_sample() {
        let root = scratch("sealed-sample");
        let pkg = "com.example.demo.domain";
        let main = main_dir(&root, pkg);
        fs::create_dir_all(&main).unwrap();
        fs::write(
            main.join("Outcome.java"),
            sealed_java(pkg, "Outcome", &["Accepted".into(), "Rejected".into()]),
        )
        .unwrap();
        let field = parse_fields(&["result:Outcome".to_string()])
            .unwrap()
            .remove(0);

        let project = crate::model::Project::inspect(&root).unwrap();
        let (sample, imports) = sample_in_package(&field, &project, pkg).unwrap();

        assert_eq!(sample, "new Outcome.Accepted()");
        assert!(imports.is_empty());
    }

    #[test]
    fn parse_fields_resolves_collection_types() {
        let fields = parse_fields(&[
            "matched:list<Match>".to_string(),
            "ids:list<string>".to_string(),
            "rates:map<string,double>".to_string(),
            "at:instant".to_string(),
        ])
        .unwrap();

        assert_eq!(fields[0].java_type, "List<Match>");
        assert!(fields[0].collection);
        assert_eq!(fields[1].java_type, "List<String>");
        // Generics cannot hold a primitive, so the element is the wrapper.
        assert_eq!(fields[2].java_type, "Map<String, Double>");
        assert!(fields[2].imports.contains(&"java.util.Map"));
        assert_eq!(fields[3].java_type, "Instant");
        assert!(fields[3].imports.contains(&"java.time.Instant"));
    }

    #[test]
    fn parse_fields_rejects_malformed_collection_types() {
        // A bare `list` would otherwise become List<Object>, silently.
        assert!(parse_fields(&["items:list".to_string()]).is_err());
        assert!(parse_fields(&["items:list<nope>".to_string()]).is_err());
        assert!(parse_fields(&["items:map<string>".to_string()]).is_err());
        assert!(parse_fields(&["items:list<list<string>>".to_string()]).is_err());
        // A collection already models absence; `?` on one is a mistake.
        assert!(parse_fields(&["items:list<string>?".to_string()]).is_err());
    }

    /// A collection component must be copied (so the record is genuinely
    /// immutable) and default to empty (so no consumer has to null-check a
    /// bucket).
    #[test]
    fn collection_components_are_copied_and_default_to_empty() {
        let fields = parse_fields(&[
            "matched:list<Match>".to_string(),
            "rates:map<string,double>".to_string(),
        ])
        .unwrap();
        let src = value_java("com.example.demo", "Result", &fields);

        assert!(src.contains("List<Match> matched"), "{src}");
        assert!(
            src.contains("matched = matched == null ? List.of() : List.copyOf(matched);"),
            "{src}"
        );
        assert!(
            src.contains("rates = rates == null ? Map.of() : Map.copyOf(rates);"),
            "{src}"
        );
        assert!(
            !src.contains("requireNonNull(matched"),
            "a collection is defaulted, not rejected: {src}"
        );
    }

    #[test]
    fn parse_fields_reads_the_optionality_suffixes() {
        let fields = parse_fields(&[
            "id:string!".to_string(),
            "note:string?".to_string(),
            "name:string".to_string(),
            "source:SourceRef?".to_string(),
        ])
        .unwrap();
        assert_eq!(fields[0].optionality, Optionality::NonBlank);
        assert_eq!(fields[1].optionality, Optionality::Nullable);
        assert_eq!(fields[2].optionality, Optionality::Required);
        assert_eq!(fields[3].optionality, Optionality::Nullable);
        assert!(fields[3].owned);
        assert_eq!(fields[3].java_type, "SourceRef");
    }

    #[test]
    fn parse_fields_rejects_args_without_a_colon() {
        assert!(parse_fields(&["title".to_string()]).is_err());
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

    #[test]
    fn stub_class_emits_a_plain_final_class_with_no_framework_in_it() {
        let src = stub_class("gym", "MoneyMoved");

        assert_eq!(
            src, "package gym;\n\npublic final class MoneyMoved {\n}\n",
            "{src}"
        );
        for forbidden in ["@", "org.springframework", "record "] {
            assert!(
                !src.contains(forbidden),
                "{forbidden} should not appear in a plain class"
            );
        }
    }

    /// The companion test has to compile against the class jails just wrote,
    /// which means constructing it with the implicit no-arg constructor -- the
    /// only one a bare class has.
    #[test]
    fn class_test_constructs_the_class_it_accompanies() {
        let src = class_test("gym", "MoneyMoved");

        assert!(src.contains("class MoneyMovedTest {"), "{src}");
        assert!(
            src.contains("MoneyMoved moneyMoved = new MoneyMoved();"),
            "{src}"
        );
        assert!(src.contains("import org.junit.jupiter.api.Test;"), "{src}");
        // The three defects of the old `isNotNull()` body: it passed while
        // the class was broken, it counted as coverage, and it taught `null`
        // as a constructor argument.
        assert!(
            !src.contains("isNotNull"),
            "a test that passes over a broken class is worse than no test: {src}"
        );
        assert!(src.contains("@Disabled("), "{src}");
        assert!(
            src.contains("todo: state what MoneyMoved is supposed to do"),
            "the disabled reason has to say what to prove: {src}"
        );
    }

    #[test]
    fn record_java_emits_a_record_with_a_null_rejecting_compact_constructor() {
        let fields =
            parse_fields(&["amount:long".to_string(), "currency:string".to_string()]).unwrap();
        let src = record_java("com.example.demo", "Money", &fields);

        // Primitive components make null impossible for numeric/boolean values: a
        // `long` cannot be null, so it needs neither the box nor the check.
        assert!(
            src.contains("public record Money(long amount, String currency) {"),
            "{src}"
        );
        assert!(
            src.contains("public Money {"),
            "expected a compact constructor"
        );
        assert!(
            !src.contains("requireNonNull(amount"),
            "a primitive cannot be null"
        );
        assert!(src.contains(r#"Objects.requireNonNull(currency, "currency");"#));
        // Plain Java: no framework persistence annotations.
        for forbidden in ["@", "org.springframework"] {
            assert!(
                !src.contains(forbidden),
                "{forbidden} should not appear in a plain record"
            );
        }
    }

    /// A record whose components are all primitives cannot hold a null, so the
    /// compact constructor would be empty -- and an empty one is noise.
    #[test]
    fn record_java_omits_the_compact_constructor_when_every_component_is_primitive() {
        let fields = parse_fields(&["amount:long".to_string(), "count:int".to_string()]).unwrap();
        let src = record_java("com.example.demo", "Tally", &fields);

        assert!(
            src.contains("public record Tally(long amount, int count) {"),
            "{src}"
        );
        assert!(
            !src.contains("public Tally {"),
            "nothing to validate: {src}"
        );
        assert!(!src.contains("import java.util.Objects;"));
    }

    /// A no-field record has nothing to null-check, so the compact constructor
    /// (and the Objects import that only exists to serve it) must be omitted
    /// rather than emitted empty.
    #[test]
    fn record_java_omits_the_compact_constructor_when_there_are_no_fields() {
        let src = record_java("com.example.demo", "Marker", &[]);

        assert!(src.contains("public record Marker() {"));
        assert!(!src.contains("public Marker {"));
        assert!(!src.contains("import java.util.Objects;"));
    }

    #[test]
    fn record_java_sorts_time_imports_with_the_objects_import() {
        let fields = parse_fields(&["on:date".to_string()]).unwrap();
        let src = record_java("com.example.demo", "Entry", &fields);

        let time = src.find("import java.time.LocalDate;").unwrap();
        let objects = src.find("import java.util.Objects;").unwrap();
        assert!(time < objects, "java.time should sort before java.util");
    }

    /// The compact constructor's validation is real behaviour and can
    /// regress. An accessor round-trip cannot: it asserts that javac
    /// generated an accessor, which `java.md` §7 names as a thing not to
    /// test.
    #[test]
    fn record_test_pins_the_validation_and_not_the_accessors() {
        let fields =
            parse_fields(&["amount:long".to_string(), "currency:string".to_string()]).unwrap();
        let test = record_test(
            &crate::model::Project::inspect(Path::new("/tmp/does-not-matter")).unwrap(),
            "com.example.demo",
            "Money",
            &fields,
        );

        assert!(test.contains("class MoneyTest"));
        assert!(test.contains("assertThatNullPointerException()"));
        // `amount` is a primitive, so the null case has to target the first
        // *reference* component or the generated test would not compile.
        assert!(test.contains("new Money(1L, null)"), "{test}");

        assert!(
            !test.contains("accessorsReturnWhatWasConstructed"),
            "{test}"
        );
        assert!(
            !test.contains("assertThat(money.amount()).isEqualTo(1L);"),
            "testing the compiler: {test}"
        );
    }

    /// A record with nothing to validate has nothing honest to assert, so it
    /// says so rather than emitting a green tick over an unproven type.
    #[test]
    fn a_record_with_no_validation_gets_a_disabled_todo_rather_than_a_tick() {
        let fields = parse_fields(&["amount:long".to_string()]).unwrap();
        let test = record_test(
            &crate::model::Project::inspect(Path::new("/tmp/does-not-matter")).unwrap(),
            "com.example.demo",
            "Money",
            &fields,
        );
        assert!(test.contains("@Disabled("), "{test}");
        assert!(test.contains("todo: state what Money guarantees"), "{test}");
        assert!(
            test.contains("import org.junit.jupiter.api.Disabled;"),
            "{test}"
        );
        assert!(!test.contains("assertThatNullPointerException"), "{test}");
    }

    /// With no fields there is no null to reject, so the test that asserts the
    /// rejection would not compile -- it must not be emitted.
    #[test]
    fn record_test_skips_the_null_case_for_a_no_field_record() {
        let test = record_test(
            &crate::model::Project::inspect(Path::new("/tmp/does-not-matter")).unwrap(),
            "com.example.demo",
            "Marker",
            &[],
        );

        assert!(!test.contains("assertThatNullPointerException"));
        assert!(!test.contains(
            "import static org.assertj.core.api.Assertions.assertThatNullPointerException;"
        ));
        assert!(test.contains("new Marker()"));
    }

    #[test]
    fn command_java_returns_an_exit_code_and_never_exits_the_process() {
        let src = command_java("com.example.demo", "Greet");

        assert!(src.contains("public final class GreetCommand"));
        assert!(src.contains(r#"public static final String NAME = "greet";"#));
        assert!(
            src.contains("public static int run(PrintStream out, PrintStream err, String... args)")
        );
        // A CLI command has no business depending on Spring.
        assert!(!src.contains("org.springframework"));

        // The whole point: main owns the exit, so the command stays testable
        // in-process, and output goes to injected streams, not System.out.
        // Only the class body is checked -- the Javadoc deliberately shows a
        // `main` that does call System.exit, since that is where it belongs.
        let body = &src[src.find("public final class").unwrap()..];
        assert!(
            !body.contains("System.exit"),
            "run() must not exit the process"
        );
        assert!(
            !body.contains("System.out"),
            "output should go to the injected stream"
        );
    }

    #[test]
    fn command_test_drives_the_command_through_captured_streams() {
        let test = command_test("com.example.demo", "Greet");

        assert!(test.contains("class GreetCommandTest"));
        assert!(test.contains("ByteArrayOutputStream"));
        assert!(
            test.contains("GreetCommand.run(new PrintStream(out), new PrintStream(err), args)")
        );
        assert!(test.contains("GreetCommand.USAGE_ERROR"));
    }

    /// The default endpoint: what `g controller Post` emits with no flags.
    fn plain_endpoint() -> crate::generate::web::Endpoint<'static> {
        crate::generate::web::Endpoint {
            method: jails_spec::spec::kind::HttpMethod::Get,
            returns: None,
            accepts: None,
            extra: String::new(),
        }
    }

    /// Every verb reaches the annotation, the handler name and the test that
    /// calls it -- and the three agree, which is the whole reason
    /// `web::Endpoint` is one value rather than three derivations.
    #[test]
    fn a_controller_answers_the_method_it_was_asked_for() {
        use jails_spec::spec::kind::HttpMethod;
        for (method, mapping) in [
            (HttpMethod::Post, "PostMapping"),
            (HttpMethod::Put, "PutMapping"),
            (HttpMethod::Patch, "PatchMapping"),
            (HttpMethod::Delete, "DeleteMapping"),
        ] {
            let endpoint = crate::generate::web::Endpoint {
                method,
                ..plain_endpoint()
            };
            let source = stub_controller("com.example.blog", "Post", &endpoint);
            assert!(
                source.contains(&format!("@{mapping}(\"/post\")")),
                "{source}"
            );
            assert!(
                source.contains(&format!(
                    "import org.springframework.web.bind.annotation.{mapping};"
                )),
                "{source}"
            );
            let test = controller_stub_test("com.example.blog", "Post", "x.Y", &endpoint, 4);
            assert!(
                test.contains(&format!("mvc.{}().uri(\"/post\")", method.label())),
                "{test}"
            );
        }
    }

    /// A verb that carries no body must not be given a `@RequestBody`
    /// parameter: it is not forbidden by HTTP and it never binds, so a
    /// parameter there would be a silent nothing.
    #[test]
    fn only_a_verb_with_a_body_takes_a_request_body() {
        use jails_spec::spec::kind::HttpMethod;
        for (method, expected) in [(HttpMethod::Post, true), (HttpMethod::Get, false)] {
            let endpoint = crate::generate::web::Endpoint {
                method,
                accepts: Some("Verify"),
                ..plain_endpoint()
            };
            assert_eq!(
                stub_controller("com.example.blog", "Post", &endpoint).contains("@RequestBody"),
                expected,
                "{method:?}"
            );
        }
    }

    /// The `sample_value` rule, applied to a route: jails cannot build a
    /// `Verification`, so the test is emitted whole and `@Disabled` naming
    /// what to do rather than asserting a body jails invented.
    #[test]
    fn a_route_returning_a_project_type_is_tested_but_disabled() {
        let endpoint = crate::generate::web::Endpoint {
            returns: Some("Verification"),
            ..plain_endpoint()
        };
        let test = controller_stub_test("com.example.blog", "Post", "x.Y", &endpoint, 4);
        assert!(test.contains("@Disabled"), "{test}");
        assert!(
            test.contains("import org.junit.jupiter.api.Disabled;"),
            "{test}"
        );
        assert!(!test.contains("bodyText()"), "{test}");

        let plain = controller_stub_test("com.example.blog", "Post", "x.Y", &plain_endpoint(), 4);
        assert!(!plain.contains("@Disabled"), "{plain}");
        assert!(plain.contains("bodyText()"), "{plain}");
    }

    #[test]
    fn stub_templates_use_the_package_and_class_name() {
        assert!(
            stub_controller("com.example.blog", "Post", &plain_endpoint())
                .contains("class PostController")
        );
        // Package-private: Spring wires these by reflection, so `public` only
        // widens what other packages can compile against.
        assert!(
            stub_service("com.example.blog", "Post").contains("\n@Component\nclass PostService")
        );
        assert!(
            !stub_service("com.example.blog", "Post").contains("public class"),
            "spring.md §2: public only where the type is module API"
        );
        assert!(
            !stub_controller("com.example.blog", "Post", &plain_endpoint())
                .contains("public class"),
            "spring.md §2: public only where the type is module API"
        );
        assert!(
            interface_java("com.example.blog", "PostStore").contains("public interface PostStore")
        );
        assert!(stub_test("com.example.blog", "Post").contains("class PostTest"));
    }

    /// The shape `is_dispatcher` looks for, which is what `new-cli` writes.
    fn dispatcher_java() -> &'static str {
        "package com.example.demo;\n\
         \n\
         import java.util.LinkedHashMap;\n\
         import java.util.SequencedMap;\n\
         \n\
         public class App {\n\
         \x20   static SequencedMap<String, Command> commands() {\n\
         \x20       SequencedMap<String, Command> commands = new LinkedHashMap<>();\n\
         \x20       return commands;\n\
         \x20   }\n\
         }\n"
    }

    /// The dispatcher's own Javadoc carries an example `commands.put(...)`
    /// line. Unregistering must not reach into it -- that is documentation,
    /// not a registration.
    #[test]
    fn unsplice_registration_leaves_an_unregistered_command_alone() {
        let source = dispatcher_java();
        assert!(unsplice_registration(source, "GreetCommand").is_none());
    }
}
