//! Entry points into the canonical semantic model and exact plans.

use crate::cli::ModelCommand;
use crate::{Invocation, Output};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::{Path, PathBuf};

const CHECK_SCHEMA: &str = "jails.model-check.v1";
pub(crate) const JDL_PATH: &str = jails_model::MODEL_FILE;
/// The file a project wrote before `jdl 1`. Nothing reads it; a project that
/// still has one is refused by name so a model is never seeded beside it.
const TOML_PATH: &str = ".jails/model.toml";

/// The project a command is about: the nearest ancestor that is one.
///
/// **The same walk `jails_spec::spec::paths::find_project_root` does**, plus
/// the two model markers, and that agreement is the whole point: testing
/// `.jails/model.jdl` against the *process* directory would make `jails g
/// record` in `src/main/java` answer differently from the same command at
/// the root.
///
/// Nearest wins, and the model markers are checked at each level before the
/// build marker, so a canonical root is recognised as one. A nested module
/// with its own build file and no model is its own project rather than
/// being claimed by an ancestor's model.
pub(crate) fn project_root() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join(JDL_PATH).is_file()
            || jails_spec::build::detect(&dir) != jails_spec::build::Build::Bare
        {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// The root every canonical command works against.
///
/// **"You are not in a project" is answered here, once.** This is the one
/// walk, so it is the only place that knows the answer is no, and letting
/// each command fall through to its own next step produced four wordings for
/// one condition: two differing lists of build files, a paragraph about the
/// base package, and a missing-model error that never mentioned the
/// directory. The reader learns the same thing whichever command they typed.
pub(crate) fn root() -> Result<PathBuf> {
    match project_root() {
        Some(root) => Ok(root),
        None => Err(Failure::Told(jails_spec::spec::paths::not_a_project())),
    }
}

/// The one read of the model, against a root the caller has resolved.
///
/// The root is an argument rather than a walk because `jails new --app`
/// replays a manifest into the project it is creating, and the process
/// directory is that project's *parent*; `Invocation::root` is where the
/// walk lives.
pub(crate) fn read_source(root: &Path, model_path: &Path) -> Result<String> {
    let source = match std::fs::read_to_string(root.join(model_path)) {
        Ok(source) => source,
        // **A project still on `.jails/model.toml` reads as a refusal, not as
        // an absence.** Nothing in this binary reads that file, and deriving a
        // seed beside it would strand its declarations outside the model that
        // owns them, so the file is named rather than passed over.
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound && root.join(TOML_PATH).is_file() =>
        {
            return Err(refuse_retired_toml());
        }
        // The same rule as `load_model`: a project with no model reads as
        // the model `model init` would write, so the first mutation patches a
        // real seed rather than refusing over the file it is about to create.
        // **The derive's own refusal is what the reader needs**, not a report
        // that a file jails was about to create is missing. Discarding it
        // would say "could not read `.jails/model.jdl`: No such file or
        // directory" about a project whose base package cannot be read, which
        // names neither the problem nor anything to do about it.
        //
        // Returned rather than checked below: what `model init` derives is
        // `jdl 1` by construction, so putting it through the dialect test
        // would only be able to fail on a bug in the deriver.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !owns(root) => {
            return jails_project::project::Project::load(root)
                .and_then(|project| crate::model_init::derive(&project));
        }
        Err(error) => {
            return Err(Failure::Told(format!(
                "could not read model `{}`: {error}",
                model_path.display()
            )));
        }
    };
    // **One model language, and it is `jdl 1`.** The header is checked here
    // rather than left to the parser so the refusal names the file and what
    // it has to be, before any command reads a declaration out of it.
    if model_path.ends_with(JDL_PATH) && !starts_with_jdl_header(&source) {
        return Err(refuse_not_jdl_1());
    }
    Ok(source)
}

fn refuse_retired_toml() -> Failure {
    Failure::Told(format!(
        "`{TOML_PATH}` is not a model this jails reads.\n       fix: write the model as `jdl 1` in `{JDL_PATH}` and remove `{TOML_PATH}`"
    ))
}

