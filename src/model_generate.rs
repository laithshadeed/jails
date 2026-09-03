//! The one mutation pipeline, and the familiar generation syntax in front of it.

pub(crate) use jails_model::field_syntax::{ParsedField, normalize_type, parse_field};

mod profile;

mod effects;
pub(crate) mod report;

pub(crate) use effects::{run_follow_up_effects, run_owed_format};
use report::refuse_unconfirmed_deletions;
pub(crate) use report::{report_plan, write_bundle};

pub(crate) use profile::{
    operation_profile, reject_unsupported_operation_options, validate_entity_args,
};

use crate::ArtifactKind;
use crate::cli::GenerateArgs;
use crate::{Invocation, Output};
use jails_contracts::{ModelFileUpdate, PlanInput, ProjectPath};
use jails_model::field_syntax::java_to_label;
use jails_model::{AppModel, Evolution};
use jails_support::{Failure, Result};
use std::path::Path;

/// One model mutation, ready for the pipeline every frontend shares.
///
/// A frontend decides *what* changes -- the edited source and the evolution
/// -- and nothing else: capture, compilation, materialization, the
/// preview and the execution are one computation here, so `--pretend` and
/// the real run cannot describe the transition differently.
pub(crate) struct PreparedMutation {
    pub(crate) name: String,
    pub(crate) invocation: Invocation,
    /// The source and model the mutation starts from.
    pub(crate) current: crate::model_command::Current,
    /// The source after the frontend's edit; equal to `current.source` for a
    /// mutation that declares nothing.
    pub(crate) next_source: String,
    /// What the edited source cannot say: the one-shot policies about how the
    /// accepted schema reaches the next model. The plan records it as its
    /// input; the model itself is whatever `next_source` links to.
    pub(crate) evolution: Evolution,
    /// A migration the *reader* authored, rather than one the compiler
    /// derived from a schema change.
    ///
    /// JDL v1 §2.1 is explicit that ordered migration files are not JDL
    /// -- "immutable, append-only history" -- and §2 lists writing one among
    /// the *non-model* actions a familiar command may map to. So this is not
    /// smuggled into rendering: it joins `PlanDraft.migrations` beside the
    /// derived ones, and the materializer turns it into an ordinary
    /// `AppendMigration` operation with an allocated version and a `Missing`
    /// precondition. It is as visible in the reviewed plan as any other.
    pub(crate) authored_migration: Option<jails_contracts::RenderedMigration>,
    /// Reader-owned files the plan edits beyond what the model implies: the
    /// destination of an ejection, the backfill file a required field is
    /// proved against.
    pub(crate) reader_paths: Vec<ProjectPath>,
}

/// Report a declaration that was already there.
///
/// Every canonical frontend is idempotent, and the ordinary path says so with
/// `0 files written`. A frontend that can tell *before* preparing a patch --
/// `g association`, where re-issuing `AddRelation` fails on the id rather than
/// reconciling -- returns early instead, and says the same thing from here so
/// the sentence lives with the rest of this module's output.
pub(crate) fn report_already_declared(name: &str) {
    println!("{name} is already declared (0 files written)");
}

/// Where the wall clock went, under `--debug`.
///
/// **Named for the pipeline's own steps, because that is what runs**: capture
/// the workspace, apply the patch to the model, compile it, materialize the
/// exact plan, and execute it. A timing list naming steps the binary does
/// not have is worse than none, because it sends the reader looking for the
/// wrong thing.
///
/// `execute` is absent on a preview, and that absence is the point: it is how
/// a reader confirms `--pretend` stopped before the only step that writes.
#[derive(Default)]
struct Stopwatch {
    phases: Vec<(&'static str, std::time::Duration)>,
    since: Option<std::time::Instant>,
}

impl Stopwatch {
    fn start(enabled: bool) -> Self {
        Self {
            phases: Vec::new(),
            since: enabled.then(std::time::Instant::now),
        }
    }

    fn mark(&mut self, phase: &'static str) {
        if let Some(since) = self.since {
            self.phases.push((phase, since.elapsed()));
            self.since = Some(std::time::Instant::now());
        }
    }

