//! What the project will look like after the changes planned so far — without
//! writing anything.
//!
//! ## The problem this solves
//!
//! `app apply` runs several intents in one invocation, and a later one has to
//! see what an earlier one did: a repository that needs the record the
//! previous intent generated, a capability that needs the dependency the
//! previous change spliced. Reading the filesystem between them is how that
//! works today, and it is exactly what R2 removes — planning reads the
//! snapshot, and the snapshot is captured once.
//!
//! So the snapshot gets an *overlay*. A read goes through the overlay first
//! and falls back to the captured base, and the answer is the project as the
//! plan will leave it rather than as it currently is.
//!
//! ## Why a render stays deferred
//!
//! plan.md §R2.4: *"Never eagerly render a template here and render it again
//! in preparation."* Rendering twice means two chances to differ, and the
//! second one is the one that reaches disk. So a `DesiredBody::Render` enters
//! the overlay as [`ProjectedEntry::Deferred`] and R3 renders it exactly once.
//! A planner that tries to read those bytes gets an error naming the reason,
//! not a silent empty file.
//!
//! ## Why facts are invalidated by path
//!
//! Each `FactKind` records which paths it was parsed from. A change that
//! touches one of those paths invalidates that kind, and the kind is reparsed
//! from the *projected* bytes. Without the dependency map a deleted POM would
//! leave its dependency facts in place, and every later planner would decide
//! against a file that no longer exists.
//!
//! ## What this module may not do
//!
//! No formatter, no subprocess, no filesystem. Projection is pure, and the
//! only reason it can be trusted to produce the same plan twice is that it has
//! nothing else to read.

use crate::pom::{self, DependencyRef, Flavor};
use crate::properties;
use jails_protocol::change::DesiredChange;
use jails_protocol::conflict::FileMode;
use jails_protocol::edit::SemanticEdit;
use jails_protocol::fact::{FactKind, FactSourceState, ProjectFact, ProjectFactKey, ProjectFacts};
use jails_protocol::identity::{ObjectId, Package, ProjectPath};
use jails_protocol::render::DesiredBody;
use jails_protocol::resource::{ProjectedResource, ResourceKey, ResourceOwner, ResourceValue};
use jails_protocol::snapshot::{Captured, ProjectSnapshot, SnapshotFile};
use jails_spec::build::Build;
use jails_support::Result;
use jails_support::codec::sha256;
use jails_support::codemod::Marked;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// One path's projected state.
#[derive(Clone, Debug)]
pub enum ProjectedEntry {
    File(SnapshotFile),
    /// A render R3 performs exactly once. Its facts are declared now because a
    /// later planner may need them; its bytes are not knowable yet.
    Deferred {
        body: DesiredBody,
        facts: jails_protocol::edit::FactDelta,
    },
    Deleted,
}

/// The project as the plan will leave it.
#[derive(Clone, Debug)]
pub struct ProjectedProject {
    base: Arc<ProjectSnapshot>,
    overlay: BTreeMap<ProjectPath, ProjectedEntry>,
    resources: BTreeMap<ResourceKey, ProjectedResource>,
    /// What the store already recorded, read only by retirement.
    recorded: BTreeMap<ResourceKey, ResourceValue>,
    facts: ProjectFacts,
    build: Build,
    base_package: Package,
    java_release: u32,
    flavor: Option<Flavor>,
    fact_dependencies: BTreeMap<FactKind, BTreeSet<ProjectPath>>,
}

/// What a projected read found.
#[derive(Clone, Debug)]
pub enum Projected<'a> {
    Present(&'a SnapshotFile),
    Absent,
}

impl ProjectedProject {
    pub fn new(
        base: Arc<ProjectSnapshot>,
        build: Build,
        base_package: Package,
        java_release: u32,
        flavor: Option<Flavor>,
    ) -> Self {
        Self {
            base,
            overlay: BTreeMap::new(),
            resources: BTreeMap::new(),
            recorded: BTreeMap::new(),
            facts: ProjectFacts::new(),
            build,
            base_package,
            java_release,
            flavor,
            fact_dependencies: BTreeMap::new(),
        }
    }

    pub fn build(&self) -> Build {
        self.build
    }

    pub fn base_package(&self) -> &Package {
        &self.base_package
    }

    pub fn java_release(&self) -> u32 {
        self.java_release
    }

    pub fn flavor(&self) -> Option<Flavor> {
        self.flavor
    }

    pub fn facts(&self) -> &ProjectFacts {
        &self.facts
    }