fn refuse_not_jdl_1() -> Failure {
    Failure::Told(format!(
        "`{JDL_PATH}` does not start with `jdl 1`.\n       fix: rewrite it to start with `jdl 1`, then check it with `jails model check`"
    ))
}

/// The header test: the first line that is neither blank nor a comment opens
/// with `jdl`.
fn starts_with_jdl_header(source: &str) -> bool {
    source
        .lines()
        .find_map(|line| {
            let line = line.trim();
            (!line.is_empty() && !line.starts_with("//")).then_some(line)
        })
        .is_some_and(|line| line.split_whitespace().next() == Some("jdl"))
}

/// The model a mutation starts from: the one editable source, read through
/// [`read_source`], and the model it links to.
///
/// **Every frontend begins here and nowhere else.** The read is anchored to
/// the invocation's project -- the process directory's nearest model for a
/// typed command, the tree being created for `jails new --app` -- so a
/// frontend cannot read one project and plan another.
#[derive(Clone)]
pub(crate) struct Current {
    pub(crate) source: String,
    pub(crate) model: jails_model::AppModel,
}

impl Current {
    pub(crate) fn load(invocation: &Invocation) -> Result<Self> {
        let source = read_source(&invocation.root()?, Path::new(JDL_PATH))?;
        let model = parse(&source)?;
        Ok(Self { source, model })
    }
}

/// Parse and link JDL text, rendering the diagnostics as one refusal.
pub(crate) fn parse(source: &str) -> Result<jails_model::AppModel> {
    jails_model::parse_jdl(source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))
}

/// Whether this root has a model. The root is the caller's: a walk from the
/// process directory is the wrong answer for every command holding an
/// explicit one -- `jails new --app` stands in the parent of the project it
/// is creating.
pub(crate) fn owns(root: &Path) -> bool {
    root.join(JDL_PATH).is_file()
}

/// Give this project a model if it has none, so a mutation has somewhere to go.
///
/// Every project jails creates is canonical from its first command; this is
/// what makes somebody else's repository canonical from its first mutation,
/// without the reader having to know `model init` exists.
///
/// It adopts no line of the reader's Java: what it writes is the app block,
/// every field of it read off the project rather than asked for. What changes
/// is that the *next* generator renders through the compiler, and that is
/// said out loud rather than done quietly -- a reader whose files start being
/// merge-managed with no explanation has been surprised by their tool.
///
/// **A project holding `.jails/ledger.toml` is refused by name**: nothing in
/// this binary can read it, and auto-initialising over it would strand the
/// project's whole contents outside the model that owns it. `--pretend`
/// refuses too: a dry run must not write, and there is no model to plan
/// against.
pub(crate) fn ensure_owned(invocation: Invocation) -> Result<()> {
    // **The invocation's project, not the process directory.** `jails new
    // --app` stands in the *parent* of the project it is creating and replays
    // the manifest through these same frontends, so asking the walk would ask
    // about the wrong tree -- and answer "not canonical" about a project that
    // was seeded a moment ago.
    let root = invocation.root()?;
    if owns(&root) {
        return Ok(());
    }
    // **Nothing is written here, and that is the point.** Running `model
    // init` as its own transition before the command that needs it would let
    // `jails add csv security` on a plain Maven project create the model,
    // splice the pom, and only *then* refuse `security` -- leaving a project
    // half-converted by a command that failed. The seed is derived in memory
    // by `load_model`, and the mutation's own plan carries it as an
    // ordinary `ReplaceModelFile` with no before-image, so a refusal anywhere
    // in that plan writes nothing at all and a success creates the model in
    // the same reviewed transition. What is left here is the one refusal that
    // has to come first, because it is about a model this jails cannot read
    // rather than about the mutation.
    crate::model_init::refuse_if_modelled(&root)?;
    // **That this is a Java project at all is still asked here.** The seed is
    // derived from the project -- its package, its build file, its release --
    // so a directory that is not one has no model to derive and no mutation to
    // apply, and saying so before planning is what turns a report about a
    // missing `.jails/model.jdl` into the answer the reader needs.
    jails_project::project::Project::load(&root).map(|_| ())
}

