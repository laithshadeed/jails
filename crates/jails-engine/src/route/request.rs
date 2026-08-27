//! Assembling one request: from what was typed to what the store should become.
//!
//! The half of `route` that runs *before* anything is committed. A [`Request`]
//! measures a desired shape against the observed store and says what the store
//! becomes; [`Asked`] carries what the reader actually typed, so a refusal can
//! quote it.
//!
//! Split from [`super::commit`] along the seam `pending.md` §8.1 names: this
//! module decides, that one drives. Nothing here writes, starts a subprocess or
//! touches `.jails/` — which is why every one of these functions is testable
//! against a `Project` and a store with no transaction in sight.

use super::*;

mod naming;

pub(super) use naming::{refuse_java_lang_shadow, refuse_reserved_variable};

/// `(recipe, name, resolved package)` — the identity everything about this
/// artifact is filed under.
///
/// The package is resolved rather than optional: two rows for one artifact,
/// one saying "wherever the convention puts it" and one naming the package it
/// went to, are two authorities for one identity.
pub(super) fn identity(
    project: &Project,
    kind: ArtifactKind,
    name: &str,
    package: Option<&str>,
) -> Result<IntentId> {
    let canonical = jails_generate::generate::strip_redundant_suffix(
        kind,
        &jails_spec::spec::field::capitalize(name),
    );
    let canonical = Name::parse(&canonical).map_err(|error| {
        format!(
            "{error}.\n       fix: choose an entity name that is a valid Java identifier, such as `Order`."
        )
    })?;
    if kind == ArtifactKind::Scaffold {
        let table = jails_protocol::identity::SqlName::conventional_table(&canonical);
        if jails_protocol::identity::SqlName::is_postgres_reserved(table.as_str()) {
            return Err(format!(
                "entity name `{name}` derives PostgreSQL table `{}`, which is reserved.\n       \
                 fix: choose a domain-specific entity name whose plural table is not a PostgreSQL keyword.",
                table.as_str()
            )
            .into());
        }
    }
    Ok(IntentId {
        recipe: kind,
        name: canonical,
        package: Package::parse(&project.package_named("", package))?,
    })
}

/// One artifact's identity and everything it was declared to be.
///
/// `pending.md` §6.2. These were two functions taking eight and seven
/// positional arguments, and both call sites — `generate` and the manifest
/// loop — passed each of them the same values off the same [`Recipe`] they had
/// just built. Four of the eight were unused and had been for long enough to
/// have grown `_` prefixes.
///
/// Parsing them together is what removes the second parse: translating
/// `--index created_at` into the field it names needs the fields, so the
/// arguments had to be parsed once for the translation and then again inside
/// `IntentSpec::parse`. `IntentSpec::from_arguments` takes the parsed value
/// instead, so a declaration is read exactly once per request.
pub(super) struct Declared {
    pub(super) id: IntentId,
    pub(super) spec: IntentSpec,
}

pub(super) fn declared(
    project: &Project,
    recipe: &Recipe<'_>,
    package: Option<&str>,
) -> Result<Declared> {
    let base = Package::parse(project.base())?;
    let arguments = IntentArguments::parse(recipe.kind, recipe.fields, &base)?;
    let translated: Vec<String> = recipe
        .indexes
        .iter()
        .map(|index| as_field_names(index, arguments.fields()))
        .collect();
    let mut spec = IntentSpec::from_arguments(
        recipe.kind,
        arguments,
        &translated,
        // `--timestamps` is expanded into fields before a recipe ever sees it,
        // so by the time there is a spec the two extra components are ordinary
        // ones. Recording it again would make one request two facts.
        false,
    )?;
    spec.on = recipe.strategy_on.map(JavaType::parse).transpose()?;
    spec.yields = recipe.strategy_yields.map(JavaType::parse).transpose()?;
    spec.via = recipe.via.map(JavaType::parse).transpose()?;
    spec.order_by = recipe
        .order_by
        .map(|token| {
            jails_protocol::declaration::IndexSpec::parse_columns(token).map(|spec| spec.columns)
        })
        .transpose()?
        .unwrap_or_default();
    spec.limit = recipe.limit;
    spec.on_conflict = recipe
        .on_conflict
        .map(jails_protocol::identity::Name::parse)
        .transpose()?;
    spec.path = recipe
        .path
        .map(jails_protocol::identity::RoutePath::parse)
        .transpose()?;
    // Recorded, not applied: an intent regenerated with a different verb is
    // the *same* entity with new content, which is what makes it an edit the
    // three-way merge can carry rather than an orphan and a rewrite.
    spec.method = recipe.method;
    spec.consumes = recipe.consumes;
    Ok(Declared {
        id: identity(project, recipe.kind, recipe.name, package)?,
        spec,
    })
}

