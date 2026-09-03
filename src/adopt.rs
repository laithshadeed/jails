//! `jails adopt`: record what a foreign project already has.
//!
//! Two halves, for the two things a project jails did not create already
//! owns. **`jails adopt`** records what the project calls its layers, and
//! **`jails adopt resource <Name>`** registers a type the reader wrote in the
//! model, so the commands that evolve a declared entity -- `entity field`,
//! `rename resource`, `destroy` -- work on it.
//!
//! ## The layout half
//!
//! **Configuration, not machinery**, and that is why it is here rather than in
//! a transition engine. What it produces is `(layer, directory)` pairs and
//! nothing else, written into one `[layout]` table of `jails.toml`; everything
//! downstream already reads `Config::layers()`, so there is no code path to
//! change and nothing for a later command to reconcile.
//!
//! It is also one of the two commands that run *before* a project has a model
//! -- `jails model init` reads the layout this writes -- so it does not
//! initialise one, and a reader who runs it on a canonical project simply
//! updates a table their compiler already honours.
//!
//! Three rules, each load-bearing:
//!
//! - **An unrecognised directory is reported, not guessed.** A synonym table
//!   answers or it does not.
//! - **Two candidates for one layer writes neither**, because a `[layout]`
//!   table can only name one and picking would be silent.
//! - **`[project] capabilities` is never touched.** That is the list `jails
//!   sync` acts on, and it is unreachable from here by construction: nothing
//!   in this module produces a capability name.
//!
//! ## The resource half
//!
//! A model mutation like every other frontend's: it edits the JDL source and
//! hands `finish_generation` a [`PreparedMutation`], so `--pretend` previews
//! and `--output json` reports the same plan the real run executes. What it
//! writes is an `entity` declaration whose fields are the record's components
//! read off the reader's file, beside one `eject <Name>.record @adopted` line
//! saying the record is the reader's. The compiler then excludes the record
//! from the managed tree without creating anything -- there is nothing to
//! transfer -- and the reader's file rides in the plan as an exact input,
//! captured through `reader_paths`, never as an output.
//!
//! Three rules, each load-bearing:
//!
//! - **A Java type maps onto a field type through the one table in
//!   `jails-model`, read backwards** ([`BuiltinType::from_java`]). A type the
//!   table does not render to is refused by component; a capitalised name
//!   the project itself declares is a project type and passes through as
//!   one, which is what `jails g record` does with it.
//! - **A component is required unless it is `Optional<T>`.** That is the one
//!   nullability rule, stated rather than inferred from annotations, because
//!   the compact syntax has exactly those two shapes.
//! - **The record's package is recorded, never moved.** A record outside the
//!   `domain` layer is pinned with `@package`, so generated code imports it
//!   from where it is.

use crate::Invocation;
use crate::model_generate::{PreparedMutation, finish_generation};
use jails_contracts::ProjectPath;
use jails_model::field_syntax::java_to_label;
use jails_model::{BuiltinType, EntityId, Evolution, Package, StableId, boundary};
use jails_project::java::{self, Param};
use jails_project::project::Project;
use jails_project::synonyms::Reading;
use jails_support::{Failure, Result};
use std::collections::BTreeSet;
use std::path::Path;

/// Where a reader's own Java lives, and the only tree `adopt resource` reads.
const MAIN_JAVA: &str = "src/main/java";