/// `jails sync`: recompile the model as it stands and make the tree match.
///
/// `no_start` is accepted rather than refused by name: a sync can introduce
/// a compose service, so a script that passes the flag is saying something
/// coherent -- it just happens to be what sync does anyway. The root is the
/// invocation's, which is the process directory's walk for every command but
/// `jails new --app`, which stands in the parent of the project it creates.
pub(crate) fn sync(no_start: bool, invocation: Invocation) -> Result<()> {
    let invocation = invocation.without_starting(no_start);
    let root = &invocation.root()?;
    // **`jails.toml` is still read, and a name it gets wrong is still an
    // error.** The model is what sync applies, so nothing here acts on
    // that file's capability list -- but a `postgress` sitting in it looks
    // applied and never will be, which is the exact failure a manifest exists
    // to remove. Parsing it is also how `[layout]` is validated, so the read
    // is one the project needs either way.
    jails_project::config::Config::load(root)?;
    let manifest = resolve_manifest(None)?;
    let (source, model) = load_model(root, &manifest, invocation.output)?;
    let bare = model.capabilities.is_empty();
    let bundle = compile(
        root,
        &manifest,
        source.as_bytes(),
        model,
        // **`sync` is the verb that makes the tree match the model, and a
        // deleted managed file is the tree not matching it.** It used to
        // refuse and name `jails entity repair`, which is a second verb for
        // the same sentence -- and the reader who deleted the file reached
        // for `sync` first, got a refusal, and had to learn a command they
        // will use once. Nothing can be lost: a managed file is reproducible
        // from the model, a reader edit inside one is a merge rather than a
        // deletion, and deleting it again is one keystroke.
        Repair::MissingManagedFiles,
        invocation.output.into(),
    )?;
    // **A dry run must not write.** `sync` is the command a
    // reader reaches for when they are least sure what the tree is about to
    // become -- a merged branch, an edited model -- so it is the last one
    // that should ignore the flag.
    if let Some(path) = &invocation.plan_out {
        crate::model_generate::write_bundle(path, &bundle)?;
    }
    if invocation.pretend || invocation.plan_out.is_some() {
        return crate::model_generate::report_plan(&bundle, &invocation);
    }
    let execution = jails_workspace::execute(root, &bundle).map_err(|error| {
        Failure::diagnosed(error.code, format!("could not synchronize model: {error}"))
    })?;
    if invocation.output == Output::Human {
        // The same distinction `add` draws: a sync over a project that is
        // already correct did all its work and changed nothing, which is the
        // answer worth saying rather than three zeroes.
        if execution.files_written == 0 && execution.files_deleted == 0 {
            println!("nothing to do, the project already matches the model");
        } else {
            println!(
                "synchronized {}: {} operations, {} files written, {} files deleted",
                execution.plan_digest.as_str(),
                execution.operations,
                execution.files_written,
                execution.files_deleted
            );
            // Every path it changes, the same list every other mutation
            // prints: a convergence that names only its deletions answers
            // half the question a reader runs `sync` to ask.
            for line in crate::plan_delta::preview_lines(&bundle) {
                println!("{line}");
            }
        }
        // **A project that has nothing to sync is usually a project that has
        // not declared anything yet**, and the report on its own reads as a
        // tool that did not work. Saying which command puts something in the
        // model costs one line and answers the question the reader is actually
        // about to ask. It is said whether or not files moved: the sync that
        // creates the model on a foreign project writes two and still has an
        // empty model to show for it.
        if bare {
            println!("       no capabilities are declared: `jails add <capability>` declares one");
        }
    } else {
        let delta = crate::plan_delta::preview(&bundle);
        let status = match execution.files_written == 0 && execution.files_deleted == 0 {
            true => "nothing-to-do",
            false => "synchronized",
        };
        let value =
            crate::model_generate::report::json_report(status, "sync", &bundle, &delta, &[]);
        print_json(&value)?;
    }
    // **Sync converges the files; `add` installs the service.** A compose
    // service that arrives through an edited model is worth naming -- the
    // reader has just gained one and nothing else would say so -- but
    // starting it is not what "make the tree match the model" means, and a
    // convergence command that brings containers up cannot be run casually.
    // `--no-start` is accepted for the scripts that pass it and changes
    // nothing here.
    crate::model_generate::run_follow_up_effects(
        root,
        &bundle,
        &execution,
        &invocation.clone().without_starting(true),
    )
}