    fn report(&self) {
        for (phase, elapsed) in &self.phases {
            println!("  timing  {phase:<12}{:>8.1?}", elapsed);
        }
    }
}

pub(crate) fn finish_generation(prepared: PreparedMutation) -> Result<()> {
    let PreparedMutation {
        name,
        invocation,
        current,
        next_source,
        evolution,
        authored_migration,
        reader_paths,
    } = prepared;
    let model_path = Path::new(crate::model_command::JDL_PATH);
    let mut clock = Stopwatch::start(invocation.debug);
    // The invocation's, so `jails new --app` can replay a manifest into the
    // project it is creating rather than into whatever encloses the directory
    // the reader is standing in.
    let root = invocation.root()?;
    // **The model is what the edited source links to**, decided once here
    // and nowhere else: the frontend wrote the bytes, and the linker says
    // what they mean.
    let next_model = crate::model_command::parse(&next_source)?;
    let mut capture_paths = reader_paths;
    capture_paths.extend(jails_compiler::external_project_paths(&next_model));
    capture_paths.sort();
    capture_paths.dedup();
    let mut snapshot = jails_project::capture::capture(
        &root,
        model_path,
        current.source.as_bytes(),
        current.model,
        Some(&next_model),
        &capture_paths,
        jails_project::capture::ModelFile::Observed,
    )
    .map_err(|error| {
        Failure::diagnosed(error.code, format!("could not capture workspace: {error}"))
    })?;
    clock.mark("capture");
    // **The refusal says the whole request was abandoned.** A command naming
    // several things -- `jails add csv security` -- plans all of them and
    // applies all of them or none, and a reader who is not told that has to
    // work out by hand which half to retry. Nothing has been written at this
    // point by construction: the executor has not run.
    // **The plan records the evolution as its input**, so two mutations that
    // edit the source identically but mean different things -- a rename that
    // preserves the column and one that cuts over -- have different digests.
    let input = PlanInput::evolution(&evolution).map_err(Failure::Told)?;
    let mut draft =
        jails_compiler::Compiler::compile(&snapshot, &next_model, &evolution).map_err(|error| {
            Failure::diagnosed(
                error.code,
                format!("could not compile model change: {error}\n       nothing was written"),
            )
        })?;
    // After the compile, because it is not derived from the model: see
    // `PreparedMutation::authored_migration`. It is still the plan's, not a
    // side effect -- the materializer allocates its version from the observed
    // history and refuses if the path it lands on already exists.
    clock.mark("compile");
    // The one observation the capture could not make: which paths the render
    // wants is known only now, and a reader file already at one of them is a
    // collision the materializer refuses by name.
    jails_project::capture::observe_rendered_paths(
        &root,
        &mut snapshot,
        draft.generated.files.keys(),
    )
    .map_err(|error| {
        Failure::diagnosed(error.code, format!("could not capture workspace: {error}"))
    })?;
    draft.migrations.extend(authored_migration);
    // **What the compiler noticed but would not refuse over.** A warning that
    // stays inside the draft is a warning nobody reads; these are the shapes
    // that compile and run and are probably not what the reader meant, so
    // they belong on the way past rather than in a report they would have to
    // know to ask for.
    // **Under the report, not over it.** The lines are about what the
    // transition produced, so they belong where the reader is already
    // looking when the command has finished, rather than above a file list
    // that has not been printed yet. Kept here because `draft` is consumed
    // by the materializer on the next line.
    let notes = match invocation.output {
        Output::Human => report::notice_lines(&draft.diagnostics),
        _ => Vec::new(),
    };
    let bundle = jails_workspace::materialize(
        &snapshot,
        input,
        draft,
        Some(ModelFileUpdate {
            path: ProjectPath::parse(crate::model_command::JDL_PATH).map_err(Failure::Told)?,
            bytes: next_source.into_bytes(),
            retire: Vec::new(),
        }),
        jails_compiler::COMPILER_VERSION,
        if invocation.force {
            jails_workspace::Restore::EditedAndRemoved
        } else {
            jails_workspace::Restore::Refuse
        },
    )
    .map_err(|error| {
        Failure::diagnosed(
            error.code,
            format!("could not materialize exact plan: {error}"),
        )
    })?;
    clock.mark("materialize");

    if let Some(path) = &invocation.plan_out {
        write_bundle(path, &bundle)?;
    }
    if invocation.pretend || invocation.plan_out.is_some() {
        report_plan(&bundle, &invocation)?;
        for line in &notes {
            println!("{line}");
        }
        if invocation.output == Output::Human {
            clock.report();
        }
        return Ok(());
    }
    if let Some(refusal) = refuse_unconfirmed_deletions(&bundle, &invocation) {
        return refusal;
    }
    let managed = snapshot
        .accepted_projection
        .as_ref()
        .map(|projection| projection.files.keys().cloned().collect())
        .unwrap_or_default();
    let stranded =
        report::stranded_reader_references(&root, &snapshot.model.model, &next_model, &managed);
    // **Said only once the model exists, and only if it does.** Reading the
    // plan for a model file with no before-image says it at the one moment
    // it is true, where announcing it before the mutation would announce a
    // conversion a refused command then abandons. It goes to stderr because
    // stdout is the command's own output and a caller piping it did not ask
    // for this.
    let converted = invocation.output == Output::Human
        && bundle.plan.operations.iter().any(|operation| {
            matches!(
                operation,
                jails_contracts::PlannedOperation::ReplaceModelFile { before: None, .. }
            )
        });
    let execution = jails_workspace::execute(&root, &bundle).map_err(|error| {
        Failure::diagnosed(error.code, format!("could not apply exact plan: {error}"))
    })?;
    clock.mark("execute");
    if converted {
        eprintln!("  create  {}", crate::model_command::JDL_PATH);
        eprintln!(
            "This project is canonical now: `jails g` renders through the compiler into \
             `src/`, and `.jails/compiler.lock.json` says which files are jails'; your \
             own sources stay yours."
        );
    }
    if invocation.output == Output::Human {
        for line in &stranded {
            eprintln!("{line}");
        }
        // **"Nothing happened" and "everything happened and changed nothing"
        // are different answers**, and only the second has files to name. A
        // second `jails add csv` is the first, and a reader who cannot tell
        // them apart cannot tell a no-op from a command that silently did not
        // run.
        if execution.files_written == 0 && execution.files_deleted == 0 {
            println!("{name}: nothing to do, the project already matches the model");
        } else {
            println!(
                "applied model patch for {}: {} ({} files written)",
                name,
                execution.plan_digest.as_str(),
                execution.files_written
            );
            // **Every path, because a count is not an answer.** `g field
            // Order memo:string?` rewrites the query, the transition and the
            // use case that construct `Order`, and reporting "17 files
            // written" leaves a reader who wanted to know whether their
            // companions moved with no way to find out but `git status`. The
            // same lines `--pretend` prints, so the preview and the report
            // cannot describe the transition differently.
            //
            // A deleted file is the one that most needs saying -- it leaves
            // nothing behind, and with `--force` it may have carried an
            // afternoon of edits -- and a migration is the other: it is
            // append-only, so the moment to read it is before it reaches a
            // database.
            for line in crate::model_command::preview_lines(&bundle) {
                println!("{line}");
            }
        }
        for line in &notes {
            println!("{line}");
        }
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&execution)
                .map_err(|error| Failure::Told(format!("could not encode execution: {error}")))?
        );
    }
    report::report_review(&bundle, &invocation);
    if invocation.output == Output::Human {
        clock.report();
    }
    run_follow_up_effects(&root, &bundle, &execution, &invocation)
}

