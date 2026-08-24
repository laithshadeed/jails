//! `generate` and `destroy`: the routes whose subject is one artifact.
//!
//! `ReconcileScope::DirectEntity` is the narrow scope, and it is the whole
//! difference from the capability routes: `jails destroy record Note` says
//! nothing about `record Memo`, so silence about another identity must not be
//! read as absence.

use super::*;

/// Generate one persistent artifact through the transaction protocol.
///
/// The direct counterpart of `generate_in_project`, and the subject is one
/// entity rather than the capability list: `ReconcileScope::DirectEntity` is
/// "exactly one direct `generate`/`destroy` request", so this route may add or
/// remove its own claim and says nothing about anybody else's.
#[allow(clippy::too_many_arguments)]
pub fn generate(
    run: &Run,
    kind: ArtifactKind,
    name: &str,
    fields: &[String],
    package: Option<&str>,
    indexes: &[String],
    on: Option<&str>,
    yields: Option<&str>,
) -> Result<Outcome> {
    let project = run.project();
    let change = with_test_support(
        project,
        jails_generate::generate::plan_recipe(
            project, kind, name, fields, package, indexes, on, yields,
        )?,
    );
    // A foreign build file changes what gets emitted -- plain JDBC instead of
    // `JdbcClient`, no JSpecify -- and a dependency claim splices into nothing
    // because there is no pom to splice it into. Said out loud, because the
    // alternative is a reader discovering it by reading the generated code.
    jails_generate::generate::report_degraded_shape(project, &change);
    let id = intent(project, kind, name, package, fields, indexes, on, yields)?;
    let owner = ResourceOwner::Entity(EntityId::Intent(id.clone()));
    let mut desired = desire::contribution(&owner, &change, project)?;
    let entity = DesiredEntity {
        id: EntityId::Intent(id.clone()),
        spec: EntitySpec::Intent(spec(project, kind, fields, indexes, on, yields)?),
        owners: BTreeSet::from([OwnerId::DirectCli]),
    };
    provenance::stamp_files(
        &mut desired,
        project,
        RendererId::Recipe(kind),
        Some(RenderedSubjectContext::Entity {
            id: entity.id.clone(),
            spec: entity.spec.clone(),
        }),
    )?;
    let reads = declaration(project, &change, &desired)?;
    // The request carries the resolved identity and spec, not the argument
    // strings: `g record Note title:string!` and the same call with the
    // package spelled out are one request, and a resume must recognise them
    // as one. The *syntax* half keeps the spelling separately.
    let asked = Asked::new(
        CanonicalMutationRequest::Generate(CanonicalGenerateRequest::Entity {
            id: entity.id.clone(),
            spec: entity.spec.clone(),
        }),
        &["generate"],
        std::iter::once(label(kind).to_string())
            .chain(std::iter::once(name.to_string()))
            .chain(fields.iter().cloned())
            .collect(),
        match package {
            Some(package) => BTreeMap::from([("package".to_string(), vec![package.to_string()])]),
            None => BTreeMap::new(),
        },
        BTreeSet::new(),
    );
    let request = Request {
        scope: ReconcileScope::DirectEntity(EntityId::Intent(id)),
        declared: BTreeMap::from([(entity.id.clone(), entity)]),
        changes: vec![desired],
    };
    commit(run, request, &reads, &asked)
}