pub(crate) fn run(command: ModelCommand, invocation: Invocation) -> Result<()> {
    match command {
        ModelCommand::Init => crate::model_init::run(invocation),
        ModelCommand::Check { manifest, frozen } => {
            let manifest = resolve_manifest(manifest.as_deref())?;
            check(&invocation.root()?, &manifest, frozen, invocation.output)
        }
        ModelCommand::Fmt { check } => format(check, invocation),
        ModelCommand::Plan { manifest, bundle } => {
            let manifest = resolve_manifest(manifest.as_deref())?;
            // `--plan-out` is the spelling; `--bundle` is the retired one,
            // hidden and still parsed. Whichever was given names the file.
            let bundle = bundle.or_else(|| invocation.plan_out.clone());
            plan(
                &invocation.root()?,
                &manifest,
                bundle.as_deref(),
                invocation.output,
            )
        }
        ModelCommand::Apply { bundle } => {
            // `--plan-in` is the spelling; `--bundle` is the retired one.
            let bundle = bundle.or_else(|| invocation.plan_in.clone()).ok_or_else(|| {
                Failure::Told(
                    "`jails model apply` needs the reviewed plan to apply.\n       fix: pass `--plan-in <file>`, the bundle a `--plan-out` run wrote"
                        .to_string(),
                )
            })?;
            apply(&bundle, invocation.output)
        }
        ModelCommand::Eject { semantic_id } => crate::model_eject::run(semantic_id, invocation),
        ModelCommand::Explain { filter } => crate::model_explain::run(filter, invocation),
        ModelCommand::Jdl => jails_report::explain::explain_language(),
        ModelCommand::Status => crate::model_ownership::run(invocation),
        ModelCommand::Relocate => crate::model_relocate::run(invocation),
    }
}

