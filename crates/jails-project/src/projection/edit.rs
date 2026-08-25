//! How one semantic edit reaches a file, and how one claim is taken back out.
//!
//! The two halves of a single table: [`ProjectedProject::apply_edit`] renders
//! each [`SemanticEdit`] into the projected text, and
//! [`ProjectedProject::retire`] undoes exactly what the matching
//! [`ResourceKey`] installed. They are 429 lines of arm list between them,
//! which is what made `projection.rs` the largest module in the workspace --
//! `pending.md` §8.1 records that the honest answer to the next rise there was
//! the split rather than another ceiling, and this is it.
//!
//! **The seam is real rather than a size cut.** Everything left in the parent
//! is *state*: the overlay, the facts, the reads, what a path currently says.
//! Everything here is the per-key rendering, and the two arm lists have to be
//! read against each other -- an installing arm with a `match self.build` and a
//! retiring arm without one is the asymmetry that reported a Gradle project's
//! dependency retired while it stayed in `build.gradle`. Side by side in one
//! file, that is visible.

use super::*;

impl ProjectedProject {
    /// Apply one edit through its format owner, returning the path it changed.
    ///
    /// Against the *projected* bytes, not the base snapshot: two changes in
    /// one run that both splice the POM must compose, and reading the base
    /// each time would make the second overwrite the first.
    pub(super) fn apply_edit(&mut self, edit: &SemanticEdit) -> Result<Option<ProjectPath>> {
        match edit {
            SemanticEdit::MavenDependency { value, .. } => {
                let path = self.build_file_path()?;
                let Some(text) = self.optional_text(&path)? else {
                    return Ok(None);
                };
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
                let declaration = DependencyRef {
                    group_id: value.coordinate.group_id.as_str(),
                    artifact_id: value.coordinate.artifact_id.as_str(),
                    version,
                    scope,
                    optional: value.optional,
                };
                // One claim, two build files. The `Maven` in the edit's name is
                // the *coordinate's* -- `group:artifact:version` is what both
                // tools resolve against -- not the tool's, which is why a
                // Gradle project needs no second `SemanticEdit` variant and no
                // second recipe. Only the rendering differs, and it differs
                // here.
                let spliced = match self.build {
                    Build::Gradle => crate::gradle::add_dependency_ref(&text, declaration)?,
                    _ => pom::add_dependency_ref(&text, declaration)?,
                };
                self.write_text(&path, spliced.unwrap_or(text));
                Ok(Some(path))
            }
            SemanticEdit::BuildPlugin { key, value } => {
                let path = self.build_file_path()?;
                let Some(text) = self.optional_text(&path)? else {
                    return Ok(None);
                };
                // The key says what the build has to *do*; the value is the
                // Maven rendering of it. Both sides come off the claim rather
                // than being inferred from an artifact id -- `pending.md` §3.
                let ResourceKey::BuildFeature(feature) = key else {
                    return Err(format!(
                        "a build plugin edit keyed by {key:?}.\n       fix: this is a bug in \
                         jails, not something a project can cause -- please report the command."
                    )
                    .into());
                };
                let spliced = match self.build {
                    Build::Gradle => crate::gradle::add_feature(&text, *feature)?,
                    _ => pom::add_plugin(
                        &text,
                        value.coordinate.artifact_id.as_str(),
                        value.block.as_str(),
                    )?,
                };
                self.write_text(&path, spliced.unwrap_or(text));
                Ok(Some(path))
            }
            SemanticEdit::ComposeService { value, .. } => {
                let path = compose_path()?;
                let text = self.text(&path)?.unwrap_or_default();
                let volume = value.volumes.iter().next().map(|v| v.as_str());
                let spliced = crate::compose::add_canonical_service(
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
                    return Err(jails_support::Failure::Told(
                        "a property edit filed under another key".to_string(),
                    ));
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
                    return Err(jails_support::Failure::Told(
                        "a marked-block edit filed under another key".to_string(),
                    ));
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
                    return Err(jails_support::Failure::Told(
                        "a capability edit filed under another key".to_string(),
                    ));
                };
                let path = human_config_path()?;
                let text = self.text(&path)?.unwrap_or_default();
                let declaration = crate::capability::Declaration::of(id, spec);
                if let Some(updated) = crate::config::with_capability(&text, &declaration)? {
                    self.write_text(&path, updated);
                }
                Ok(Some(path))
            }
            // The packaged entry point, moved rather than rewritten: every
            // other byte of the POM is left alone, and a POM that names no
            // entry point at all is a Spring Boot project where the plugin
            // finds `@SpringBootApplication` itself -- nothing to claim.
            SemanticEdit::MavenMainClass { class, .. } => {
                let path = self.build_file_path()?;
                let Some(text) = self.optional_text(&path)? else {
                    return Ok(None);
                };
                let moved = match self.build {
                    Build::Gradle => crate::gradle::with_main_class(&text, &class.qualified()),
                    _ => pom::with_main_class(&text, &class.qualified()),
                };
                match moved {
                    Some(updated) => self.write_text(&path, updated),
                    None => return Ok(None),
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
                    return Err(jails_support::Failure::Told(
                        "a test-import edit filed under another key".to_string(),
                    ));
                };
                if keyed != class {
                    return Err(format!("test import {class} filed under key {keyed}").into());
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
            // A dispatch line in a file the command does not own: the
            // dispatcher belongs to the project, and a `commands.put(...)`
            // line is this command's claim inside it. Spliced here rather than
            // by the recipe rewriting the file whole, because owning it would
            // make `destroy command` delete the CLI.
            SemanticEdit::CommandRegistration { key, command } => {
                let ResourceKey::CommandRegistration { dispatcher, .. } = key else {
                    return Err(jails_support::Failure::Told(
                        "a registration filed under another key".to_string(),
                    ));
                };
                let path = java_source_of(dispatcher)?;
                let Some(text) = self.text(&path)? else {
                    // The dispatcher is not there. Nothing to splice into and
                    // nothing to refuse over: the generated command's Javadoc
                    // carries the line to add by hand.
                    return Ok(None);
                };
                let import = crate::spec::import_of(
                    dispatcher.package().as_str(),
                    command.package().as_str(),
                    command.name().as_str(),
                );
                let Some(spliced) = jails_java::dispatch::splice_registration(
                    &text,
                    command.name().as_str(),
                    &import,
                ) else {
                    return Ok(None);
                };
                self.write_text(&path, spliced);
                Ok(Some(path))
            }
        }
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
    pub(super) fn retire(&mut self, key: &ResourceKey) -> Result<Option<ProjectPath>> {
        match key {
            ResourceKey::MavenDependency(coordinate) => {
                // The same two-build-files split the `MavenDependency` edit
                // makes above, and for the same reason: the `Maven` in the
                // key's name is the *coordinate's*, not the tool's. This arm
                // used to read `pom_path()` unconditionally, so on a Gradle
                // project it opened a `pom.xml` that is not there, returned
                // `Ok(None)`, and left the dependency in `build.gradle` --
                // `remove` reporting success over a claim it had not retired.
                // Nothing caught it because `gradle::remove_dependency` was
                // `pub` and therefore not `dead_code`, which is `pending.md`
                // §7.2's whole argument.
                let path = self.build_file_path()?;
                let Some(text) = self.optional_text(&path)? else {
                    return Ok(None);
                };
                let group = coordinate.group_id.as_str();
                let artifact = coordinate.artifact_id.as_str();
                let without = match self.build {
                    Build::Gradle => crate::gradle::remove_dependency(&text, group, artifact)?,
                    _ => pom::remove_dependency(&text, group, artifact)?,
                };
                self.write_text(&path, without.unwrap_or(text));
                Ok(Some(path))
            }
            ResourceKey::BuildFeature(feature) => {
                let path = self.build_file_path()?;
                let Some(text) = self.optional_text(&path)? else {
                    return Ok(None);
                };
                let without = match self.build {
                    Build::Gradle => crate::gradle::remove_feature(&text, *feature),
                    // Maven unsplices by artifact id, which the retiring
                    // resource's own value carries -- but retirement is keyed,
                    // and a key that had to be mapped back to a coordinate is
                    // what §3 was about. The mapping is the feature's own.
                    _ => pom::remove_plugin(&text, feature.maven_artifact_id())?,
                };
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
                    )
                    .into());
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
                    // `remove_service_ref` answers with an empty document when
                    // the last service leaves, and an empty `compose.yaml` is
                    // not a file anybody keeps.
                    Some(without) => self.write_or_delete(&path, without),
                    None => return Ok(None),
                }
                Ok(Some(path))
            }
            ResourceKey::Property { path, key } => {
                let Some(text) = self.text(path)? else {
                    return Ok(None);
                };
                // The comment comes off the *recorded* value, for the same
                // reason the compose marker does: what jails wrote above this
                // key is a fact about how it was installed, and guessing at it
                // would either strip a reader's note or leave prose describing
                // a setting that is gone.
                let comment = match self.recorded.get(&ResourceKey::Property {
                    path: path.clone(),
                    key: key.clone(),
                }) {
                    Some(ResourceValue::Property(setting)) => setting.comment.clone(),
                    _ => Vec::new(),
                };
                self.write_or_delete(path, properties::remove(&text, key.as_str(), &comment));
                Ok(Some(path.clone()))
            }
            ResourceKey::CommandRegistration {
                dispatcher,
                command,
            } => {
                let path = java_source_of(dispatcher)?;
                let Some(text) = self.text(&path)? else {
                    return Ok(None);
                };
                let Some(without) =
                    jails_java::dispatch::unsplice_registration(&text, command.name().as_str())
                else {
                    return Ok(None);
                };
                self.write_text(&path, without);
                Ok(Some(path))
            }
            ResourceKey::MarkedBlock { path, marker } => {
                let Some(text) = self.text(path)? else {
                    return Ok(None);
                };
                let marked = Marked::new(marker.as_str());
                let Some(without) = marked.strip_from(&text) else {
                    return Ok(None);
                };
                self.write_or_delete(path, without);
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
            ResourceKey::MavenMainClass(path) => {
                let Some(text) = self.optional_text(path)? else {
                    return Ok(None);
                };
                // Restored from the recorded predecessor, never derived. A POM
                // that names `LedgerCli` says nothing about what it named
                // before, so a retirement that recomputed would put the jar on
                // a class nobody chose.
                let Some(ResourceValue::MavenMainClass { previous, .. }) = self.recorded.get(key)
                else {
                    return Ok(None);
                };
                let previous = previous.qualified();
                // Two build files, same as the edit that installed it. Without
                // this branch `destroy cli` on a Gradle project handed Groovy
                // to the XML rewriter, changed nothing, and reported the claim
                // retired.
                let restored = match self.build {
                    Build::Gradle => crate::gradle::with_main_class(&text, &previous),
                    _ => pom::with_main_class(&text, &previous),
                };
                match restored {
                    Some(updated) => self.write_text(path, updated),
                    None => return Ok(None),
                }
                Ok(Some(path.clone()))
            }
            ResourceKey::WholeFile(_) => Err(format!(
                "{key:?} is not retired by an edit.\n       fix: a whole file leaves as an \
                 absence, which is what the executor can guard a preimage for."
            )
            .into()),
        }
    }
}