    /// Seed the resources the store already records.
    ///
    /// Kept apart from `resources`, which means "what the changes applied to
    /// this projection claim" and is checked against the ledger intent. These
    /// are what the store *already says*, and exactly one thing reads them:
    /// retirement. Which marker wraps a compose block, and whether it declared
    /// a volume, are facts about how the resource was installed, and they live
    /// in the store rather than in the request that removes it.
    pub fn record(&mut self, rows: &[jails_protocol::resource::ResourceRecord]) {
        for row in rows {
            self.recorded.insert(row.key.clone(), row.value.clone());
        }
    }

    pub fn resources(&self) -> &BTreeMap<ResourceKey, ProjectedResource> {
        &self.resources
    }

    /// Every path this projection has touched, in path order.
    pub fn overlay(&self) -> impl Iterator<Item = (&ProjectPath, &ProjectedEntry)> {
        self.overlay.iter()
    }

    pub fn entry(&self, path: &ProjectPath) -> Option<&ProjectedEntry> {
        self.overlay.get(path)
    }

    /// Who is charged with a path.
    ///
    /// Derived from the resource rows rather than recorded per file: a whole
    /// file's owners *are* its `WholeFile` resource's owners, and keeping a
    /// second copy beside the overlay is how the two come to disagree about
    /// whose removal deletes it.
    pub fn contributors(&self, path: &ProjectPath) -> BTreeSet<ResourceOwner> {
        self.resources
            .get(&ResourceKey::WholeFile(path.clone()))
            .map(|resource| resource.owners.clone())
            .unwrap_or_default()
    }

    /// Record that a fact kind was parsed from a path, so a later change to
    /// that path invalidates it.
    pub fn depends_on(&mut self, kind: FactKind, path: ProjectPath) {
        self.fact_dependencies.entry(kind).or_default().insert(path);
    }