fn format(check: bool, invocation: Invocation) -> Result<()> {
    if !invocation.root()?.join(JDL_PATH).is_file() {
        return Err(Failure::Told(format!(
            "`jails model fmt` requires the JDL authoring source `{JDL_PATH}`.\n       fix: import or create a JDL v1 model before formatting"
        )));
    }
    // **Formatting is syntactic, and a model being fixed is the one that most
    // needs it.** `fmt` used to link before it laid out a byte, so the file a
    // reader was mid-repair on -- a mistyped type, a name that does not
    // resolve -- could not be formatted at all, and the command answered with
    // a diagnostic about something it had not been asked to do. It formats
    // whatever the parser accepts; the linker's answer follows the layout
    // rather than standing in front of it.
    let root = invocation.root()?;
    let source = read_source(&root, Path::new(JDL_PATH))?;
    let next_source = jails_model::format_jdl_v1(&source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    // The round-trip is checked where there is a model to compare: a document
    // that does not link has no semantics for the formatter to have changed.
    let linked = parse(&source);
    if let Ok(model) = &linked
        && *model != parse(&next_source)?
    {
        return Err(Failure::Told(
            "the JDL formatter changed what the model means.\n       fix: report this formatter bug; the source was not written"
                .to_string(),
        ));
    }

    if check {
        if source != next_source {
            return Err(Failure::Told(format!(
                "formatting differs in `{JDL_PATH}`.\n       fix: run `jails model fmt` and review the change it makes"
            )));
        }
        if invocation.output == Output::Human {
            println!("model format valid: {JDL_PATH}");
        } else {
            print_json(&json!({
                "schema": "jails.model-format.v1",
                "formatted": true,
                "changed": false,
                "manifest": JDL_PATH,
            }))?;
        }
        // The layout is the question `--check` was asked and it has been
        // answered; a model that does not link still does not link, and
        // saying so here is what keeps `fmt` and `fmt --check` from
        // disagreeing about the same file.
        return linked.map(|_| ());
    }

    if source == next_source {
        if invocation.output == Output::Human {
            println!("model already formatted: {JDL_PATH}");
        } else {
            print_json(&json!({
                "schema": "jails.model-format.v1",
                "formatted": true,
                "changed": false,
                "manifest": JDL_PATH,
            }))?;
        }
        // Nothing to lay out, and the same linker answer `--check` gives.
        return linked.map(|_| ());
    }

    // A model that links goes through the one pipeline, so the layout change
    // is a reviewed plan like every other edit to this file. One that does
    // not has no plan to be part of -- the model file is the compiler's
    // input, not its output -- so the bytes are written and the refusal the
    // linker was already going to make follows them.
    let Ok(model) = linked else {
        jails_support::apply::put_one_shot(root.join(JDL_PATH), next_source)?;
        if invocation.output == Output::Human {
            println!("formatted: {JDL_PATH}");
        }
        return linked.map(|_| ());
    };
    crate::model_generate::finish_generation(crate::model_generate::PreparedMutation {
        name: "JDL formatting".to_string(),
        invocation,
        current: Current { source, model },
        next_source,
        evolution: jails_model::Evolution::none(),
        authored_migration: None,
        reader_paths: Vec::new(),
    })
}

pub(crate) fn apply(bundle_path: &Path, output: Output) -> Result<()> {
    let bytes = std::fs::read(bundle_path).map_err(|error| {
        Failure::Told(format!(
            "could not read plan file `{}`: {error}\n       fix: pass the file written by `jails model plan --bundle <path>`",
            bundle_path.display()
        ))
    })?;
    let bundle: jails_contracts::PlanBundle = serde_json::from_slice(&bytes).map_err(|error| {
        Failure::Told(format!(
            "could not decode plan file `{}`: {error}\n       fix: regenerate the bundle with this version of jails",
            bundle_path.display()
        ))
    })?;
    let root = crate::model_command::root()?;
    let execution = jails_workspace::execute(&root, &bundle).map_err(|error| {
        Failure::diagnosed(error.code, format!("could not apply the plan: {error}"))
    })?;
    if output == Output::Human {
        println!(
            "applied {}: {} operations, {} files written, {} files deleted",
            execution.plan_digest.as_str(),
            execution.operations,
            execution.files_written,
            execution.files_deleted
        );
    } else {
        // The report, like every other command's: an apply that changed
        // nothing has an empty list, and a caller reads the list rather than
        // three zeroes.
        let delta = crate::plan_delta::preview(&bundle);
        let status = match execution.files_written == 0 && execution.files_deleted == 0 {
            true => "nothing-to-do",
            false => "applied",
        };
        let value =
            crate::model_generate::report::json_report(status, "apply", &bundle, &delta, &[]);
        print_json(&value)?;
    }
    Ok(())
}

fn check(root: &Path, manifest: &Path, frozen: bool, output: Output) -> Result<()> {
    let (source, model) = load_model(root, manifest, output)?;
    let bundle = frozen
        .then(|| {
            compile(
                root,
                manifest,
                source.as_bytes(),
                model.clone(),
                Repair::No,
                Notice::Print,
            )
        })
        .transpose()?;
    if let Some(bundle) = &bundle
        && !bundle.plan.operations.is_empty()
    {
        return frozen_failure(manifest, bundle, output);
    }

    if output == Output::Human {
        println!(
            "model valid{}: {} ({} nodes, {} entities, {} operations)",
            if frozen { " and frozen" } else { "" },
            manifest.display(),
            model.node_count(),
            model.entities.len(),
            model.operations.len()
        );
    } else {
        print_json(&json!({
            "schema": CHECK_SCHEMA,
            "valid": true,
            "frozen": frozen,
            "manifest": manifest,
            "summary": {
                "nodes": model.node_count(),
                "capabilities": model.capabilities.len(),
                "entities": model.entities.len(),
                "operations": model.operations.len(),
            },
            "model": model,
            "plan_digest": bundle.map(|bundle| bundle.plan.digest),
            "diagnostics": [],
        }))?;
    }
    Ok(())
}

fn plan(root: &Path, manifest: &Path, bundle_path: Option<&Path>, output: Output) -> Result<()> {
    let (source, model) = load_model(root, manifest, output)?;
    let bundle = compile(
        root,
        manifest,
        source.as_bytes(),
        model,
        Repair::No,
        Notice::Print,
    )?;
    let encoded = serde_json::to_vec_pretty(&bundle)
        .map_err(|error| Failure::Told(format!("could not encode the plan: {error}")))?;
    if let Some(path) = bundle_path {
        jails_support::apply::put_outside_project_private_atomic(path, &encoded)?;
    }
    if output == Output::Human {
        let delta = crate::plan_delta::preview(&bundle);
        println!(
            "plan {}: {} operations, {}{}",
            bundle.plan.digest.as_str(),
            bundle.plan.operations.len(),
            delta.summary(),
            bundle_path.map_or_else(String::new, |path| format!(", bundle {}", path.display()))
        );
        for line in &delta.lines() {
            println!("{line}");
        }
    } else {
        println!("{}", String::from_utf8_lossy(&encoded));
    }
    Ok(())
}

/// Compile and apply a freshly seeded model, printing nothing.
///
/// `jails new` needs the first canonical plan to *run* rather than to be
/// reported: the project it creates has to arrive with its
/// `application.properties` written and `.jails/compiler.lock.json` recording
/// which keys the model owns. Without that lock the very next command sees
/// every key `new` wrote as reader-owned text and refuses to touch it.
pub(crate) fn materialize_seed(root: &Path) -> Result<()> {
    let manifest = resolve_manifest(None)?;
    let (source, model) = load_model(root, &manifest, Output::Human)?;
    let bundle = compile(
        root,
        &manifest,
        source.as_bytes(),
        model,
        Repair::No,
        Notice::Silent,
    )?;
    jails_workspace::execute(root, &bundle)
        .map_err(|error| Failure::Told(format!("could not apply the seeded model: {error}")))?;
    Ok(())
}

/// Whether this compilation is `jails entity repair`.
///
/// It rides on `compile` rather than on a wrapper beside it: a second
/// root-taking entry point is one more place re-deriving what the existing
/// one decides, and this is one more value it decides with.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Repair {
    No,
    /// `jails sync`: write back what is simply gone.
    MissingManagedFiles,
    /// `jails entity repair`: also rewrite an edited sealed migration.
    MissingOrEditedMigrations,
}