/// Take one persistent artifact back out.
///
/// The counterpart of [`generate`], and the same shape as [`remove`] is to
/// [`install`]: nothing is described, the request simply stops declaring the
/// entity, and reconciliation works out what that means. §R6.2 asks
/// `destroy` to "forward-plan remaining resources from recorded exact state"
/// rather than rebuild a path list, which is what this is -- the store says
/// which resources this owner holds, and a resource nobody claims any more is
/// retired.
///
/// `ReconcileScope::DirectEntity` is the narrow scope for exactly this
/// reason: `jails destroy record Note` says nothing about `record Memo`, so
/// declaring nothing here retires one entity rather than every intent in the
/// project.
pub fn destroy(
    run: &Run,
    kind: ArtifactKind,
    name: &str,
    package: Option<&str>,
    force: bool,
) -> Result<Outcome> {
    // Decided before anything is looked up: these three are *forward-only*,
    // and "no such row" would be the wrong reason to refuse. A migration that
    // has run cannot be unrun by deleting its file, an association's DDL is
    // the same, and a field overlay is undone by another overlay. `why.rs`
    // explains the migration case to anyone who hits it, so the wording is
    // load-bearing rather than decorative.
    if matches!(
        kind,
        ArtifactKind::Migration | ArtifactKind::Association | ArtifactKind::Field
    ) {
        return Err(
            "migrations, associations, and field changes are forward-only; create a new \
             migration instead of destroying one"
                .to_string(),
        );
    }
    // `cases` is the fourth one-shot and the one that used to be an
    // exception. V1 destroyed it by rebuilding the test path from the
    // markdown path; here a one-shot is a *receipt*, keyed by its source and
    // the hash of that source's bytes, and the schema has no list for taking
    // one back -- deliberately, since `entities_removed` exists because an
    // entity can be relinquished and a one-shot cannot. Regenerating from the
    // same source is already a no-op, so the receipt is not in the way.
    if matches!(kind, ArtifactKind::Cases) {
        return Err(format!(
            "`cases` is a one-shot: it is recorded as a receipt over `{name}`'s bytes rather \
             than as an entity jails owns.\n       fix: delete the generated test yourself. \
             Re-running `jails g cases {name}` over the same brief changes nothing, so \
             nothing has to be taken back first."
        ));
    }
    let project = run.project();
    let id = intent(project, kind, name, package, &[], &[], None, None)?;
    let entity = EntityId::Intent(id.clone());
    let store = observed(project)?;
    if !store
        .ledger
        .as_ref()
        .is_some_and(|ledger| ledger.applied.iter().any(|row| row.id == entity))
    {
        // Naming the command that *would* have recorded it is the whole
        // difference between this and a bare "nothing to destroy" printed
        // over files that are right there.
        return Err(format!(
            "no `{} {}` is recorded in this project.\n       fix: `jails g {} {}` is what \
             records one. A destroy that guessed at paths would delete files jails never wrote.",
            label(kind),
            id.name,
            label(kind),
            id.name,
        ));
    }
    let owner = ResourceOwner::Entity(entity.clone());
    let mut reads = retiring(&store, &owner)?;
    let mut changes = Vec::new();
    if kind == ArtifactKind::Strategy {
        let strays = unnamed_implementations(project, &store, &owner, id.name.as_str())?;
        if !strays.is_empty() {
            let mut change = DesiredChange::owned_by(owner.clone());
            for path in strays {
                reads = reads.file(path.clone());
                change.absences.push(ManagedPath {
                    resource: ResourceKey::WholeFile(path.clone()),
                    path,
                    // The bytes are not jails'. `force` is exactly the flag
                    // that says so, and the human ask it requires is the
                    // deletion prompt every `destroy` puts up -- or `--force`,
                    // which is the reader answering it in advance.
                    force: true,
                });
            }
            changes.push(change);
        }
    }
    let request = Request {
        scope: ReconcileScope::DirectEntity(entity.clone()),
        declared: BTreeMap::new(),
        changes,
    };
    commit(
        run,
        request,
        &reads,
        &Asked::plain(
            CanonicalMutationRequest::destroy_entity(entity, force)?,
            &["destroy"],
            &[&label(kind), name],
        ),
    )
}