    /// Read the project as the plan will leave it.
    ///
    /// An undeclared read is still an error — the overlay does not widen what
    /// planning may know, it only changes the answer for paths a change has
    /// touched.
    pub fn read(&self, path: &ProjectPath) -> Result<Projected<'_>> {
        match self.overlay.get(path) {
            Some(ProjectedEntry::File(file)) => Ok(Projected::Present(file)),
            Some(ProjectedEntry::Deleted) => Ok(Projected::Absent),
            Some(ProjectedEntry::Deferred { .. }) => Err(format!(
                "`{path}` is a deferred render, so its bytes do not exist yet.\n       fix: a \
                 planner may read its declared facts; only preparation renders it, and it \
                 renders it exactly once."
            )),
            None => Ok(match self.base.read(path)? {
                Captured::Present(file) => Projected::Present(file),
                Captured::Absent => Projected::Absent,
            }),
        }
    }

    /// The projected text of a path, or `None` when it is absent.
    pub fn text(&self, path: &ProjectPath) -> Result<Option<String>> {
        match self.read(path)? {
            Projected::Present(file) => String::from_utf8(file.bytes.to_vec())
                .map(Some)
                .map_err(|_| format!("`{path}` is not UTF-8")),
            Projected::Absent => Ok(None),
        }
    }

    /// Advance the projection by one change. plan.md §R2.4's six steps, in
    /// order, because each one reads what the previous produced.
    pub fn advance(&mut self, change: &DesiredChange) -> Result<()> {
        change.validate()?;
        let mut touched: BTreeSet<ProjectPath> = BTreeSet::new();

        // 1. Compose the resources this change claims.
        for resource in &change.resources {
            self.claim(resource.key.clone(), &resource.value, &resource.owners)?;
        }

        // 2. Files. Materialised bytes enter as bytes; a render stays deferred.
        for file in &change.files {
            match &file.body {
                DesiredBody::Bytes(bytes) => self.place(
                    &file.path,
                    bytes.to_vec(),
                    file.mode.unwrap_or(default_mode()),
                ),
                DesiredBody::Render { .. } => {
                    self.overlay.insert(
                        file.path.clone(),
                        ProjectedEntry::Deferred {
                            body: file.body.clone(),
                            facts: change.fact_delta.clone(),
                        },
                    );
                }
            }
            touched.insert(file.path.clone());
        }

        // 2b. Semantic edits, through the format owner, against the current
        // projected bytes rather than the base snapshot.
        for edit in &change.edits {
            if let Some(path) = self.apply_edit(edit)? {
                touched.insert(path);
            }
        }

        // 3. Absences. A parent directory is never removed implicitly: an
        // empty directory is not the same observation as a missing one, and a
        // later listing has to be able to tell them apart.
        for absence in &change.absences {
            self.overlay
                .insert(absence.path.clone(), ProjectedEntry::Deleted);
            touched.insert(absence.path.clone());
        }

        // 4 and 5. Invalidate and reparse, in `FactKind` order.
        self.reparse(&touched)?;

        // 6. Apply the recipe's declared delta and check it against what the
        // bytes actually say. A disagreement is a planner error, not a
        // difference to reconcile: one of the two is describing a project that
        // does not exist.
        self.apply_delta(&change.fact_delta)
    }

    fn claim(
        &mut self,
        key: ResourceKey,
        value: &ResourceValue,
        owners: &BTreeSet<ResourceOwner>,
    ) -> Result<()> {
        value.agrees_with(&key)?;
        match self.resources.get_mut(&key) {
            Some(existing) => {
                if existing.value != *value {
                    return Err(format!(
                        "two owners want different values for the same resource {key:?}.\n       \
                         fix: a shared resource has one value; reconcile the two declarations."
                    ));
                }
                existing.owners.extend(owners.iter().cloned());
            }
            None => {
                self.resources.insert(
                    key,
                    ProjectedResource {
                        value: value.clone(),
                        owners: owners.clone(),
                    },
                );
            }
        }
        Ok(())
    }

    /// Apply one edit through its format owner, returning the path it changed.
    ///
    /// Against the *projected* bytes, not the base snapshot: two changes in
    /// one run that both splice the POM must compose, and reading the base
    /// each time would make the second overwrite the first.
    fn apply_edit(&mut self, edit: &SemanticEdit) -> Result<Option<ProjectPath>> {
        match edit {
            SemanticEdit::MavenDependency { value, .. } => {
                let path = pom_path()?;
                let text = self.required_text(&path)?;
                let version = match &value.version {
                    jails_protocol::coordinate::MavenVersion::Managed => None,
                    jails_protocol::coordinate::MavenVersion::Pinned(pinned) => {
                        Some(pinned.as_str())
                    }
                };
                let scope = match value.scope {
                    jails_protocol::coordinate::MavenScope::Compile => None,
                    other => Some(other.label()),
                };
                let spliced = pom::add_dependency_ref(
                    &text,
                    DependencyRef {
                        group_id: value.coordinate.group_id.as_str(),
                        artifact_id: value.coordinate.artifact_id.as_str(),
                        version,
                        scope,
                        optional: value.optional,
                    },
                )?;
                self.write_text(&path, spliced.unwrap_or(text));
                Ok(Some(path))
            }
            SemanticEdit::MavenPlugin { value, .. } => {
                let path = pom_path()?;
                let text = self.required_text(&path)?;
                let spliced = pom::add_plugin(
                    &text,
                    value.coordinate.artifact_id.as_str(),
                    value.block.as_str(),
                )?;
                self.write_text(&path, spliced.unwrap_or(text));
                Ok(Some(path))
            }
            SemanticEdit::ComposeService { value, .. } => {
                let path = compose_path()?;
                let text = self.text(&path)?.unwrap_or_default();
                let volume = value.volumes.iter().next().map(|v| v.as_str());
                let spliced = crate::compose::add_service_ref(
                    &text,
                    crate::compose::ServiceRef {
                        name: value.name.as_str(),
                        marker: value.marker.as_str(),
                        body: value.mapping.as_str(),
                        volume,
                    },
                );
                self.write_text(&path, spliced.unwrap_or(text));
                Ok(Some(path))
            }
            SemanticEdit::Property { key, value } => {
                let ResourceKey::Property { path, key } = key else {
                    return Err("a property edit filed under another key".to_string());
                };
                let text = self.text(path)?.unwrap_or_default();
                self.write_text(
                    path,
                    properties::introduce(&text, key.as_str(), &value.value, &value.comment),
                );
                Ok(Some(path.clone()))
            }
            SemanticEdit::MarkedBlock { key, body } => {
                let ResourceKey::MarkedBlock { path, marker } = key else {
                    return Err("a marked-block edit filed under another key".to_string());
                };
                let text = self.text(path)?.unwrap_or_default();
                let marked = Marked::new(marker.as_str());
                // Replacing means removing then rendering, which is the same
                // path `sync` takes -- `codemod` deliberately has no
                // `replace`, so this cannot drift from it.
                let without = marked.strip_from(&text).unwrap_or(text);
                self.write_text(path, format!("{without}{}", marked.render(body)));
                Ok(Some(path.clone()))
            }
            SemanticEdit::HumanConfigCapability { key, spec } => {
                let ResourceKey::HumanConfigCapability(id) = key else {
                    return Err("a capability edit filed under another key".to_string());
                };
                let path = human_config_path()?;
                let text = self.text(&path)?.unwrap_or_default();
                let declaration = crate::capability::Declaration::of(id, spec);
                if let Some(updated) = crate::config::with_capability(&text, &declaration)? {
                    self.write_text(&path, updated);
                }
                Ok(Some(path))
            }
            SemanticEdit::Retire { key } => self.retire(key),
            SemanticEdit::HumanConfigLayout { layer, directory } => {
                let path = human_config_path()?;
                let text = self.text(&path)?.unwrap_or_default();
                let updated = crate::config::with_layout(&text, layer.package(), directory)?;
                self.write_text(&path, updated);
                Ok(Some(path))
            }
            SemanticEdit::SpringTestImport {
                key,
                class,
                statement,
            } => {
                let ResourceKey::SpringTestImport { path, class: keyed } = key else {
                    return Err("a test-import edit filed under another key".to_string());
                };
                if keyed != class {
                    return Err(format!("test import {class} filed under key {keyed}"));
                }
                // Absent rather than an error: the reader may have deleted the
                // test since this was planned, and the recheck under the lock
                // is where that becomes a refusal -- not here, where it would
                // be a refusal against a stale read.
                let Some(text) = self.text(path)? else {
                    return Ok(None);
                };
                match jails_java::annotate::splice_import(&text, class.name().as_str(), statement) {
                    Some(spliced) => self.write_text(path, spliced),
                    // No `@SpringBootTest` anchor any more. Same reasoning.
                    None => return Ok(None),
                }
                Ok(Some(path.clone()))
            }
            // The dispatcher's source file is a Java file the recipe rewrites
            // as a whole, so the registration reaches the projection as that
            // file rather than as a keyed splice. What is recorded here is
            // the fact, through the change's `FactDelta`.
            SemanticEdit::CommandRegistration { .. } => Ok(None),
        }
    }

    fn required_text(&self, path: &ProjectPath) -> Result<String> {
        self.text(path)?.ok_or_else(|| {
            format!(
                "`{path}` is absent, so there is nothing to splice into.\n       fix: a \
                 generator that emits code must supply the dependency it needs, and a project \
                 with no build file cannot take one."
            )
        })
    }

    /// Put bytes at a path, applying the one write-time rule about Java.
    ///
    /// Import order is normalised here rather than in the twenty templates
    /// that would otherwise each have to remember it -- CLAUDE.md's rule, and
    /// the direct write path has applied it for as long as there has been one.
    /// Applying it on only one of the two paths is worse than applying it on
    /// neither: the same recipe would then produce two different files
    /// depending on which engine ran it, and the difference is invisible until
    /// `jails add format` fails `mvn verify` on a freshly generated project.
    /// Take one resource back out of the file that holds it.
    ///
    /// Keyed removal, never a byte comparison: the caller asked for the thing
    /// that owns this line to go, so the line goes even if somebody edited it.
    /// A `WholeFile` key is not handled here -- a file an entity owns is
    /// removed as an absence in the change, where the executor can guard the
    /// preimage it is deleting.
    fn retire(&mut self, key: &ResourceKey) -> Result<Option<ProjectPath>> {
        match key {
            ResourceKey::MavenDependency(coordinate) => {
                let path = pom_path()?;
                let text = self.required_text(&path)?;
                let without = pom::remove_dependency(
                    &text,
                    coordinate.group_id.as_str(),
                    coordinate.artifact_id.as_str(),
                )?;
                self.write_text(&path, without.unwrap_or(text));
                Ok(Some(path))
            }
            ResourceKey::MavenPlugin(coordinate) => {
                let path = pom_path()?;
                let text = self.required_text(&path)?;
                let without = pom::remove_plugin(&text, coordinate.artifact_id.as_str())?;
                self.write_text(&path, without.unwrap_or(text));
                Ok(Some(path))
            }
            ResourceKey::ComposeService(name) => {
                let path = compose_path()?;
                let Some(text) = self.text(&path)? else {
                    return Ok(None);
                };
                // Which marker wraps the block, and whether it declared a
                // volume, are facts about how it was *installed*. They are
                // read back off the recorded resource rather than guessed:
                // guessing the marker would strip nothing and leave a service
                // the project no longer wants, quietly.
                let Some(ResourceValue::ComposeService(spec)) = self.recorded.get(key) else {
                    return Err(format!(
                        "{key:?} is not a recorded compose service, so there is nothing to say \
                         which marker wraps it"
                    ));
                };
                let volume = spec.volumes.iter().next().map(|volume| volume.as_str());
                let removed = crate::compose::remove_service_ref(
                    &text,
                    crate::compose::ServiceRef {
                        name: name.as_str(),
                        marker: spec.marker.as_str(),
                        body: "",
                        volume,
                    },
                );
                match removed {
                    Some(without) => self.write_text(&path, without),
                    None => return Ok(None),
                }
                Ok(Some(path))
            }
            ResourceKey::Property { path, key } => {
                let Some(text) = self.text(path)? else {
                    return Ok(None);
                };
                self.write_text(path, properties::remove(&text, key.as_str()));
                Ok(Some(path.clone()))
            }
            ResourceKey::MarkedBlock { path, marker } => {
                let Some(text) = self.text(path)? else {
                    return Ok(None);
                };
                let marked = Marked::new(marker.as_str());
                match marked.strip_from(&text) {
                    Some(without) => self.write_text(path, without),
                    None => return Ok(None),
                }
                Ok(Some(path.clone()))
            }
            ResourceKey::HumanConfigCapability(id) => {
                let path = human_config_path()?;
                let Some(text) = self.text(&path)? else {
                    return Ok(None);
                };
                // The spec is read back from the store rather than the key,
                // because `--package ''` and no `--package` are two different
                // declarations reaching the same identity -- and only the
                // recorded spec says which line this row put in the file.
                let spec = match self.recorded.get(key) {
                    Some(ResourceValue::HumanConfigCapability(spec)) => spec.clone(),
                    _ => jails_protocol::entity::CapabilitySpec { placement: None },
                };
                let declaration = crate::capability::Declaration::of(id, &spec);
                if let Some(without) = crate::config::without_capability(&text, &declaration)? {
                    self.write_text(&path, without);
                }
                Ok(Some(path))
            }
            ResourceKey::SpringTestImport { path, class } => {
                let Some(text) = self.text(path)? else {
                    return Ok(None);
                };
                // The `import` statement to drop is a fact about how this was
                // *installed* -- whether the config lived in another package
                // -- so it is read back off the recorded resource rather than
                // recomputed. Recomputing it against today's layout would
                // leave a stale import behind after a rename.
                let statement = match self.recorded.get(key) {
                    Some(ResourceValue::SpringTestImport { statement, .. }) => statement.clone(),
                    _ => String::new(),
                };
                match jails_java::annotate::unsplice_import(
                    &text,
                    class.name().as_str(),
                    &statement,
                ) {
                    Some(without) => self.write_text(path, without),
                    None => return Ok(None),
                }
                Ok(Some(path.clone()))
            }
            ResourceKey::CommandRegistration { .. } | ResourceKey::WholeFile(_) => Err(format!(
                "{key:?} is not retired by an edit.\n       fix: a whole file leaves as an \
                 absence, and a command registration as a rewrite of the dispatcher, both of \
                 which the executor can guard."
            )),
        }
    }

    fn write_text(&mut self, path: &ProjectPath, text: String) {
        self.place(path, text.into_bytes(), default_mode());
    }

    /// The one place bytes become a projected file.
    ///
    /// Both callers go through it -- a whole file a change renders, and a
    /// surgical edit to a file somebody else owns -- so the Java write-time
    /// rules cannot apply to one and not the other. Non-UTF-8 bytes are placed
    /// untouched: a `.java` file that is not text is already wrong, and
    /// guessing at its encoding would corrupt it further.
    fn place(&mut self, path: &ProjectPath, bytes: Vec<u8>, mode: FileMode) {
        let bytes = match (path.as_str().ends_with(".java"), String::from_utf8(bytes)) {
            (true, Ok(text)) => {
                jails_java::tidy::tidy_blank_lines(&jails_java::tidy::normalize_imports(&text))
                    .into_bytes()
            }
            (false, Ok(text)) => text.into_bytes(),
            (_, Err(error)) => error.into_bytes(),
        };
        self.overlay.insert(
            path.clone(),
            ProjectedEntry::File(SnapshotFile::capture(bytes, mode)),
        );
    }

    /// Invalidate every fact kind that depended on a touched path, then
    /// reparse it from the projected bytes in `FactKind` order.
    ///
    /// A deleted input yields that parser's explicit `Absent`, never a stale
    /// cache — which is the whole point of the dependency map.
    fn reparse(&mut self, touched: &BTreeSet<ProjectPath>) -> Result<()> {
        let stale: Vec<FactKind> = self
            .fact_dependencies
            .iter()
            .filter(|(_, paths)| paths.iter().any(|path| touched.contains(path)))
            .map(|(kind, _)| kind.clone())
            .collect();
        for kind in stale {
            self.facts.invalidate(&kind);
            self.parse_source(&kind)?;
        }
        Ok(())
    }

    /// Reparse one input from the projected bytes, recording its presence
    /// either way.
    pub fn parse_source(&mut self, kind: &FactKind) -> Result<()> {
        let path = match kind {
            FactKind::Pom => pom_path()?,
            FactKind::HumanConfig => human_config_path()?,
            FactKind::Compose => compose_path()?,
            FactKind::Properties(path) | FactKind::JavaSource(path) => path.clone(),
        };
        let Some(text) = self.text(&path)? else {
            self.facts.observe(kind.clone(), FactSourceState::Absent);
            // Every scalar this input decided goes with it. A flavour read
            // from a POM that is no longer there is not a fact about anything.
            if matches!(kind, FactKind::Pom) {
                self.flavor = None;
            }
            return Ok(());
        };
        self.facts.observe(
            kind.clone(),
            FactSourceState::Present {
                sha256: ObjectId::from_bytes(sha256(text.as_bytes())),
                len: text.len() as u64,
            },
        );
        match kind {
            FactKind::Pom => {
                self.flavor = Some(pom::flavor(&text));
                if let Some(release) = pom::release_level(&text) {
                    self.java_release = release;
                }
            }
            FactKind::Properties(path) => {
                for (key, value) in properties::parse(&text) {
                    self.facts.record(
                        kind.clone(),
                        ProjectFactKey::Property {
                            path: path.clone(),
                            key: jails_protocol::identity::PropertyKey::parse(&key)?,
                        },
                        ProjectFact::Property(value),
                    )?;
                }
            }
            // A POM's dependency facts, a compose file's services and a Java
            // source's types are parsed by the recipes that declare them; the
            // presence and the scalars are what projection owns. R6 moves the
            // remaining parsers behind this call.
            FactKind::HumanConfig | FactKind::Compose | FactKind::JavaSource(_) => {}
        }
        Ok(())
    }

    /// Apply the change's declared delta, and check it against the bytes.
    ///
    /// §R2.4: *"assert it equals the reparsed facts for known bytes. A
    /// disagreement is an internal planner error."* Not a difference to
    /// reconcile — one of the two is describing a project that does not exist,
    /// and picking either silently would carry that forward.
    fn apply_delta(&mut self, delta: &jails_protocol::edit::FactDelta) -> Result<()> {
        delta.validate()?;
        for key in &delta.remove {
            self.facts.invalidate_key(key);
        }
        for (key, fact) in &delta.add {
            if let Some(observed) = self.facts.get(key) {
                if observed != fact {
                    return Err(format!(
                        "the planner declared a fact for {key:?} that the projected bytes \
                         contradict.\n       fix: one of the two describes a project that does \
                         not exist; they are not differences to reconcile."
                    ));
                }
                continue;
            }
            let source = source_of(key);
            self.facts.record(source, key.clone(), fact.clone())?;
        }
        Ok(())
    }
}