/// Whether this compilation's diagnostics reach the reader.
///
/// **A warning that stays inside the draft is a warning nobody reads.**
/// Printing from the one place `g`, `sync`, `plan` and `repair` all compile
/// through is what stops a warning appearing on one command and vanishing on
/// another, or between `--pretend` and the real run.
///
/// `Silent` is `jails new`'s seed, which is documented to print nothing, and
/// `--json`, whose answer is the payload on stdout.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Notice {
    Print,
    Silent,
}

impl From<Output> for Notice {
    fn from(output: Output) -> Self {
        match output {
            Output::Human => Self::Print,
            _ => Self::Silent,
        }
    }
}

fn compile(
    root: &Path,
    manifest: &Path,
    source: &[u8],
    model: jails_model::AppModel,
    repair: Repair,
    notice: Notice,
) -> Result<jails_contracts::PlanBundle> {
    let reader_paths = jails_compiler::external_project_paths(&model);
    let mut snapshot = jails_project::capture::capture(
        root,
        manifest,
        source,
        model,
        None,
        &reader_paths,
        jails_project::capture::ModelFile::Observed,
    )
    .map_err(|error| {
        Failure::diagnosed(error.code, format!("could not capture workspace: {error}"))
    })?;
    let draft = jails_compiler::Compiler::compile(
        &snapshot,
        &snapshot.model.model,
        &jails_model::Evolution::none(),
    )
    .map_err(|error| {
        Failure::diagnosed(
            error.code,
            format!("could not compile application model: {error}"),
        )
    })?;
    // After the compile and before materialization, in every pipeline: which
    // paths the render wants is known only now, and whether the reader has a
    // file at one of them is the one observation the capture could not make.
    jails_project::capture::observe_rendered_paths(
        root,
        &mut snapshot,
        draft.generated.files.keys(),
    )
    .map_err(|error| {
        Failure::diagnosed(error.code, format!("could not capture workspace: {error}"))
    })?;
    if notice == Notice::Print {
        for line in crate::model_generate::report::notice_lines(&draft.diagnostics) {
            println!("{line}");
        }
    }
    // Same capture, same model, same compiler: repair differs only in what
    // materialization does about a managed file that is no longer on disk.
    jails_workspace::materialize(
        &snapshot,
        jails_contracts::PlanInput::reconcile(),
        draft,
        None,
        jails_compiler::COMPILER_VERSION,
        match repair {
            Repair::No => jails_workspace::Restore::Refuse,
            Repair::MissingManagedFiles => jails_workspace::Restore::Missing,
            Repair::MissingOrEditedMigrations => jails_workspace::Restore::MissingOrEdited,
        },
    )
    .map_err(|error| Failure::diagnosed(error.code, format!("could not build the plan: {error}")))
}