/// Every class implementing this strategy's port that the strategy does not
/// own, and the test beside it.
///
/// A strategy is an interface plus a bean per implementation, and the variants
/// are not something `destroy` is given -- it takes a kind and a name and no
/// fields. Reading them back is therefore not a shortcut but the *better*
/// answer: an implementation written by hand after the generate call is still
/// one of this strategy's classes, and leaving it behind implementing a
/// deleted interface stops the project compiling on the one operation whose
/// whole job is to leave no trace.
///
/// Scoped to the package the port lives in, because `type_info` reports
/// supertypes by simple name and a same-named interface in another package is
/// a different type. The port's own package is read off the recorded rows
/// rather than recomputed from `--package`, so a strategy generated into an
/// overridden package is still swept where it actually landed.
fn unnamed_implementations(
    project: &Project,
    store: &ObservedStore,
    owner: &ResourceOwner,
    port: &str,
) -> Result<Vec<ProjectPath>> {
    let owned: BTreeSet<ProjectPath> = store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.resources.iter())
        .filter(|row| row.owners.iter().all(|held| held == owner))
        .filter_map(|row| match &row.key {
            ResourceKey::WholeFile(path) => Some(path.clone()),
            _ => None,
        })
        .collect();
    const MAIN: &str = "src/main/java/";
    // The port's own file, found by name among the rows this entity owns. Not
    // the first main source it owns: a strategy owns its implementations too,
    // and taking one of those as the port would sweep every class implementing
    // *it* instead.
    let Some(directory) = owned.iter().find_map(|path| {
        let stem = path.as_str().strip_suffix(".java")?;
        let (directory, name) = stem.rsplit_once('/')?;
        (name == port && path.as_str().starts_with(MAIN)).then(|| format!("{directory}/"))
    }) else {
        return Ok(Vec::new());
    };
    let tests = format!("src/test/java/{}", &directory[MAIN.len()..]);
    let mut strays = Vec::new();
    for (absolute, source) in project.projected_main_sources() {
        let Ok(relative) = absolute.strip_prefix(project.root()) else {
            continue;
        };
        let Ok(path) = ProjectPath::parse(&relative.to_string_lossy()) else {
            continue;
        };
        if owned.contains(&path) || !path.as_str().starts_with(&directory) {
            continue;
        }
        let Some(info) = jails_java::java::type_info(&source) else {
            continue;
        };
        if info.name == port || !info.supertypes.iter().any(|held| held == port) {
            continue;
        }
        strays.push(path);
        // The companion test goes with it. It names a class that is about to
        // stop existing, so leaving it is the same compile failure one file
        // over.
        let test = format!("{tests}{}Test.java", info.name);
        if let Ok(test) = ProjectPath::parse(&test)
            && project.root().join(test.as_str()).is_file()
        {
            strays.push(test);
        }
    }
    Ok(strays)
}

/// The one entry point from an intent to the route that owns its kind.
///
/// A `jails generate` invocation and a `[[generate]]` manifest row are the
/// same intent, and three of the kinds it can name are not persistent entities
/// at all: `field` is an overlay on a target that already exists, `migration`
/// allocates a serial, and `cases` records a source-hash receipt. §R6.2 gives
/// each its own policy, and forwarding all of them to [`generate`] is what a
/// caller does when the selection lives at the call site rather than here --
/// which is exactly what a probe of the dispatch flip found, as `g cases`
/// reaching a recipe planner that has no arm for a one-shot.
///
/// The match is closed on `ArtifactKind`, so a kind added without deciding
/// which policy it follows is a compile error rather than a one-shot silently
/// planned as an entity.
pub fn recipe(run: &Run, intent: &Intent) -> Result<Outcome> {
    let package = intent.package.as_deref();
    match intent.kind {
        // `--timestamps` is expanded into two ordinary components before any
        // recipe sees it, through the same helper the manifest uses.
        ArtifactKind::Field => {
            if !intent.indexes.is_empty() || intent.on.is_some() || intent.yields.is_some() {
                return Err(
                    "field accepts one `name:type` component; --index/--on/--yields do not \
                     apply.\n       fix: put @index on the field itself, for example \
                     `createdAt:instant@index`."
                        .to_string(),
                );
            }
            let [component] = intent.fields.as_slice() else {
                return Err(format!(
                    "`g field` takes one target and one `name:type` component; this has {}.\n    \
                     \x20  fix: add one component per call. A field is one overlay on one \
                     recorded target, and two at once could not be undone separately.",
                    intent.fields.len()
                ));
            };
            super::field(run, &intent.name, component, package)
        }
        // These two use NAME as a description or a path rather than a Java
        // class name, which is why they are decided before the capitalisation
        // every other kind gets.
        ArtifactKind::Cases => super::cases(run, &intent.name, package),
        ArtifactKind::Migration => super::migration(run, &intent.name),
        _ => {
            let fields = match intent.timestamps {
                true => jails_generate::generate::with_timestamps(intent.kind, &intent.fields)?,
                false => intent.fields.clone(),
            };
            let name = jails_generate::generate::strip_redundant_suffix(
                intent.kind,
                &jails_generate::generate::capitalize(&intent.name),
            );
            generate(
                run,
                intent.kind,
                &name,
                &fields,
                package,
                &intent.indexes,
                intent.on.as_deref(),
                intent.yields.as_deref(),
            )
        }
    }
}