pub(crate) fn operation_field_labels(
    model: &AppModel,
    entity: &str,
    fields: &[String],
) -> Result<Vec<String>> {
    operation_field_labels_via(model, entity, None, false, fields)
}

/// The same resolution, with a joined entity's components in scope.
///
/// **A `--via` query's filter may name a component the target does not have**,
/// and that is what the flag is for: `Message` has no `email`, so a query on
/// it was reachable only by a caller that already knew the surrogate user id.
/// `--via User` reads `users` alongside `messages`, and `email` is a column of
/// the join rather than of the row. The model says so directly -- an operation
/// parameter "may name a join alias that has no column on this table".
///
/// Target first, so a name both entities declare resolves against the one the
/// query is on, which is where a reader would expect it to.
pub(crate) fn operation_field_labels_via(
    model: &AppModel,
    entity: &str,
    joined: Option<&str>,
    optional_filters: bool,
    fields: &[String],
) -> Result<Vec<String>> {
    fields
        .iter()
        .map(|field| {
            // **A trailing `?` on a query filter is not a nullable column.**
            // `direction:MessageDirection?` means "filter by direction, or
            // do not" -- three independent optional filters is eight queries
            // written by hand -- and the model carries `optional_filter` for
            // it. Compared against the entity's own optionality it would read
            // as a disagreement and refuse.
            match optional_filters && field.ends_with('?') {
                true => Ok(format!(
                    "{}?",
                    resolve_filter(model, entity, joined, field.trim_end_matches('?'))?
                )),
                false => resolve(model, entity, joined, field),
            }
        })
        .collect()
}

/// The same, for a filter the caller may omit.
///
/// **The `?` stands in for the entity's own suffix rather than beside it.**
/// `status:string?` on an entity whose `status` is `string!` is the natural
/// spelling of "filter by status, or do not" -- nobody writes `status:string!?`
/// -- so the optionality is what the marker replaces and is not compared
/// against the column's. The *type* is still compared, because filtering a
/// `string` column with an `int` is a mistake either way.
fn resolve_filter(
    model: &AppModel,
    entity: &str,
    joined: Option<&str>,
    field: &str,
) -> Result<String> {
    let relaxed = field
        .split_once(':')
        .map_or_else(|| field.to_string(), |(name, _)| name.to_string());
    match resolve(model, entity, joined, field) {
        Ok(label) => Ok(label),
        // The bare name resolves against the entity's own declaration, so the
        // column still has to exist -- this relaxes the suffix, not the check.
        Err(error) => resolve(model, entity, joined, &relaxed).map_err(|_| error),
    }
}

