//! `app apply`: a whole manifest as one transition.
//!
//! §R6.2's `app::apply` row asks for "one aggregate projected plan and one
//! commit", and names what that deletes: the project reload after every
//! capability and intent, the per-intent ledger save, the second capability
//! pass, and partial success.
//!
//! ## Why the reload existed, and what replaces it
//!
//! A manifest is ordered because its steps depend on each other. `add db`
//! puts the JDBC starter in the POM and `g search` refuses without it;
//! `g scaffold Note` writes a record and `g search Note title` reads its
//! components back. V1 makes that work by writing each step to disk and
//! resolving the project again, which is why a failure halfway leaves a
//! project that is neither the old one nor the new one.
//!
//! Here each step plans against a **projection** of everything before it.
//! `Project::projected` is the same value the recipes already take, resolved
//! from planned bytes instead of from disk -- so nothing about a recipe
//! changes, and nothing is written until the whole manifest has planned.
//!
//! The projection is rebuilt per step rather than grown in place, because the
//! read declaration is only known once a step has planned: a capture may not
//! be asked for a path nobody declared. That is a handful of small reads per
//! manifest entry, and the alternative is a plan that reaches past its own
//! snapshot.

use super::*;

/// One generation intent, as the engine takes it.
///
/// A `[[generate]]` row and the equivalent `jails generate` invocation are the
/// same thing, and §R6.2 says so: a direct call has *the same direct-owner
/// semantics as an equivalent manifest row*. So they are one type, and
/// [`super::recipe`] is the one entry point that turns either into the route
/// that owns the kind.
///
/// It was two types, and the second lived in the binary. `pending.md` §6.2:
/// a `[[generate]]` row became an `app::ResolvedIntent`, which became one of
/// these at the call site, which became an `IntentSpec` inside the route --
/// three copies of one request before anything checked it. The justification
/// for the first was manifest syntax, the deprecated `strategy_on` spellings,
/// and syntax should not survive its own parser: `GenerateIntent::finish`
/// resolves the aliases and produces this directly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Intent {
    pub kind: ArtifactKind,
    pub name: String,
    pub fields: Vec<String>,
    /// Expanded into two ordinary components before anything plans, through
    /// the same helper the CLI flag uses.
    pub timestamps: bool,
    pub indexes: Vec<String>,
    pub package: Option<String>,
    pub on: Option<String>,
    pub yields: Option<String>,
    /// The second resource a `query` reads alongside `--on`. plan.md P8.1.
    pub via: Option<String>,
    /// A `query`'s explicit order and row ceiling. plan.md P8.2.
    pub order_by: Option<String>,
    pub limit: Option<u32>,
    /// The target component whose unique constraint makes a `usecase` a
    /// get-or-create. plan.md P8.3.
    pub on_conflict: Option<String>,
    /// The route a generated endpoint answers. plan.md P8.7.
    pub path: Option<String>,
    /// Which component identifies the row a `transition` updates, `id` by
    /// default. A path variable binds to it, which is what lets a transition
    /// answer on a route whose key is in the URL.
    pub select: Option<String>,
    /// Components pinned to a constant rather than read from the request, as
    /// `component=literal`. Empty is "the caller supplies every one".
    pub set: Vec<String>,
    /// The HTTP verb, for the one recipe that answers HTTP.
    pub method: Option<jails_spec::spec::kind::HttpMethod>,
    /// How that endpoint reads its request. `missing.md` M15.
    pub consumes: Option<jails_spec::spec::kind::WireFormat>,
}

/// Apply a whole manifest in one transition.
///
/// `Run::pretending` makes this `app plan`, and there is deliberately no
/// second function for it. V1 answers `app plan` with a separate walk over the
/// intent list that compares each row against the ledger and prints
/// `pending`/`update`/`applied` -- a walk that cannot see a file the reader
/// edited, cannot tell a regeneration that changes nothing from one that
/// rewrites a class, and had to be shadowed against a typed comparison
/// precisely because two implementations of one question disagree. Here the
/// plan is this computation stopped one step before the lock: what it names is
/// exactly what an apply then writes.
pub fn app_apply(run: &Run, capabilities: &[Capability], intents: &[Intent]) -> Result<Outcome> {
    let project = run.project();
    let (request, reads) = declare(project, capabilities, intents)?;
    let applied = commit(
        run,
        request,
        &reads,
        &Asked::plain(
            CanonicalMutationRequest::AppApply {
                no_start: run.no_start(),
            },
            &["app", "apply"],
            &[],
        ),
    )?;
    // A manifest that declares `format` formats what the same apply generated.
    // The order matters and is why this is a second transition: the formatter
    // is a plugin the apply just installed, and the sources it has an opinion
    // about are the ones the apply just wrote.
    super::capability::reformat_after(run, capabilities.contains(&Capability::Format), applied)
}