/// Which input a declared fact belongs to.
///
/// A declared fact still has an owning input, because the next change to that
/// input must invalidate it. A `JavaType` fact names its own source file; the
/// rest are decided by the key's kind.
fn source_of(key: &ProjectFactKey) -> FactKind {
    match key {
        ProjectFactKey::MavenDependency(_) | ProjectFactKey::MavenPlugin(_) => FactKind::Pom,
        ProjectFactKey::ComposeService(_) => FactKind::Compose,
        ProjectFactKey::Property { path, .. } => FactKind::Properties(path.clone()),
        ProjectFactKey::MarkedBlock { path, .. } => FactKind::Properties(path.clone()),
        ProjectFactKey::CommandRegistration { .. } => FactKind::HumanConfig,
        ProjectFactKey::HumanConfigCapability(_) => FactKind::HumanConfig,
        ProjectFactKey::JavaType(_) => FactKind::HumanConfig,
    }
}

fn pom_path() -> Result<ProjectPath> {
    ProjectPath::parse("pom.xml")
}

fn compose_path() -> Result<ProjectPath> {
    ProjectPath::parse("compose.yaml")
}

fn human_config_path() -> Result<ProjectPath> {
    ProjectPath::parse("jails.toml")
}

fn default_mode() -> jails_protocol::conflict::FileMode {
    jails_protocol::conflict::FileMode::new(0o644).expect("0o644 is a permission mode")
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_protocol::change::{DesiredChange, MaintenanceAttribution};
    use jails_protocol::coordinate::{DependencySpec, MavenCoordinate};
    use jails_protocol::edit::FactDelta;
    use jails_protocol::entity::{EntityId, IntentId, Recipe};
    use jails_protocol::identity::{Name, PropertyKey};
    use jails_protocol::render::DesiredFile;
    use jails_protocol::resource::{DesiredResource, ResourceOwner};
    use jails_protocol::snapshot::CanonicalRoot;
    use std::collections::BTreeMap;

    const POM: &str = "<project>\n  <dependencies>\n  </dependencies>\n</project>\n";

    fn path(text: &str) -> ProjectPath {
        ProjectPath::parse(text).unwrap()
    }

    fn owner(name: &str) -> ResourceOwner {
        ResourceOwner::Entity(EntityId::Intent(IntentId::new(
            Recipe::Record,
            Name::parse(name).unwrap(),
            Package::parse("com.example.demo.domain").unwrap(),
        )))
    }

    fn snapshot(files: &[(&str, &str)], absences: &[&str]) -> Arc<ProjectSnapshot> {
        let mut captured = BTreeMap::new();
        for (name, body) in files {
            captured.insert(
                path(name),
                SnapshotFile::capture(body.as_bytes().to_vec(), default_mode()),
            );
        }
        Arc::new(
            ProjectSnapshot::new(
                CanonicalRoot::new("/srv/demo").unwrap(),
                captured,
                absences.iter().map(|name| path(name)).collect(),
                BTreeMap::new(),
            )
            .unwrap(),
        )
    }

    fn project(files: &[(&str, &str)], absences: &[&str]) -> ProjectedProject {
        ProjectedProject::new(
            snapshot(files, absences),
            Build::Maven,
            Package::parse("com.example.demo").unwrap(),
            25,
            None,
        )
    }

    fn dependency(group: &str, artifact: &str) -> (ResourceKey, DependencySpec) {
        let coordinate = MavenCoordinate::parse(group, artifact).unwrap();
        (
            ResourceKey::MavenDependency(coordinate.clone()),
            DependencySpec::managed(coordinate),
        )
    }

    fn splice(name: &str, group: &str, artifact: &str) -> DesiredChange {
        let (key, spec) = dependency(group, artifact);
        let mut change = DesiredChange::owned_by(owner(name));
        change.resources.push(
            DesiredResource::new(
                key.clone(),
                BTreeSet::from([owner(name)]),
                ResourceValue::MavenDependency(spec.clone()),
            )
            .unwrap(),
        );
        change
            .edits
            .push(SemanticEdit::MavenDependency { key, value: spec });
        change
    }

    /// The reason the overlay exists: `app apply` runs several intents in one
    /// invocation, and the second one has to splice into what the first left.
    #[test]
    fn a_second_change_splices_into_what_the_first_left() {
        let mut projected = project(&[("pom.xml", POM)], &[]);
        projected
            .advance(&splice("Note", "org.postgresql", "postgresql"))
            .unwrap();
        projected
            .advance(&splice("Memo", "com.h2database", "h2"))
            .unwrap();

        let text = projected.text(&path("pom.xml")).unwrap().unwrap();
        assert!(text.contains("postgresql"), "{text}");
        assert!(text.contains("<artifactId>h2</artifactId>"), "{text}");
    }

    /// Removing one owner leaves the resource. This is the whole reason a
    /// resource records an owner *set* rather than an owner.
    #[test]
    fn two_owners_of_one_dependency_compose_into_one_resource() {
        let mut projected = project(&[("pom.xml", POM)], &[]);
        projected
            .advance(&splice("Note", "org.postgresql", "postgresql"))
            .unwrap();
        projected
            .advance(&splice("Memo", "org.postgresql", "postgresql"))
            .unwrap();

        let (key, _) = dependency("org.postgresql", "postgresql");
        assert_eq!(projected.resources()[&key].owners.len(), 2);
        let text = projected.text(&path("pom.xml")).unwrap().unwrap();
        assert_eq!(
            text.matches("<artifactId>postgresql</artifactId>").count(),
            1,
            "the splice is idempotent: {text}"
        );
    }

    #[test]
    fn two_owners_wanting_different_values_for_one_resource_is_refused() {
        let mut projected = project(&[("pom.xml", POM)], &[]);
        projected
            .advance(&splice("Note", "org.postgresql", "postgresql"))
            .unwrap();

        let (key, _) = dependency("org.postgresql", "postgresql");
        let mut clash = DesiredChange::owned_by(owner("Memo"));
        let mut other = DependencySpec::managed(
            MavenCoordinate::parse("org.postgresql", "postgresql").unwrap(),
        );
        other.scope = jails_protocol::coordinate::MavenScope::Test;
        clash.resources.push(
            DesiredResource::new(
                key,
                BTreeSet::from([owner("Memo")]),
                ResourceValue::MavenDependency(other),
            )
            .unwrap(),
        );
        let error = projected.advance(&clash).unwrap_err();
        assert!(error.contains("different values"), "{error}");
    }

    /// §R2.4: never render here and render again in preparation. Two renders
    /// are two chances to differ, and the second is the one that reaches disk.
    #[test]
    fn a_render_stays_deferred_and_says_so_when_read() {
        let mut projected = project(&[], &["src/main/java/com/example/demo/App.java"]);
        let mut change = DesiredChange::maintenance(MaintenanceAttribution::AppInit);
        change.files.push(DesiredFile {
            path: path("src/main/java/com/example/demo/App.java"),
            body: jails_protocol::render::DesiredBody::Render {
                template: jails_protocol::identity::TemplateId::parse("app_java").unwrap(),
                bindings: jails_protocol::render::TemplateBindings::new(),
            },
            mode: None,
            resource: None,
            renderer: None,
        });
        projected.advance(&change).unwrap();

        let error = projected
            .text(&path("src/main/java/com/example/demo/App.java"))
            .unwrap_err();
        assert!(error.contains("deferred render"), "{error}");
    }

    /// A deleted input yields the parser's explicit `Absent`, never a stale
    /// cache. Without this a deleted POM would leave its facts in place.
    #[test]
    fn deleting_an_input_invalidates_its_facts_rather_than_leaving_them() {
        let properties = path("src/main/resources/application.properties");
        let mut projected = project(
            &[(
                "src/main/resources/application.properties",
                "server.port=8080\n",
            )],
            &[],
        );
        projected.depends_on(FactKind::Properties(properties.clone()), properties.clone());
        projected
            .parse_source(&FactKind::Properties(properties.clone()))
            .unwrap();

        let key = ProjectFactKey::Property {
            path: properties.clone(),
            key: PropertyKey::parse("server.port").unwrap(),
        };
        assert!(projected.facts().get(&key).is_some());

        let mut removal = DesiredChange::maintenance(MaintenanceAttribution::Format);
        removal.absences.push(jails_protocol::render::ManagedPath {
            path: properties.clone(),
            resource: ResourceKey::WholeFile(properties.clone()),
            force: false,
        });
        projected.advance(&removal).unwrap();

        assert_eq!(projected.facts().get(&key), None, "a stale fact survived");
        assert_eq!(
            projected.facts().source(&FactKind::Properties(properties)),
            Some(FactSourceState::Absent),
            "absence is recorded, not merely missing"
        );
    }

    /// A later change in the same run must observe the earlier POM edit rather
    /// than the base snapshot's scalar.
    #[test]
    fn a_scalar_is_recomputed_when_its_owning_input_changes() {
        let mut projected = project(&[("pom.xml", POM)], &[]);
        projected.depends_on(FactKind::Pom, path("pom.xml"));
        projected.parse_source(&FactKind::Pom).unwrap();
        assert_eq!(projected.flavor(), Some(Flavor::PlainMaven));

        let mut change = DesiredChange::maintenance(MaintenanceAttribution::Format);
        change.files.push(DesiredFile {
            path: path("pom.xml"),
            body: jails_protocol::render::DesiredBody::Bytes(
                b"<project>\n  <parent>\n    <artifactId>spring-boot-starter-parent</artifactId>\n  </parent>\n</project>\n"
                    .to_vec()
                    .into(),
            ),
            mode: None,
            resource: None,
            renderer: None,
        });
        projected.advance(&change).unwrap();
        assert_eq!(projected.flavor(), Some(Flavor::SpringBoot));
    }

    /// One of the two is describing a project that does not exist, and picking
    /// either silently would carry that forward.
    #[test]
    fn a_declared_fact_the_bytes_contradict_is_a_planner_error() {
        let properties = path("src/main/resources/application.properties");
        let mut projected = project(
            &[(
                "src/main/resources/application.properties",
                "server.port=8080\n",
            )],
            &[],
        );
        projected.depends_on(FactKind::Properties(properties.clone()), properties.clone());
        projected
            .parse_source(&FactKind::Properties(properties.clone()))
            .unwrap();

        let mut change = DesiredChange::maintenance(MaintenanceAttribution::Format);
        change.fact_delta = FactDelta {
            add: BTreeMap::from([(
                ProjectFactKey::Property {
                    path: properties.clone(),
                    key: PropertyKey::parse("server.port").unwrap(),
                },
                ProjectFact::Property("9090".to_string()),
            )]),
            remove: BTreeSet::new(),
        };
        let error = projected.advance(&change).unwrap_err();
        assert!(error.contains("contradict"), "{error}");
    }

    /// The overlay changes the answer for touched paths; it does not widen
    /// what planning may know.
    #[test]
    fn an_undeclared_read_is_still_an_error_through_the_projection() {
        let projected = project(&[("pom.xml", POM)], &[]);
        let error = projected.read(&path("build.gradle")).unwrap_err();
        assert!(error.contains("not captured"), "{error}");
    }

    #[test]
    fn a_property_edit_reaches_the_file_it_names() {
        let properties = path("src/main/resources/application.properties");
        let mut projected = project(
            &[(
                "src/main/resources/application.properties",
                "server.port=8080\n",
            )],
            &[],
        );
        let key = ResourceKey::Property {
            path: properties.clone(),
            key: PropertyKey::parse("server.port").unwrap(),
        };
        let mut change = DesiredChange::maintenance(MaintenanceAttribution::AdoptLayout);
        change.edits.push(SemanticEdit::Property {
            key,
            value: jails_protocol::resource::PropertySetting::plain("9090"),
        });
        projected.advance(&change).unwrap();
        assert_eq!(
            projected.text(&properties).unwrap().unwrap(),
            "server.port=9090\n"
        );
    }
}