/// `jails adopt resource <Name>`: register one type the reader wrote.
pub(crate) fn resource(name: String, invocation: Invocation) -> Result<()> {
    crate::model_command::ensure_owned(invocation.clone())?;
    let current = crate::model_command::Current::load(&invocation)?;
    let project = Project::load(&invocation.root()?)?;
    let relative = locate(&project, &name)?;
    let text = std::fs::read_to_string(project.root().join(&relative))
        .map_err(|error| Failure::Told(format!("could not read `{relative}`: {error}")))?;
    let info = java::type_info(&text).filter(|info| info.name == name).ok_or_else(|| {
        Failure::Told(format!(
            "`{relative}` does not declare a type called `{name}`.\n       fix: name the file's own type; a nested type cannot be adopted"
        ))
    })?;
    if info.constructor_params.is_empty() {
        return Err(Failure::Told(format!(
            "`{relative}` declares no record components or constructor parameters, so there are no fields to record.\n       fix: adopt a record, or a class whose constructor lists its fields"
        )));
    }

    let entity_label = java_to_label(&name);
    let entity_id = EntityId::parse(format!("ent_{entity_label}"))
        .map_err(|error| Failure::Told(format!("could not assign entity identity: {error}")))?;
    let record = boundary::RECORD.owned_by(entity_id.as_str());
    if let Some(existing) = current
        .model
        .entities
        .values()
        .find(|entity| entity.names.java_type == name || entity.id == entity_id)
    {
        if current
            .model
            .ejections
            .values()
            .any(|ejection| ejection.adopted && ejection.target == record)
        {
            println!("{name} is already adopted (0 files written)");
            return Ok(());
        }
        return Err(Failure::Told(format!(
            "`{}` is already declared, and jails renders its record.\n       fix: `jails destroy record {}` first if `{relative}` is the one to keep, or delete your copy and let the managed record stand",
            existing.names.java_type, existing.names.java_type
        )));
    }

    let main_root = project.root().join(MAIN_JAVA);
    let fields = info
        .constructor_params
        .iter()
        .map(|param| field_token(&main_root, &name, param))
        .collect::<Result<Vec<_>>>()?;

    // **Where the record is, as the model spells it.** The convention puts a
    // record in the `domain` layer -- under the name `jails.toml` gives that
    // layer, which is the compiler's answer too, because capture hands it
    // the same layout -- and one anywhere else is pinned with `@package`,
    // relative to the base, so every generated importer finds it. A package
    // outside the base is refused by `normalize_package`.
    let mut convention = current.model.project.clone();
    convention.layout = project.facts().layout.clone();
    let package = java::package_of(&text).unwrap_or_default();
    let pinned = (package != convention.package_for(Package::Domain))
        .then(|| crate::model_generate_jdl::normalize_package(&convention.base_package, &package))
        .transpose()?;
    let declaration = crate::model_generate_jdl::entity_declaration(
        &current.model,
        &crate::model_generate_jdl::EntityDeclaration {
            java_name: &name,
            scaffold: false,
            fields: &fields,
            path: None,
            uniques: &[],
            package: pinned.as_deref(),
        },
    )?;
    let ejection = crate::model_eject::ejection_id(&record)?;
    let next_source = jails_model::append_jdl_declaration(
        &current.source,
        &format!(
            "{declaration}\neject {name}.record @id({}) @adopted\n",
            ejection.as_str()
        ),
    )
    .map_err(crate::model_generate_jdl::jdl_edit_failure)?;
    // A check that the declaration links -- the boundary path resolves, the
    // field types are ones the linker accepts -- before anything is planned.
    crate::model_command::parse(&next_source)?;

    if invocation.output == crate::Output::Human {
        println!("  adopt   {name}  {relative}");
        for field in &fields {
            println!("  field   {field}");
        }
        if let Some(package) = &pinned {
            println!("  package {package}  (pinned: outside the domain layer)");
        }
        println!("  yours   {name}.record  -- jails will not write `{relative}`");
    }
    finish_generation(PreparedMutation {
        name,
        invocation,
        current,
        next_source,
        evolution: Evolution::none(),
        authored_migration: None,
        reader_paths: vec![ProjectPath::parse(relative).map_err(Failure::Told)?],
    })
}

/// The one `<Name>.java` under `src/main/java`, project-relative.
///
/// Every match is listed rather than one picked, the way `jails src` answers
/// the same question: two files declaring one simple name is a fact about the
/// project, and adopting either would register the other's namesake as well.
fn locate(project: &Project, name: &str) -> Result<String> {
    let wanted = format!("{name}.java");
    let matches = java::source_files(&project.root().join(MAIN_JAVA))
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|file| file == wanted.as_str()))
        .map(|path| {
            path.strip_prefix(project.root())
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(Failure::Told(format!(
            "no `{wanted}` under {MAIN_JAVA}.\n       fix: check the spelling -- `jails src {name}` lists every match -- or `jails g record {name} <fields>` to have jails write one"
        ))),
        many => Err(Failure::Told(format!(
            "`{wanted}` is declared in more than one place: {}.\n       fix: move or remove the ones that are not `{name}`, then run this again",
            many.join(", ")
        ))),
    }
}