/// The manifest as a request, and everything reading it declared.
fn declare(
    project: &Project,
    capabilities: &[Capability],
    intents: &[Intent],
) -> Result<(Request, ReadDeclaration)> {
    let store = observed(project)?;
    let mut declared: BTreeMap<EntityId, DesiredEntity> = BTreeMap::new();
    let mut changes: Vec<DesiredChange> = Vec::new();
    let mut reads = capture::capability_reads()?;

    for &capability in capabilities {
        let planned = super::projected_after(project, &reads, &changes)?;
        // The same resolution `add` performs, so a manifest row and a command
        // line naming one capability reach one identity. `app.toml` cannot yet
        // carry `--name`/`--package`; when it can, this is where they arrive.
        let (id, spec) = Declaration::plain(capability).resolve(&planned)?;
        let owner = ResourceOwner::Entity(EntityId::Capability(id.clone()));
        let change = with_test_support(
            &planned,
            jails_generate::add::plan_for(capability, &planned)?,
        );
        let mut desired = desire::contribution(&owner, &change, &planned)?;
        record_capability(&mut desired, &owner, &id, &spec)?;
        provenance::stamp_files(
            &mut desired,
            &planned,
            RendererId::Capability(capability),
            Some(RenderedSubjectContext::Entity {
                id: EntityId::Capability(id.clone()),
                spec: EntitySpec::Capability(spec.clone()),
            }),
        )?;
        declared.insert(
            EntityId::Capability(id.clone()),
            DesiredEntity {
                id: EntityId::Capability(id),
                spec: EntitySpec::Capability(spec),
                // The manifest is the owner, not `jails.toml`'s list: absence
                // *there* really does mean relinquished, which is what makes
                // a removed row a removal rather than silence.
                owners: BTreeSet::from([OwnerId::AppManifest]),
            },
        );
        reads = widen(reads, &planned, &change, &desired)?;
        changes.push(desired);
    }

    for intent in intents {
        let planned = super::projected_after(project, &reads, &changes)?;
        let expanded;
        let fields = match intent.timestamps {
            true => {
                expanded = jails_generate::generate::with_timestamps(intent.kind, &intent.fields)?;
                expanded.as_slice()
            }
            false => intent.fields.as_slice(),
        };
        let recipe = jails_generate::generate::Recipe {
            kind: intent.kind,
            name: &intent.name,
            fields,
            indexes: &intent.indexes,
            strategy_on: intent.on.as_deref(),
            strategy_yields: intent.yields.as_deref(),
            via: intent.via.as_deref(),
            order_by: intent.order_by.as_deref(),
            limit: intent.limit,
            on_conflict: intent.on_conflict.as_deref(),
            path: intent.path.as_deref(),
            method: intent.method,
            consumes: intent.consumes,
            select: intent.select.as_deref(),
            pins: &intent.set,
        };
        let change = with_test_support(
            &planned,
            jails_generate::generate::plan_recipe(&planned, &recipe, intent.package.as_deref())?,
        );
        let Declared { id, spec } = super::declared(&planned, &recipe, intent.package.as_deref())?;
        let mut change = change;
        evolve_declared_storage(
            &planned,
            &store,
            &id,
            &spec,
            intent.package.as_deref(),
            &mut change,
        )?;
        let owner = ResourceOwner::Entity(EntityId::Intent(id.clone()));
        let mut desired = desire::contribution(&owner, &change, &planned)?;
        provenance::stamp_files(
            &mut desired,
            &planned,
            RendererId::Recipe(intent.kind),
            Some(RenderedSubjectContext::Entity {
                id: EntityId::Intent(id.clone()),
                spec: EntitySpec::Intent(spec.clone()),
            }),
        )?;
        if declared
            .insert(
                EntityId::Intent(id.clone()),
                DesiredEntity {
                    id: EntityId::Intent(id.clone()),
                    spec: EntitySpec::Intent(spec),
                    owners: BTreeSet::from([OwnerId::AppManifest]),
                },
            )
            .is_some()
        {
            return Err(format!(
                "the manifest declares `{} {}` twice.\n       fix: one row per artifact. Two \
                 rows for one identity would apply both, and the second would land on the \
                 first's files.",
                label(intent.kind),
                id.name
            )
            .into());
        }
        reads = widen(reads, &planned, &change, &desired)?;
        changes.push(desired);
    }

    refuse_undeclared_storage(&store, &declared)?;

    // Everything the store already owns, so a row the manifest stopped naming
    // can be *retired*. A removal is a decision about what is there, and the
    // executor guards the preimage it deletes -- which is only meaningful if
    // the file was captured. `sync` declares the same set for the same
    // reason.
    for row in store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.resources.iter())
    {
        if let ResourceKey::WholeFile(path)
        | ResourceKey::SpringTestImport { path, .. }
        | ResourceKey::MarkedBlock { path, .. } = &row.key
        {
            reads = reads.file(path.clone());
        }
    }

    let request = Request {
        // Complete presence *and* absence for this owner. A capability or an
        // artifact the manifest no longer names is relinquished, which is
        // exactly what a declarative manifest means and what the per-intent
        // loop could never express.
        scope: ReconcileScope::AppManifest,
        declared,
        changes,
    };
    Ok((request, reads))
}