fn resolve(model: &AppModel, entity: &str, joined: Option<&str>, field: &str) -> Result<String> {
    match operation_field_label(model, entity, field) {
        Ok(label) => Ok(label),
        // Qualified by the join's alias, because that is how the model names a
        // column that is not on this table: an unqualified `email` is checked
        // against the target and rejected, while `user.email` is the join's.
        Err(error) => match joined {
            Some(joined) => operation_field_label(model, joined, field)
                .map(|label| format!("{joined}.{label}"))
                .map_err(|_| error),
            None => Err(error),
        },
    }
}

/// The payload components of an event, where a typed token means something a
/// filter or an input cannot.
///
/// **An event is the one operation whose payload is not a subset of the row.**
/// It can carry a component the target does not have -- its own minted
/// identity, the moment it happened -- and JDL v1 spells that `name: type`,
/// against a bare `name` for a projection. Everywhere else a typed token is a
/// redundant restatement of a projection, checked against the entity field and
/// then collapsed to its label; here it is the only way to say the thing.
///
/// This is what makes `g usecase --yields` reachable: an outbox stages by a
/// *minted* `id`, and an event whose `id` is projected from the row makes
/// `on conflict (id) do nothing` discard the second event about that resource.
/// Without a spelling for the difference the flag writes a policy the model
/// then refuses.
pub(crate) fn event_component_declarations(
    model: &AppModel,
    entity: &str,
    fields: &[String],
) -> Result<Vec<String>> {
    fields
        .iter()
        .map(|token| match token.split_once(':') {
            Some((name, ty)) => {
                let parsed = parse_field(token)?;
                if parsed.primary_key
                    || parsed.unique
                    || parsed.indexed
                    || parsed.min_length.is_some()
                    || parsed.max_length.is_some()
                {
                    return Err(Failure::Told(format!(
                        "event component `{token}` carries a table constraint.\n       fix: an event is not stored -- use `{name}:{ty}` without `@pk`, `@unique`, `@index` or a range"
                    )));
                }
                Ok(format!("{}: {}", parsed.label, parsed.type_name))
            }
            None if entity.is_empty() => Err(Failure::Told(format!(
                "event component `{token}` has no type, and this event names no entity to read one from.\n       fix: write `{token}:<type>`, or pass `--on <Entity>` to borrow the row's"
            ))),
            None => operation_field_label(model, entity, token),
        })
        .collect()
}

pub(crate) fn operation_field_label(model: &AppModel, entity: &str, token: &str) -> Result<String> {
    let declaration = model
        .entities
        .values()
        .find(|candidate| candidate.label == entity)
        .ok_or_else(|| {
            Failure::Told(format!(
                "`{entity}` does not name a canonical entity.\n       fix: choose an entity declared under `[entities]`"
            ))
        })?;
    if !token.contains(':') {
        let label = java_to_label(token);
        if declaration.fields.iter().any(|field| field.label == label) {
            return Ok(label);
        }
        return Err(Failure::Told(format!(
            "`{token}` is not a field on `{entity}`.\n       fix: name an existing entity field"
        )));
    }
    let parsed = parse_field(token)?;
    let field = declaration
        .fields
        .iter()
        .find(|field| field.label == parsed.label)
        .ok_or_else(|| {
            Failure::Told(format!(
                "`{}` is not a field on `{entity}`.\n       fix: name an existing entity field",
                parsed.java_name
            ))
        })?;
    if parsed.primary_key
        || parsed.unique
        || parsed.indexed
        || parsed.min_length.is_some()
        || parsed.max_length.is_some()
    {
        return Err(Failure::Told(format!(
            "operation field `{token}` redeclares an entity constraint.\n       fix: use `{}:{}` without range, `@pk`, `@unique`, or `@index`",
            parsed.java_name,
            field.ty.canonical_name()
        )));
    }
    let expected_type = field.ty.canonical_name();
    if parsed.type_name != expected_type
        || parsed.required != field.required
        || parsed.non_blank != field.non_blank
    {
        return Err(Failure::Told(format!(
            "operation field `{token}` disagrees with canonical entity field `{entity}.{}`.\n       fix: use `{}:{expected_type}{}` or the bare field name",
            field.label,
            field.names.java_member,
            if !field.required {
                "?"
            } else if field.non_blank {
                "!"
            } else {
                ""
            }
        )));
    }
    Ok(parsed.label)
}

pub(crate) fn kind_name(kind: ArtifactKind) -> String {
    use clap::ValueEnum as _;
    kind.to_possible_value()
        .map(|value| value.get_name().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