/// One component as the compact field syntax spells it: `title:string`,
/// `body:string?`, `priority:Priority`.
fn field_token(main_root: &Path, owner: &str, param: &Param) -> Result<String> {
    let written = param.raw_type.trim().trim_start_matches("final ").trim();
    let (spelling, optional) = match written
        .strip_prefix("Optional<")
        .or_else(|| written.strip_prefix("java.util.Optional<"))
        .and_then(|inner| inner.strip_suffix('>'))
    {
        Some(inner) => (inner.trim(), true),
        None => (written, false),
    };
    let suffix = if optional { "?" } else { "" };
    let simple = java::simple_name(spelling);
    if let Some(builtin) =
        BuiltinType::from_java(spelling).or_else(|| BuiltinType::from_java(&simple))
    {
        return Ok(format!(
            "{}:{}{suffix}",
            param.name,
            builtin.semantics().token
        ));
    }
    // A capitalised name the project itself declares -- an enum of its own,
    // a value type -- is what `name:Priority` means in the compact syntax:
    // passed through verbatim, never imported. Checked on disk rather than
    // assumed, because `Date` is capitalised too and nothing here declares it.
    if !spelling.contains('<')
        && !spelling.contains('[')
        && simple.starts_with(|c: char| c.is_ascii_uppercase())
        && java::source_files(main_root).iter().any(|path| {
            path.file_name()
                .is_some_and(|f| f == format!("{simple}.java").as_str())
        })
    {
        return Ok(format!("{}:{simple}{suffix}", param.name));
    }
    Err(Failure::Told(format!(
        "component `{}` of `{owner}` has type `{written}`, which jails cannot record as a field type.\n       fix: the types jails knows are {}; a capitalised type this project declares under {MAIN_JAVA} passes through by name",
        param.name,
        known_java_spellings()
    )))
}

/// Every Java spelling the table renders to, for the refusal that names them.
fn known_java_spellings() -> String {
    BuiltinType::java_spellings()
        .iter()
        .map(|spelling| format!("`{spelling}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn layout(invocation: Invocation) -> Result<()> {
    let project = Project::discover()?;
    if project.base().is_empty() {
        return Err(Failure::Told(
            "no Java sources found under src/main/java, so there is no package to read.\n       \
             fix: run this from a project with sources, or `jails new <name>` to create one."
                .to_string(),
        ));
    }
    let names = subpackages(&project);
    let readings = jails_project::synonyms::readings(&names);
    let resolved = jails_project::synonyms::resolve(&readings);

    for reading in &readings {
        if let Reading::Conventional(layer) = reading {
            println!("  keep    {layer:<10} already jails' own name");
        }
    }
    for (layer, dir) in &resolved.writes {
        println!("  layout  {layer:<10} = \"{dir}\"");
    }
    for (layer, dirs) in &resolved.ambiguous {
        println!(
            "  ask     {layer:<10} matches {} -- a [layout] table can only name one, so none \
             is written",
            dirs.join(", ")
        );
    }
    for name in &resolved.unknown {
        println!("  ignore  {name:<10} not a layer jails knows -- left alone");
    }
    if resolved.writes.is_empty() {
        return Err(Failure::Told(
            "nothing to adopt: no package under the base package needs a different name."
                .to_string(),
        ));
    }
    println!("[project] capabilities is not touched: `jails sync` acts on that list.");
    if invocation.pretend {
        println!("--pretend: nothing was written.");
        return Ok(());
    }

    // **Composed against one text and written once.** Splicing each layer
    // against a re-read file is how the second edit comes to be written over
    // the first, and `jails.toml` has more than one contributor: the
    // capability list `add` maintains lives in the same file.
    let path = project.root().join(jails_project::config::FILE);
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    for (layer, directory) in &resolved.writes {
        text = jails_project::config::with_layout(&text, layer, directory)?;
    }
    jails_support::apply::put_one_shot(&path, text)?;
    println!("wrote {}", jails_project::config::FILE);
    Ok(())
}

/// The subpackages of the base package that hold Java.
///
/// **Found by the Java in them, not by listing a directory**, and the
/// difference is not pedantry: a listing returns names without kinds, so a
/// *file* called `controllers` would be adopted as the web layer's package,
/// and a directory holding no Java is not a package anybody can be in.
///
/// The whole package-relative path, not its first segment: a class in
/// `infra/jdbc` recorded as `adapters = "infra"` names a package holding no
/// Java at all, and every later command would be pointed at an empty tree.
fn subpackages(project: &Project) -> Vec<String> {
    let base = format!("src/main/java/{}", project.base().replace('.', "/"));
    let root = project.root().join(&base);
    let mut names = BTreeSet::new();
    for absolute in jails_project::java::source_files(&root) {
        let Ok(relative) = absolute.strip_prefix(&root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        // A `.java` directly under the base package has no `/` and is in no
        // subpackage.
        if let Some((package, _)) = relative.rsplit_once('/') {
            names.insert(package.replace('/', "."));
        }
    }
    names.into_iter().collect()
}