/// `jails entity repair` on a canonical project.
///
/// **Ordinary compilation with one guard waived**, which is the whole of it:
/// managed output is reproducible from the model, so
/// a file the reader deleted has an exact answer and repair is writing it.
///
/// `sync` refuses on a deleted managed file, so this is the one command that
/// writes it back. It takes no `--strategy`: there is one strategy, and it
/// is the model.
pub(crate) fn repair(invocation: Invocation) -> Result<()> {
    let root = invocation.root()?;
    let manifest = resolve_manifest(None)?;
    let (source, model) = load_model(&root, &manifest, invocation.output)?;
    let bundle = compile(
        &root,
        &manifest,
        source.as_bytes(),
        model,
        Repair::MissingOrEditedMigrations,
        invocation.output.into(),
    )?;
    let execution = jails_workspace::execute(&root, &bundle)
        .map_err(|error| Failure::Told(format!("could not repair managed output: {error}")))?;
    if invocation.output == Output::Human {
        if execution.files_written == 0 && execution.files_deleted == 0 {
            println!("managed output already matches the model: nothing to repair");
        } else {
            println!(
                "repaired {}: {} files written, {} files deleted",
                execution.plan_digest.as_str(),
                execution.files_written,
                execution.files_deleted
            );
        }
        return Ok(());
    }
    let value = serde_json::to_value(execution)
        .map_err(|error| Failure::Told(format!("could not encode execution: {error}")))?;
    print_json(&value)?;
    Ok(())
}

fn frozen_failure(
    manifest: &Path,
    bundle: &jails_contracts::PlanBundle,
    output: Output,
) -> Result<()> {
    let message = format!(
        "managed output differs from model `{}` (plan {})",
        manifest.display(),
        bundle.plan.digest.as_str()
    );
    let fix = "review `jails model plan`, then apply that plan";
    if output == Output::Human {
        return Err(Failure::Told(format!("{message}\n       fix: {fix}")));
    }
    print_json(&json!({
        "schema": CHECK_SCHEMA,
        "valid": false,
        "frozen": false,
        "manifest": manifest,
        "plan_digest": bundle.plan.digest,
        "diagnostics": [{
            "code": "model-generated-drift",
            "path": jails_contracts::SourceRoot::PARENT,
            "message": message,
            "fix": fix,
        }],
    }))?;
    Err(Failure::Reported)
}