/// Everything the next step is allowed to look at, once this one has planned.
///
/// A path a change writes becomes a declared read for the steps after it,
/// because the capture is what they see it through -- and a path nobody
/// declared is a fact the plan may not use.
fn widen(
    reads: ReadDeclaration,
    project: &Project,
    change: &Change,
    desired: &DesiredChange,
) -> Result<ReadDeclaration> {
    let mut reads = reads;
    for artifact in &change.files {
        reads = reads.file(relative_path(project, &artifact.path)?);
    }
    for resource in &desired.resources {
        if let ResourceKey::SpringTestImport { path, .. } | ResourceKey::MarkedBlock { path, .. } =
            &resource.key
        {
            reads = reads.file(path.clone());
        }
    }
    Ok(reads)
}

/// Refuse to retire a table-backed resource that the manifest merely stopped
/// naming.
///
/// The imperative path insists on a storage policy: `jails destroy scaffold
/// Deal` refuses without `--storage preserve` or `--storage drop
/// --confirm-table deals`, because deleting the Java says nothing about what
/// happens to the rows. Deleting the `[[generate]]` block did the same removal
/// with no policy, no confirmation and no `drop table` migration -- the table
/// survived with no code that knows about it, and nothing reports an orphan.
/// The same intent, expressed two ways, got two different levels of care.
///
/// The manifest has no syntax for storage intent, so this does not invent one:
/// it names the command that does have it. Running that destroy retires the
/// row, after which the manifest and the store agree and `app apply` is a
/// no-op for that resource.
fn refuse_undeclared_storage(
    store: &ObservedStore,
    declared: &BTreeMap<EntityId, DesiredEntity>,
) -> Result<()> {
    let mut orphaned = Vec::new();
    for lifecycle in store.lifecycles() {
        let EntityId::Intent(id) = &lifecycle.entity else {
            continue;
        };
        let Some(table) = lifecycle.table.as_ref() else {
            continue;
        };
        if !matches!(
            lifecycle.state,
            jails_protocol::lifecycle::ResourceState::Active
        ) || declared.contains_key(&lifecycle.entity)
        {
            continue;
        }
        let claimed_by_manifest = store
            .entities()
            .iter()
            .any(|row| row.id == lifecycle.entity && row.owners.contains(&OwnerId::AppManifest));
        if claimed_by_manifest {
            orphaned.push((id.name.clone(), table.table.as_str().to_string()));
        }
    }
    let [(name, table)] = orphaned.as_slice() else {
        if orphaned.is_empty() {
            return Ok(());
        }
        let names = orphaned
            .iter()
            .map(|(name, table)| format!("`{name}` (table `{table}`)"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "storage-policy-required: the manifest no longer declares {names}, and the manifest \
             cannot say what happens to the rows.\n       fix: retire each one explicitly first \
             -- `jails destroy scaffold <Name> --storage preserve` keeps the table, `--storage \
             drop --confirm-table <table>` plans the data loss -- then re-run `jails app apply`."
        )
        .into());
    };
    Err(format!(
        "storage-policy-required: the manifest no longer declares `{name}`, which is backed by \
         table `{table}`, and the manifest cannot say what happens to the rows.\n       fix: \
         `jails destroy scaffold {name} --storage preserve` keeps the table, or `jails destroy \
         scaffold {name} --storage drop --confirm-table {table}` plans the data loss. Then \
         re-run `jails app apply`."
    )
    .into())
}