/// An `--index` token as the RFC's canonical spelling.
///
/// `IndexSpec` names *fields*, which plan.md §R1.1 fixes deliberately -- the
/// column name is derived, and a spec that stored the derived form would be a
/// second authority on it. But the shipped CLI spelling is the column:
/// `--index "created_at desc"` is what `README.md` documents and what every
/// scenario types, because that is the name the reader sees in the DDL.
///
/// So the column spelling is translated here, at the boundary, rather than
/// either spelling being taught to the protocol or the CLI being changed
/// under people. A token that already names a field passes through untouched,
/// and one that names neither is left exactly as typed so `IndexSpec::parse`
/// produces the refusal that lists the declared fields.
pub(super) fn as_field_names(token: &str, fields: &[FieldSpec]) -> String {
    token
        .split(',')
        .map(|part| {
            let mut words = part.split_whitespace();
            let Some(first) = words.next() else {
                return String::new();
            };
            let named = fields.iter().find(|field| {
                field.name.as_str() != first
                    && jails_generate::sql::snake_case(field.name.as_str()) == first
            });
            let rest: Vec<&str> = words.collect();
            let head = named.map_or(first, |field| field.name.as_str());
            if rest.is_empty() {
                head.to_string()
            } else {
                format!("{head} {}", rest.join(" "))
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A recorded index as the columns PostgreSQL wants: [`as_field_names`]'s
/// inverse, and the reason it has to exist.
///
/// `IndexSpec` names **fields** -- plan.md §R1.1 fixes that deliberately,
/// because the column is derived and a spec storing the derived form would be
/// a second authority on it. So every path that hands a recorded index to a
/// generator has to render it back, and until this existed one of them did not:
/// `app apply` passed the manifest's own column tokens while a re-plan passed
/// `IndexSpec::canonical()`, which is camelCase. The create migration is
/// one-shot, so the camelCase spelling only surfaced when
/// `resource index add` re-planned a scaffold and `validate_index` reported
/// "no column 'customerId' in this table" over a table that has it.
///
/// The column comes from the field's own recorded binding, not from
/// `snake_case`: a `@column(...)` override is exactly the case where the two
/// differ.
pub(super) fn as_column_names(
    index: &jails_protocol::declaration::IndexSpec,
    fields: &[FieldSpec],
) -> String {
    index
        .columns
        .iter()
        .map(|column| {
            let name = fields
                .iter()
                .find(|field| field.name == column.field)
                .map(|field| field.name.column().as_str().to_string())
                .unwrap_or_else(|| jails_generate::sql::snake_case(column.field.as_str()));
            match column.direction {
                jails_protocol::declaration::IndexDirection::Ascending => name,
                jails_protocol::declaration::IndexDirection::Descending => format!("{name} desc"),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Record the capability in the manifest `sync` acts on.
///
/// CLAUDE.md states the rule and the reason: a manifest somebody has to
/// remember to update is a manifest that is wrong, and a wrong one is worse
/// than none because `sync` acts on it. It is a resource rather than a side
/// effect, so removing the capability takes the line out by the same
/// mechanism that put it in.
pub(super) fn record_capability(
    change: &mut DesiredChange,
    owner: &ResourceOwner,
    id: &CapabilityId,
    spec: &CapabilitySpec,
) -> Result<()> {
    let key = ResourceKey::HumanConfigCapability(id.clone());
    let spec = spec.clone();
    change.resources.push(DesiredResource::new(
        key.clone(),
        BTreeSet::from([owner.clone()]),
        ResourceValue::HumanConfigCapability(spec.clone()),
    )?);
    change
        .edits
        .push(SemanticEdit::HumanConfigCapability { key, spec });
    Ok(())
}

/// What a removal is allowed to read: the format owners, plus every file this
/// owner is about to give up.
///
/// A file is deleted against a *guarded preimage*, and the guard is only
/// meaningful if the file was captured. Declaring the ones that are leaving --
/// rather than every file jails has ever written -- keeps the preconditions to
/// what this request actually depends on, so an unrelated generated file
/// changing does not make the removal refuse.
pub(super) fn retiring(store: &ObservedStore, owner: &ResourceOwner) -> Result<ReadDeclaration> {
    let mut declaration = capture::capability_reads()?;
    for row in store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.resources.iter())
    {
        // Shared resources are not retired, but their current image can still
        // be an invariant of the transition. Migration history is shared with
        // `SchemaHistory`: destroying the entity must capture and verify those
        // bytes even though the history owner keeps the file alive.
        // `contains` is not the whole rule. A field overlay's migration is
        // owned by its own one-shot, and the seal walk still resolves that
        // one-shot back to the entity being retired -- so planning reads the
        // file whether or not the entity is listed on the row. Declaring only
        // the direct rows left `destroy scaffold Note` planning against a
        // `V002__add_..._to_notes.sql` it had not captured, and refusing its
        // own request.
        let claimed = row.owners.contains(owner)
            || matches!(owner, ResourceOwner::Entity(entity)
                if row.owners.iter().any(|held| held.names_entity(entity)));
        if !claimed {
            continue;
        }
        match &row.key {
            ResourceKey::WholeFile(path) => declaration = declaration.file(path.clone()),
            // A surgical edit is undone in the file that holds it, which is a
            // file this owner does not own -- so it has to be declared
            // separately from the ones it does. `add db` splices `@Import`
            // into a test the reader wrote; the retirement reads that test
            // back.
            ResourceKey::SpringTestImport { path, .. } | ResourceKey::MarkedBlock { path, .. } => {
                declaration = declaration.file(path.clone())
            }
            ResourceKey::CommandRegistration { dispatcher, .. } => {
                declaration = declaration.file(dispatcher_source(dispatcher)?)
            }
            _ => {}
        }
    }
    Ok(declaration)
}

pub(super) fn recorded_migrations(
    store: &ObservedStore,
    target: &IntentId,
) -> BTreeSet<ProjectPath> {
    let lifecycle_paths = store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.lifecycles.iter())
        .filter(|lifecycle| lifecycle.entity == EntityId::Intent(target.clone()))
        .flat_map(|lifecycle| lifecycle.migrations.iter().map(|seal| seal.path.clone()));
    let owned_paths = store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.resources.iter())
        .filter(|row| {
            row.owners.iter().any(|owner| match owner {
                ResourceOwner::Entity(EntityId::Intent(owner)) => owner == target,
                ResourceOwner::OneShot(jails_protocol::entity::OneShotId::Field {
                    target: jails_protocol::entity::TypeTargetId::Managed(owner),
                    ..
                }) => owner == target,
                _ => false,
            })
        })
        .filter_map(|row| match &row.key {
            ResourceKey::WholeFile(path) if row.key.is_migration_history() => Some(path.clone()),
            _ => None,
        });
    lifecycle_paths.chain(owned_paths).collect()
}

/// Every capability `jails.toml`'s scope currently declares, with `changed`
/// applied to it.
///
/// `DirectConfig` speaks for the *whole* capability list, so a request that
/// declared only the capability it is installing would be saying every other
/// capability is no longer wanted -- and the reconciler would dutifully
/// retire them. Passing `None` for `changed` is how a removal says one is
/// gone.
pub(super) fn declared_capabilities(
    observed: &ObservedStore,
    changed: Option<DesiredEntity>,
) -> Result<BTreeMap<EntityId, DesiredEntity>> {
    let mut declared = BTreeMap::new();
    if let Some(store) = &observed.ledger {
        for row in &store.applied {
            if !matches!(row.id, EntityId::Capability(_))
                || !row.owners.contains(&OwnerId::DirectConfig)
            {
                continue;
            }
            declared.insert(
                row.id.clone(),
                DesiredEntity {
                    id: row.id.clone(),
                    spec: row.version.spec.clone(),
                    owners: BTreeSet::from([OwnerId::DirectConfig]),
                },
            );
        }
    }
    if let Some(entity) = changed {
        declared.insert(entity.id.clone(), entity);
    }
    Ok(declared)
}

/// One request, before it is measured against the store.
///
/// `Clone` because §R3.4's replan-once loop measures the same request against
/// the store twice when recovery moves it in between -- the *request* is what
/// was asked and does not change; only what the store makes of it does.
///
/// Deliberately not a `DesiredChangeSet` yet. That value states what the store
/// looks like afterwards, and afterwards is a function of what is there now --
/// which exactly one place may read (see [`commit`]). A field filled with a
/// placeholder here and corrected there is two authorities on one number, and
/// the executor refuses when they disagree.
#[derive(Clone)]
pub(super) struct Request {
    pub(super) scope: ReconcileScope,
    /// What this scope declares. Empty is a real declaration: it says this
    /// scope wants nothing, which is how a removal is expressed.
    pub(super) declared: BTreeMap<EntityId, DesiredEntity>,
    /// One per entity that has something to install. A `sync` that brings two
    /// capabilities in has two, and they commit together or not at all.
    pub(super) changes: Vec<DesiredChange>,
}

impl Request {
    /// Measure this request against the store, and say what the store becomes.
    ///
    /// The reconciliation is [`jails_protocol::ownership::reconcile`]'s, not a
    /// second copy of it: a scope may only add or remove *its own* claim, an
    /// owner outside the scope is carried forward untouched, and an entity
    /// whose last owner leaves is removed. What is left here is projecting
    /// that answer onto the resource rows -- a resource loses the owners whose
    /// entities went, and a resource nobody claims any more is retired.
    pub(super) fn against(self, observed: &ObservedStore) -> Result<DesiredChangeSet> {
        let recorded = observed.ledger.as_ref();
        let applied: BTreeMap<EntityId, ObservedEntity> = recorded
            .map(|store| {
                store
                    .applied
                    .iter()
                    .map(|row| {
                        (
                            row.id.clone(),
                            ObservedEntity {
                                spec: row.version.spec.clone(),
                                owners: row.owners.clone(),
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let references = reference_edges(&applied);
        let reconciled = jails_protocol::ownership::reconcile(
            &self.scope,
            &self.declared,
            &applied,
            &references,
        )?;

        let entities_after = reconciled
            .entities
            .iter()
            .map(|(id, entity)| DesiredAppliedEntity {
                id: id.clone(),
                owners: entity.owners.clone(),
                spec: entity.spec.clone(),
            })
            .collect();
        let gone: BTreeSet<ResourceOwner> = reconciled
            .removed
            .iter()
            .map(|id| ResourceOwner::Entity(id.clone()))
            .collect();

        // Which resources this transition leaves unowned. Computed here only
        // to decide what has to come *out of the files*; the store derives the
        // same answer from `entities_removed`, so the two cannot disagree
        // about which rows survive.
        let mut changes = self.changes;
        let mut retirement: BTreeMap<ResourceOwner, DesiredChange> = BTreeMap::new();
        for row in recorded
            .map(|store| store.resources.as_slice())
            .unwrap_or(&[])
        {
            // A published migration is schema history, not a generated
            // projection. The durable ledger upgrade below adds the
            // SchemaHistory owner; skipping retirement here also protects
            // ledgers written before that owner tag existed.
            if row.key.is_migration_history() {
                continue;
            }
            if row.owners.iter().any(|owner| !gone.contains(owner)) {
                continue;
            }
            // Charged to one of the owners that is leaving, because that is
            // what a change *is* here: work an owner is responsible for. A
            // maintenance attribution would be a lie about who asked, and the
            // change set refuses it under this subject for exactly that
            // reason. The lowest owner is picked so two runs of one removal
            // produce the same transaction.
            let Some(owner) = row.owners.iter().next().cloned() else {
                continue;
            };
            let change = retirement
                .entry(owner.clone())
                .or_insert_with(|| DesiredChange::owned_by(owner));
            match &row.key {
                // A whole file leaves as an absence rather than an edit: the
                // executor guards the preimage it deletes, which an edit
                // cannot do.
                ResourceKey::WholeFile(path) => change.absences.push(ManagedPath {
                    path: path.clone(),
                    resource: row.key.clone(),
                    force: false,
                }),
                _ => change.edits.push(SemanticEdit::Retire {
                    key: row.key.clone(),
                }),
            }
        }
        changes.extend(retirement.into_values());

        // Exactly the claims these changes make, merged the way the projection
        // merges them: one row per key, owners unioned. `require_intent_
        // matches` holds the intent to saying the same thing the changes do.
        let mut merged: BTreeMap<ResourceKey, DesiredResource> = BTreeMap::new();
        for change in &changes {
            for desired in &change.resources {
                match merged.get_mut(&desired.key) {
                    Some(row) => row.owners.extend(desired.owners.iter().cloned()),
                    None => {
                        merged.insert(desired.key.clone(), desired.clone());
                    }
                }
            }
        }
        let resources_after: Vec<DesiredResource> = merged.into_values().collect();

        let set = DesiredChangeSet {
            ledger_intent: LedgerIntent {
                generation_before: observed.generation(),
                entities_after,
                one_shots_after: Vec::new(),
                resources_after,
                entities_removed: reconciled.removed,
            },
            ordered: changes,
            subject: PlannedSubject::Reconcile(DesiredState::new(self.scope, self.declared)?),
        };
        set.validate()?;
        Ok(set)
    }
}

/// Typed references already recorded in entity specs.
///
/// This deliberately derives from the durable declaration rather than Java
/// text. Project field types and `--on`/`--yields` are the facts that make one
/// generated entity stop compiling when another disappears; passing an empty
/// graph to ownership reconciliation made its last-owner guard unreachable.
fn reference_edges(applied: &BTreeMap<EntityId, ObservedEntity>) -> Vec<(EntityId, EntityId)> {
    let intents = applied
        .iter()
        .filter_map(|(entity, observed)| match (entity, &observed.spec) {
            (EntityId::Intent(id), EntitySpec::Intent(spec)) => Some((entity, id, spec)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut edges = Vec::new();
    for (source, source_id, spec) in &intents {
        let mut targets = Vec::new();
        if let Some(target) = &spec.on {
            targets.push(target);
        }
        if let Some(target) = &spec.yields {
            targets.push(target);
        }
        for field in spec.fields() {
            use jails_protocol::declaration::{FieldType, ScalarFieldType};
            let scalars: Vec<&ScalarFieldType> = match &field.field_type {
                FieldType::Scalar(scalar) | FieldType::List(scalar) => vec![scalar],
                FieldType::Map { key, value } => vec![key, value],
            };
            for scalar in scalars {
                if let ScalarFieldType::Project(target) = scalar {
                    targets.push(target);
                }
            }
        }
        for target_type in targets {
            for (target, target_id, _) in &intents {
                if *source != *target && target_id.name == *target_type.name() {
                    edges.push(((*source).clone(), (*target).clone()));
                }
            }
        }
        // A scaffold installs a JDBC adapter and its database integration
        // test. If `db` is separately declared, removing that capability must
        // not retire the dependencies those generated sources still import.
        if source_id.recipe == jails_protocol::entity::Recipe::Scaffold {
            for target in applied.keys() {
                if matches!(
                    target,
                    EntityId::Capability(id)
                        if id.kind == jails_spec::spec::kind::Capability::Db
                ) {
                    edges.push(((*source).clone(), target.clone()));
                }
            }
        }
    }
    edges.sort();
    edges.dedup();
    edges
}

/// What this request is allowed to read: the format owners, plus every file it
/// intends to write, plus every file it intends to edit surgically.
///
/// A file it writes has to be declared too, because writing one is a decision
/// about what was there — and "there was nothing there" is exactly the kind of
/// fact the executor rechecks under the lock.
///
/// The `desired` half is what makes a surgical edit safe. `add db` splices
/// `@Import` into every `@SpringBootTest` it finds, and *which tests exist* is
/// read while planning. Declaring each one turns that read into a
/// precondition, so a test added between the plan and the commit makes this
/// refuse rather than silently miss it.
pub(super) fn declaration(
    project: &Project,
    change: &Change,
    desired: &DesiredChange,
) -> Result<ReadDeclaration> {
    let mut declaration = capture::capability_reads()?;
    for artifact in &change.files {
        declaration = declaration.file(relative_path(project, &artifact.path)?);
    }
    for resource in &desired.resources {
        match &resource.key {
            // A surgical edit is made *in* a file this change does not own, so
            // the file is a precondition of the plan rather than an incidental
            // read: the executor rechecks it under the lock, and a block
            // spliced into bytes that moved in between would be spliced into
            // the wrong place.
            ResourceKey::SpringTestImport { path, .. } | ResourceKey::MarkedBlock { path, .. } => {
                declaration = declaration.file(path.clone());
            }
            ResourceKey::CommandRegistration { dispatcher, .. } => {
                declaration = declaration.file(dispatcher_source(dispatcher)?);
            }
            _ => {}
        }
    }
    Ok(declaration)
}

/// Where a dispatcher's source lives, by the convention every Java build
/// follows. The same derivation the projection uses, so a read and the splice
/// that depends on it cannot name two files.
pub(super) fn dispatcher_source(ty: &jails_protocol::identity::JavaType) -> Result<ProjectPath> {
    let package = ty.package();
    let directory = match package.is_base() {
        true => String::new(),
        false => format!("{}/", package.as_str().replace('.', "/")),
    };
    ProjectPath::parse(&format!("src/main/java/{directory}{}.java", ty.name()))
}

/// What was asked for, canonically -- both halves of §R5.4's invocation.
///
/// The two are not redundant. `request` is the *meaning*: which capabilities,
/// which recipe, which force flag, with aliases resolved and set-valued
/// positions sorted. `syntax` is the *spelling*, and it is what a resume
/// compares first, because two different spellings of one meaning are still
/// two different things a person typed and a resumption that silently
/// accepted either would be resuming the wrong one.
///
/// Built by the route rather than parsed out of `argv`. A route knows what it
/// was asked far more exactly than a parser reading the command line back
/// does, and there is no second implementation to disagree with.
pub(super) struct Asked {
    request: CanonicalMutationRequest,
    syntax: CanonicalRequestSyntaxV1,
}

impl Asked {
    /// Name the command and the arguments that decide what it does.
    ///
    /// `command` is the subcommand path without dashes; `positionals` are its
    /// arguments; `options`/`flags` carry only what was explicitly supplied
    /// and only what is *semantic* -- §R5.4 excludes presentation flags
    /// (`--debug`, an output format) because rerunning with colour on is the
    /// same request.
    pub fn new(
        request: CanonicalMutationRequest,
        command: &[&str],
        positionals: Vec<String>,
        options: BTreeMap<String, Vec<String>>,
        flags: BTreeSet<String>,
    ) -> Self {
        Self {
            request,
            syntax: CanonicalRequestSyntaxV1 {
                command_path: command.iter().map(|part| part.to_string()).collect(),
                positionals,
                options,
                flags,
            },
        }
    }

    /// The shorter form: a command with positional arguments and nothing else.
    pub fn plain(
        request: CanonicalMutationRequest,
        command: &[&str],
        positionals: &[&str],
    ) -> Self {
        Self::new(
            request,
            command,
            positionals.iter().map(|one| one.to_string()).collect(),
            BTreeMap::new(),
            BTreeSet::new(),
        )
    }

    /// The line a lock, a report and a resume prompt all show.
    pub(super) fn display(&self) -> String {
        let mut out = String::from("jails");
        for part in self
            .syntax
            .command_path
            .iter()
            .chain(self.syntax.positionals.iter())
        {
            out.push(' ');
            out.push_str(part);
        }
        out
    }

    pub(super) fn syntax_fingerprint(
        &self,
    ) -> Result<jails_protocol::request::RequestSyntaxFingerprint> {
        self.syntax.fingerprint()
    }

    /// Whether this request is one that may reconcile runtime services.
    ///
    /// §R3.3: only the variants carrying a `no_start` field are eligible, and
    /// every other one behaves as though it had said `--no-start`. Read off
    /// the canonical request rather than off a flag, so the answer is a
    /// property of what was asked rather than of what a caller remembered to
    /// pass.
    pub(super) fn starts_services(&self) -> bool {
        match &self.request {
            CanonicalMutationRequest::Add { no_start, .. }
            | CanonicalMutationRequest::Remove { no_start, .. }
            | CanonicalMutationRequest::Sync { no_start }
            | CanonicalMutationRequest::AppApply { no_start } => !no_start,
            _ => false,
        }
    }

    /// §R5.4's fingerprint, over this request and the human inputs it reads.
    ///
    /// `DirectRequest` is mandatory and hashes the request's own canonical
    /// bytes, which is what makes the fingerprint depend on *what was asked*
    /// rather than only on how it was spelled. The other rows are the human
    /// sources a resumption must find unchanged; `jails.toml` is the one every
    /// route may touch, and its absence is a row too -- "there was no config"
    /// is a fact a resume has to be able to check, not a gap.
    pub(super) fn fingerprint(
        &self,
        snapshot: &jails_protocol::snapshot::ProjectSnapshot,
    ) -> Result<jails_protocol::request::InvocationFingerprint> {
        let mut rows = vec![FrozenDesiredInput {
            id: DesiredInputId::DirectRequest,
            guard: {
                let mut encoder = jails_support::codec::Encoder::new();
                self.request.encode(&mut encoder)?;
                let bytes = encoder.finish()?;
                DesiredInputGuard::Exact {
                    sha256: ObjectId::from_bytes(jails_support::codec::sha256(&bytes)),
                    len: bytes.len() as u64,
                }
            },
        }];
        let config = ProjectPath::parse(jails_project::config::FILE)?;
        rows.push(FrozenDesiredInput {
            id: DesiredInputId::HumanConfig,
            guard: match snapshot.read(&config)? {
                jails_protocol::snapshot::Captured::Present(file) => DesiredInputGuard::Exact {
                    sha256: ObjectId::from_bytes(jails_support::codec::sha256(&file.bytes)),
                    len: file.bytes.len() as u64,
                },
                jails_protocol::snapshot::Captured::Absent => DesiredInputGuard::Absent,
            },
        });
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        let mut encoder = jails_support::codec::Encoder::new();
        encoder.count(rows.len())?;
        for row in &rows {
            row.encode(&mut encoder)?;
        }
        Ok(jails_protocol::request::InvocationFingerprint {
            request_syntax: self.syntax.fingerprint()?,
            request: self.request.clone(),
            // Direct CLI: no manifest is the source. `app apply` overrides
            // this once a manifest identity is threaded through.
            manifest_source: None,
            desired_input_sha256: ObjectId::from_bytes(jails_support::codec::domain_hash(
                "JAILS-DESIRED-INPUT-1",
                &encoder.finish()?,
            )),
        })
    }
}

/// The `Asked` for a command whose whole argument is one capability.
///
/// The three capability routes share it because they share the shape: one
/// name, spelled as `Capability::label()` rather than whatever alias was
/// typed, so `jails add postgres` and `jails add db` are recognised as the
/// same request by anything comparing fingerprints.
/// The canonical syntax of a capability command, parameters included.
///
/// A fingerprint proves two invocations are the same command, so `add csv
/// --name Order` and `add csv --name Invoice` must not render the same
/// syntax. Built here from the resolved declaration rather than re-parsed out
/// of `argv`, for the reason §R6.1 gives: a route knows what it was asked far
/// more exactly than a re-parse does, and there is no second implementation to
/// disagree with.
pub(super) fn asked_capabilities(
    command: &[&str],
    declaration: &Declaration,
    request: CanonicalMutationRequest,
) -> Asked {
    let mut syntax: Vec<String> = vec![declaration.kind.label().to_string()];
    if let Some(name) = &declaration.name {
        syntax.push("--name".to_string());
        syntax.push(name.clone());
    }
    if let Some(package) = &declaration.package {
        syntax.push("--package".to_string());
        syntax.push(package.clone());
    }
    let syntax: Vec<&str> = syntax.iter().map(String::as_str).collect();
    Asked::plain(request, command, &syntax)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_protocol::identity::Package;

    fn fields(tokens: &[&str]) -> Vec<FieldSpec> {
        let base = Package::parse("com.example.demo").unwrap();
        tokens
            .iter()
            .map(|token| FieldSpec::parse(token, &base).unwrap())
            .collect()
    }

    #[test]
    fn an_index_named_in_column_spelling_is_translated_to_the_field_it_means() {
        let fields = fields(&["createdAt:instant", "name:string"]);
        assert_eq!(as_field_names("created_at", &fields), "createdAt");
        assert_eq!(as_field_names("name", &fields), "name");
    }

    #[test]
    fn an_ordering_survives_the_translation() {
        let fields = fields(&["createdAt:instant"]);
        assert_eq!(as_field_names("created_at desc", &fields), "createdAt desc");
    }

    #[test]
    fn a_composite_index_translates_every_part_and_keeps_the_order() {
        let fields = fields(&["tenantId:string", "createdAt:instant"]);
        assert_eq!(
            as_field_names("tenant_id, created_at desc", &fields),
            "tenantId, createdAt desc"
        );
    }

    /// A token naming neither a field nor a column is left exactly as typed.
    ///
    /// Rewriting it into something plausible here would rob `IndexSpec::parse`
    /// of the refusal that lists the fields that *are* declared, which is the
    /// only message a reader can act on.
    #[test]
    fn a_token_that_names_nothing_is_passed_through_untouched() {
        let fields = fields(&["name:string"]);
        assert_eq!(as_field_names("nonesuch", &fields), "nonesuch");
    }

    #[test]
    fn a_dispatcher_in_the_base_package_gets_no_directory() {
        let ty = jails_protocol::identity::JavaType::parse("App").unwrap();
        assert_eq!(
            dispatcher_source(&ty).unwrap().as_str(),
            "src/main/java/App.java"
        );
    }

    #[test]
    fn a_dispatcher_in_a_package_lands_under_the_directory_that_package_names() {
        let ty =
            jails_protocol::identity::JavaType::parse("com.example.demo.cli.AdminCli").unwrap();
        assert_eq!(
            dispatcher_source(&ty).unwrap().as_str(),
            "src/main/java/com/example/demo/cli/AdminCli.java"
        );
    }
}