pub(crate) fn load_model(
    root: &Path,
    manifest: &Path,
    output: Output,
) -> Result<(String, jails_model::AppModel)> {
    // Joined to the project root, which is a no-op on the absolute path an
    // explicit `--manifest` resolves to. See `resolve_manifest`.
    let source = match std::fs::read_to_string(root.join(manifest)) {
        Ok(source) => source,
        // **A project with no model reads as the model `model init` would
        // write**, so a read-only command has something real to answer from.
        // Refusing here would make the dry run of every mutation the one
        // thing a reader cannot do on a project jails has not touched yet --
        // exactly when they most want to see the plan first. Deriving it
        // twice is free and cannot disagree: the seed is a
        // pure function of the project, and `model init` writes this same
        // source. Only a missing default source falls back, so an unreadable
        // model and a mistyped `--manifest` are still errors rather than a
        // silent plan against something the reader did not write.
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound && manifest == Path::new(JDL_PATH) =>
        {
            if root.join(TOML_PATH).is_file() {
                return Err(refuse_retired_toml());
            }
            // A project jails cannot read has no seed to derive, and the
            // honest answer there is still that the model is missing --
            // reporting why the *derivation* failed would answer a question
            // the reader did not ask.
            let derived = jails_project::project::Project::load(root)
                .and_then(|project| crate::model_init::derive(&project));
            match derived {
                Ok(source) => source,
                Err(_) => return io_failure(manifest, &error, output),
            }
        }
        Err(error) => return io_failure(manifest, &error, output),
    };
    match jails_model::parse_jdl(&source) {
        Ok(model) => Ok((source, model)),
        Err(diagnostics) if output == Output::Human => Err(Failure::Told(
            diagnostics.to_string().trim_end().to_string(),
        )),
        Err(diagnostics) => {
            print_json(&json!({
                "schema": CHECK_SCHEMA,
                "valid": false,
                "manifest": manifest,
                "diagnostics": diagnostics.diagnostics,
            }))?;
            Err(Failure::Reported)
        }
    }
}

/// Which model source this project authors in.
///
/// **The default is returned project-relative and the *explicit* one
/// absolute**, because the two are relative to different things: a default is
/// a fact about the project, while `--manifest` is a path the reader typed in
/// their own directory. Anchoring the default here instead would put an
/// absolute path into every report and every plan; resolving the explicit one
/// lazily would read it against the project root the moment the command ran
/// from a subdirectory. `load_model` joins the root either way, which is a
/// no-op on an absolute path.
pub(crate) fn resolve_manifest(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(explicit) = explicit {
        return std::path::absolute(explicit).map_err(|error| {
            Failure::Told(format!(
                "could not resolve `--manifest {}`: {error}",
                explicit.display()
            ))
        });
    }
    Ok(PathBuf::from(JDL_PATH))
}

fn io_failure<T>(manifest: &Path, error: &std::io::Error, output: Output) -> Result<T> {
    let message = format!(
        "could not read application model `{}`: {error}",
        manifest.display()
    );
    let fix = "create the model or pass `--manifest <path>`";
    if output == Output::Human {
        return Err(Failure::Told(format!("{message}\n       fix: {fix}")));
    }
    print_json(&json!({
        "schema": CHECK_SCHEMA,
        "valid": false,
        "manifest": manifest,
        "diagnostics": [{
            "code": "model-io",
            "path": "$",
            "message": message,
            "fix": fix,
        }],
    }))?;
    Err(Failure::Reported)
}

pub(crate) fn print_json(value: &serde_json::Value) -> Result<()> {
    let rendered = serde_json::to_string_pretty(value)
        .map_err(|error| Failure::Told(format!("could not encode model report: {error}")))?;
    println!("{rendered}");
    Ok(())
}