/// Turn an edited field list in a `[[generate]]` block into forward
/// migrations, instead of re-rendering the sealed `create table`.
///
/// Adding one field to a declared entity is the most common shape change
/// there is, and it was the one thing the declarative path could not express.
/// Re-planning the scaffold at the new list re-renders
/// `V001__create_deals.sql` with the extra column, the append-only seal
/// refuses it, and the offered fix -- "append the next migration for the
/// desired schema change" -- names something the manifest has no syntax for.
/// `jails resource field add` is not an escape either: it operates on the
/// imperative identity, so the manifest and the entity disagree about the
/// field list on the very next `app apply`.
///
/// So the create migration is kept exactly as sealed and the *delta* becomes
/// new `alter table ... add column` migrations -- the same SQL, and the same
/// version allocation through the projection, that `jails resource field add`
/// produces.
///
/// **Appending only.** A removed, renamed, retyped or reordered component is
/// not derivable from a list diff: dropping `amount` and adding `total` reads
/// identically to renaming it, and one of those destroys data. Those are
/// refused by name, pointing at the verbs that take the intent explicitly.
fn evolve_declared_storage(
    project: &Project,
    store: &ObservedStore,
    id: &jails_protocol::entity::IntentId,
    spec: &IntentSpec,
    package: Option<&str>,
    change: &mut jails_project::model::Change,
) -> Result<()> {
    let entity = EntityId::Intent(id.clone());
    let Some(lifecycle) = store
        .lifecycles()
        .iter()
        .find(|lifecycle| lifecycle.entity == entity)
    else {
        return Ok(());
    };
    let Some(table) = lifecycle.table.as_ref() else {
        return Ok(());
    };
    let EntitySpec::Intent(recorded) = &lifecycle.last_spec else {
        return Ok(());
    };
    let before = recorded.fields();
    let after = spec.fields();
    if before
        .iter()
        .map(FieldSpec::canonical)
        .eq(after.iter().map(FieldSpec::canonical))
        && recorded.indexes == spec.indexes
    {
        return Ok(());
    }
    let appended = after.len() > before.len()
        && before
            .iter()
            .zip(after)
            .all(|(held, wanted)| held.canonical() == wanted.canonical());
    if !appended || recorded.indexes != spec.indexes {
        return Err(format!(
            "the manifest changes `{}`'s existing shape, and a list diff cannot say which change \
             it is -- dropping one component and adding another reads exactly like renaming it, \
             and one of those destroys data.\n       fix: state it explicitly with `jails \
             resource field rename|type|nullability|drop {}`, then bring the manifest's `fields` \
             back into line. Appending a component to the end is the shape `app apply` can \
             derive on its own.",
            id.name, id.name
        )
        .into());
    }

    // The sealed create migration is kept exactly as it is. Every other
    // projection re-renders at the new spec, which is what carries the added
    // component into the record, the DTOs, the adapter and the fixtures.
    change.files.retain(|artifact| {
        !artifact
            .path
            .strip_prefix(project.root())
            .is_ok_and(|path| {
                path.to_string_lossy()
                    .replace('\\', "/")
                    .starts_with("src/main/resources/db/migration/")
            })
    });

    let domain = project.package_named(jails_spec::spec::layout::DOMAIN, package);
    for added in &after[before.len()..] {
        let column = jails_generate::sql::columns(
            &[added.projected()?],
            project,
            &domain,
            &jails_generate::generate::lower_first(id.name.as_str()),
        )
        .pop()
        .expect("one field produces one column");
        // A required component with no default cannot be added to a table
        // that may already hold rows, and the manifest has nowhere to put a
        // backfill. Say so against the component rather than letting Flyway
        // discover it.
        if added.optionality != jails_protocol::declaration::Optionality::Nullable {
            return Err(format!(
                "`{}` adds required component `{}` to table `{}`, and existing rows have no \
                 value for it.\n       fix: declare it optional (`{}?`) in the manifest, or add \
                 it with `jails resource field add {} {} --default-literal <value>` and then \
                 record it in `fields`.",
                id.name,
                added.name,
                table.table.as_str(),
                added.canonical(),
                id.name,
                added.canonical(),
            )
            .into());
        }
        let body = jails_generate::sql::add_column(id.name.as_str(), &column)?;
        let path = jails_generate::generate::migration_path(
            project,
            &format!("add_{}_to_{}", column.name, table.table.as_str()),
        )?;
        change.files.push(jails_project::model::Artifact {
            kind: "migration",
            path,
            contents: body,
        });
    }
    Ok(())
}
