//! The pure Jails application compiler.
//!
//! This crate has no workspace path, filesystem, process, clock, network, or
//! transaction API. Equal snapshots, models and evolutions produce equal
//! drafts.

mod template;
pub(crate) use template::{Template, template};

mod ejectable;
pub(crate) use ejectable::component_kind_is_emitted;
pub use ejectable::implementation_paths;

mod emit_architecture;
mod emit_capability;
mod emit_companion_test;
mod emit_component;
mod emit_dto;
mod emit_http;
mod emit_java;
mod emit_messaging;
mod emit_mockmvc;
mod emit_operation;
mod emit_relation;
mod emit_resource_http;
mod emit_seed;
mod emit_sql;
pub use emit_sql::closed_set::narrowing_refusal as enum_narrowing_refusal;
pub use emit_sql::storage_columns;
mod emit_unit;
mod recipe;
mod refuse;

use jails_contracts::{
    BuildDependency, BuildFeature, BuildSystem, DocumentIntent, FileKind, JavaSourceSet, PlanDraft,
    ProjectPath, PropertyEntry, RenderedTree, SemanticPlan, WorkspaceSnapshot,
};
use jails_model::{DependencyScope, Evolution, SettingTarget};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MANAGED_ROOT: &str = ".jails/generated";

/// Reader files outside the generated tree that the current model may merge.
/// Frontends capture these before planning so collision/refusal checks are
/// based on exact live bytes rather than filesystem observations during apply.
pub fn external_project_paths(model: &jails_model::AppModel) -> Vec<ProjectPath> {
    let mut paths = emit_capability::external_project_paths(model);
    paths.extend(
        model
            .components
            .values()
            .filter_map(|component| component.source.as_ref())
            .filter_map(|source| ProjectPath::parse(source.clone()).ok()),
    );
    paths.sort();
    paths.dedup();
    paths
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileError {
    message: String,
}

impl CompileError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for CompileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CompileError {}

mod emit;
mod plan_effects;
mod storage;

pub struct Compiler;

impl Compiler {
    /// Lower one model to the tree, reader-file intents and migrations it
    /// implies.
    ///
    /// `next` is the model the edited source links to; `evolution` is what
    /// the source cannot say about how the accepted schema reaches it. The
    /// snapshot supplies every external fact, including the accepted model
    /// the migrations are derived against.
    pub fn compile(
        snapshot: &WorkspaceSnapshot,
        next: &jails_model::AppModel,
        evolution: &Evolution,
    ) -> Result<PlanDraft, CompileError> {
        let mut next_model = next.clone();
        // Why the layout arrives here rather than beside the model at each emit
        // site: `ProjectIntent::layout`, which has the whole argument.
        next_model.project.layout = snapshot.project.layout.clone();
        // After the layout, because it moves projections: the edit can add an
        // entity and the layout renames the packages every artifact lands in. The
        // records are in the model, so they are in the plan digest -- a
        // convention that moves has to change one.
        next_model.refresh_derived();
        refuse::preflight(snapshot, &next_model)?;

        // Validate schema evolution before rendering dependent Java adapters.
        // A missing backfill or retirement policy is the root semantic error;
        // reporting a downstream constructor/SQL consequence first hides the
        // action the reader must take.
        let mut migrations = emit_sql::derive(snapshot, &next_model, evolution)?;
        migrations.extend(emit_component::migrations(
            snapshot.accepted_model.as_ref(),
            &next_model,
        ));
        migrations.extend(emit_operation::outbox::migrations(
            snapshot.accepted_model.as_ref(),
            &next_model,
        ));
        let root = ProjectPath::parse(MANAGED_ROOT).map_err(CompileError::new)?;
        let baseline_model = snapshot.accepted_model.as_ref().or_else(|| {
            snapshot
                .files
                .keys()
                .any(|path| path.as_str().starts_with(MANAGED_ROOT))
                .then_some(&snapshot.model.model)
        });
        let mut baseline = snapshot
            .accepted_projection
            .clone()
            .unwrap_or_else(|| RenderedTree::new(root.clone()));
        if baseline.root != root {
            return Err(CompileError::new(format!(
                "accepted compiler projection has root `{}` instead of `{root}`\n       fix: restore a known-good `.jails/compiler.lock.json`",
                baseline.root
            )));
        }
        if snapshot.accepted_projection.is_none()
            && let Some(model) = baseline_model
        {
            emit::emit(model, &mut baseline, snapshot)?;
            let ejected = model
                .ejections
                .values()
                .map(|ejection| ejection.target.as_str())
                .collect::<BTreeSet<_>>();
            baseline
                .files
                .retain(|_, file| !ejected.contains(file.provenance.ejection_target()));
        }
        let mut generated = RenderedTree::new(root);
        emit::emit(&next_model, &mut generated, snapshot)?;
        let previous_ejections = snapshot
            .model
            .model
            .ejections
            .values()
            .map(|ejection| ejection.target.as_str())
            .collect::<BTreeSet<_>>();
        let desired_ejections = next_model
            .ejections
            .values()
            .map(|ejection| ejection.target.as_str())
            .collect::<BTreeSet<_>>();
        let newly_ejected = desired_ejections
            .difference(&previous_ejections)
            .copied()
            .collect::<BTreeSet<_>>();
        let mut matched_ejections = BTreeSet::new();
        let mut non_ejectable = BTreeSet::new();
        let mut ejection_intents = Vec::new();
        generated.files.retain(|path, file| {
            let boundary = file.provenance.ejection_target();
            if !desired_ejections.contains(boundary) {
                return true;
            }
            matched_ejections.insert(boundary.to_string());
            if !file.provenance.ejectable {
                non_ejectable.insert(boundary.to_string());
                return true;
            }
            let was_already_ejected = previous_ejections.contains(boundary);
            if !was_already_ejected && newly_ejected.contains(boundary) {
                let destination = match file.kind {
                    FileKind::JavaMain => path
                        .as_str()
                        .strip_prefix(".jails/generated/main/java/")
                        .map(|suffix| format!("src/main/java/{suffix}")),
                    FileKind::JavaTest => path
                        .as_str()
                        .strip_prefix(".jails/generated/test/java/")
                        .map(|suffix| format!("src/test/java/{suffix}")),
                    FileKind::Resource => path
                        .as_str()
                        .strip_prefix(".jails/generated/main/resources/")
                        .map(|suffix| format!("src/main/resources/{suffix}"))
                        .or_else(|| {
                            path.as_str()
                                .strip_prefix(".jails/generated/test/resources/")
                                .map(|suffix| format!("src/test/resources/{suffix}"))
                        }),
                    FileKind::HttpCollection => None,
                };
                let Some(destination) = destination else {
                    return true;
                };
                let bytes = snapshot
                    .files
                    .get(path)
                    .map(|captured| captured.bytes.clone())
                    .unwrap_or_else(|| file.bytes.clone());
                ejection_intents.push(DocumentIntent::EjectFile {
                    source: path.clone(),
                    path: ProjectPath::parse(destination)
                        .expect("a generated Java path maps to a project path"),
                    bytes,
                    semantic_ids: file.provenance.semantic_ids.clone(),
                });
            }
            false
        });
        if !non_ejectable.is_empty() {
            return Err(refuse_ejections(
                "ejection boundary",
                "form",
                "managed ABI and cannot be ejected\n       fix: eject an adapter implementation boundary; records and ports stay managed",
                &non_ejectable,
            ));
        }
        let unmatched = desired_ejections
            .iter()
            .filter(|target| !matched_ejections.contains(**target))
            .copied()
            .collect::<BTreeSet<_>>();
        if !unmatched.is_empty() {
            return Err(refuse_ejections(
                "ejection target",
                "emit",
                "no ejectable Java implementation\n       fix: eject an implementation boundary id; records and ports remain managed ABI",
                &unmatched,
            ));
        }
        // **A source root is declared only for a tree that has something in
        // it**, main included. Declaring the main root the moment a project
        // becomes canonical edits the reader's build file for a directory that
        // may stay empty -- `jails add dependency` declares no Java at all --
        // and the edit then outlives every reason for it, so retiring the last
        // declaration could not restore the pom it started from. The adapter
        // removes the block when no root is wanted, which makes this the whole
        // of the inverse.
        //
        // **One walk, and both build systems read it.** Maven and Gradle
        // declare a root differently and want the same four answers, so the
        // question "is there anything in this tree" is asked once, here, and
        // the two arms below only spell what they were given. Asked twice it
        // was eight near-identical `if` blocks over four roots, which is where
        // a fifth root gets added to one build system and not the other.
        // Membership is the path prefix rather than the `FileKind`, because
        // that is literally the question a source root asks.
        let wanted_roots = [
            (".jails/generated/main/java", JavaSourceSet::Main),
            (".jails/generated/test/java", JavaSourceSet::Test),
            (
                ".jails/generated/test/resources",
                JavaSourceSet::TestResources,
            ),
            (
                ".jails/generated/main/resources",
                JavaSourceSet::MainResources,
            ),
        ]
        .into_iter()
        .filter(|(root, _)| {
            generated
                .files
                .keys()
                .any(|path| path.as_str().starts_with(&format!("{root}/")))
        })
        .map(|(root, source_set)| {
            ProjectPath::parse(root)
                .map(|path| (path, source_set))
                .map_err(CompileError::new)
        })
        .collect::<Result<Vec<_>, _>>()?;
        let mut build_features = next_model
            .units
            .values()
            .filter_map(|unit| {
                (unit.kind == jails_model::UnitKind::IntegrationTest)
                    .then_some(BuildFeature::IntegrationTests)
            })
            .collect::<BTreeSet<_>>();
        build_features.extend(emit_capability::build_features(&next_model));
        // **Keyed off the emitted bytes, not off the model.** Surefire runs
        // `*Test`; `*IT` is Failsafe's, and Failsafe is not in the Spring Boot
        // parent's default build -- so an `*IT` in a project without the plugin
        // is a test that never runs while `mvn verify` reports success, which
        // is worse than having no test at all. The declaration-derived features
        // above only see a capability pack's own files; an `*IT` an operation
        // or component emitter writes -- a presence adapter's, a query
        // adapter's -- needs the plugin just as much.
        if generated
            .files
            .keys()
            .any(|path| path.as_str().ends_with("IT.java"))
        {
            build_features.insert(BuildFeature::IntegrationTests);
        }
        let mut required = Requirements::seeded(
            &snapshot.project.dependencies,
            next_model
                .dependencies
                .values()
                .map(|dependency| BuildDependency {
                    group: dependency.group.clone(),
                    artifact: dependency.artifact.clone(),
                    version: dependency.version.clone(),
                    scope: dependency.scope,
                    optional: false,
                })
                .collect(),
        );
        required.extend(emit_capability::dependencies(
            &next_model,
            snapshot.project.spring_boot.as_deref(),
            snapshot.project.build_system != BuildSystem::Unknown,
        ));
        required.extend(emit_component::dependencies(&next_model));
        let declares = |kind: &str| {
            next_model
                .capabilities
                .values()
                .any(|capability| capability.kind == kind)
        };
        let has_facet = |facet: jails_model::Facet| {
            next_model
                .entities
                .values()
                .any(|entity| entity.active && entity.facets.contains(&facet))
        };
        let has_unit =
            |kind: jails_model::UnitKind| next_model.units.values().any(|unit| unit.kind == kind);
        required.when(
            has_facet(jails_model::Facet::Dto),
            spring_starter("validation", DependencyScope::Compile),
        );
        // **Only when there is a build to declare it in.** A model with no
        // captured pom or Gradle script reaches the `BuildSystem::Unknown` arm
        // below, which refuses the moment any dependency is wanted -- so
        // adding one unconditionally turns "this project has no build file"
        // into a compile error for every scaffold.
        required.when(
            emit_architecture::applies(&next_model)
                && snapshot.project.build_system != BuildSystem::Unknown,
            emit_architecture::dependency(),
        );
        if declares("db") {
            required.extend(storage::storage_dependencies(
                snapshot.project.spring_boot.as_deref(),
            ));
        }
        // **Versionless under a Boot parent, pinned without one.** A
        // `<dependency>` with no version is correct where something manages it
        // and *fatal* where nothing does: Maven refuses to read the pom at
        // all, `validate` included. The pin is the project's own JUnit,
        // because the console launcher and the tests have to be one version --
        // a mismatch resolves and then dies at run time with
        // `NoSuchMethodError`.
        required.when(
            declares("fast-test"),
            BuildDependency {
                group: "org.junit.platform".to_string(),
                artifact: "junit-platform-console".to_string(),
                version: match snapshot.project.spring_boot {
                    Some(_) => None,
                    None => snapshot.project.junit.clone(),
                },
                scope: DependencyScope::Test,
                optional: false,
            },
        );
        // Boot 4 split the servlet test slice out of `spring-boot-starter-test`,
        // so `@AutoConfigureMockMvc` and `MockMvcTester` are no longer on the
        // test classpath by default -- and every controller jails emits comes
        // with a companion test that uses both. Below Boot 4 the classes are in
        // `spring-boot-test-autoconfigure`, already present, which is why this
        // is keyed on the major rather than declared unconditionally.
        required.when(
            emit_capability::boot_major(snapshot.project.spring_boot.as_deref())
                .is_some_and(|major| major >= 4)
                && has_unit(jails_model::UnitKind::Controller),
            spring_starter("webmvc-test", DependencyScope::Test),
        );
        // **Anything that serves HTTP declares the starter that serves it.**
        // The `api` capability's operation controllers are one source; a
        // scaffold's `http` facet is the other, and it emits a
        // `@RestController` into a project whose build may never have heard of
        // Spring Web -- `package org.springframework.web.bind.annotation does
        // not exist`, on a file the reader did not write.
        //
        // Gated on the project being Spring for the facet half, because the
        // entry is versionless: correct under `spring-boot-starter-parent`,
        // and fatal without it, where Maven refuses to read the pom at all.
        required.when(
            declares("api")
                || (snapshot.project.spring_boot.is_some() && has_facet(jails_model::Facet::Http)),
            spring_starter("web", DependencyScope::Compile),
        );
        let dependencies = required.into_sorted();
        let mut reader_document_intents = match snapshot.project.build_system {
            BuildSystem::Maven => {
                let mut roots = wanted_roots
                    .into_iter()
                    .map(|(path, source_set)| jails_contracts::MavenSourceRoot { source_set, path })
                    .collect::<Vec<_>>();
                roots.sort();
                vec![
                    DocumentIntent::EnsureMavenSourceRoots { roots },
                    DocumentIntent::ReconcileBuildFeatures {
                        features: build_features.clone(),
                    },
                    DocumentIntent::ReconcileDependencies { dependencies },
                ]
            }
            BuildSystem::Gradle => wanted_roots
                .into_iter()
                .map(
                    |(path, source_set)| DocumentIntent::EnsureGradleSourceRoot {
                        path,
                        source_set,
                    },
                )
                .chain([
                    DocumentIntent::ReconcileBuildFeatures {
                        features: build_features.clone(),
                    },
                    DocumentIntent::ReconcileDependencies { dependencies },
                ])
                .collect(),
            BuildSystem::Unknown if dependencies.is_empty() && build_features.is_empty() => {
                Vec::new()
            }
            BuildSystem::Unknown => {
                return Err(CompileError::new(
                    "canonical dependencies and build features require one captured Maven or Gradle build\n       fix: restore pom.xml, build.gradle, or build.gradle.kts, then re-plan",
                ));
            }
        };
        reader_document_intents.extend(ejection_intents);
        for (target, path) in [
            (
                SettingTarget::Main,
                "src/main/resources/application.properties",
            ),
            (
                SettingTarget::Test,
                "src/test/resources/config/application.properties",
            ),
        ] {
            let previous = property_entries(
                &snapshot.model.model,
                target,
                snapshot.project.spring_boot.as_deref(),
                snapshot.project.artifact_id.as_deref(),
            )?;
            let desired = property_entries(
                &next_model,
                target,
                snapshot.project.spring_boot.as_deref(),
                snapshot.project.artifact_id.as_deref(),
            )?;
            if previous.is_empty() && desired.is_empty() {
                continue;
            }
            reader_document_intents.push(DocumentIntent::ReconcileProperties {
                path: ProjectPath::parse(path).map_err(CompileError::new)?,
                previous,
                desired,
            });
        }
        // **Every `@SpringBootTest` already on disk needs the container
        // imported into it.** Once `spring-boot-starter-jdbc` is in the build,
        // JDBC auto-configuration demands a `DataSource` for all of them --
        // including the `contextLoads` test that shipped with the project and
        // never touches a database. The intent names no paths: the compiler
        // cannot enumerate `src/test/java` and must not, so the snapshot
        // carries those files and the materializer picks the ones that carry
        // the annotation.
        //
        // **It is emitted whether or not `db` is declared**, because the edit
        // has to come back out when the capability goes: a `@SpringBootTest`
        // importing a `TestcontainersConfig` that no longer exists does not
        // compile, and `remove db` is exactly when the model stops mentioning
        // it. The adapter's target set is empty once the project agrees, so a
        // converged plan writes nothing either way.
        if snapshot.project.spring_boot.is_some() {
            reader_document_intents.push(DocumentIntent::ReconcileSpringTestImport {
                class: "TestcontainersConfig".to_string(),
                package: next_model.project.package_for(jails_model::Package::Base),
                wanted: next_model
                    .capabilities
                    .values()
                    .any(|capability| capability.kind == "db"),
            });
        }
        // The dispatcher registration for every command in the model. Like the
        // container `@Import`, it names no path: which file is the dispatcher
        // is an observation, and the materializer reads it off the snapshot.
        for component in next_model.components.values() {
            if component.kind == jails_model::ComponentKind::Command {
                reader_document_intents.push(DocumentIntent::EnsureCommandRegistration {
                    class: format!("{}Command", component.name),
                    package: next_model.project.package_for(jails_model::Package::Cli),
                });
            }
        }
        if let Some(class) = emit_component::entry_point(snapshot, &next_model) {
            reader_document_intents.push(DocumentIntent::SetMavenMainClass { class });
        }
        let follow_up_effects = plan_effects::follow_up(&next_model, &generated, &baseline);
        let diagnostics = plan_effects::diagnostics(&next_model);
        let summary = SemanticPlan {
            model_nodes: next_model.node_count(),
            managed_files: generated.files.len(),
            migrations: migrations.len(),
            reader_document_intents: reader_document_intents.len()
                + baseline
                    .reader_facets
                    .keys()
                    .chain(generated.reader_facets.keys())
                    .collect::<BTreeSet<_>>()
                    .len(),
            effects: follow_up_effects.len(),
        };
        Ok(PlanDraft {
            next_model,
            baseline,
            generated,
            migrations,
            reader_document_intents,
            follow_up_effects,
            summary,
            diagnostics,
        })
    }
}

/// The build dependencies one compile requires, each declared at most once.
///
/// **Two rules, and both used to be written out at every call site.**
///
/// **What the reader's build already declares is already satisfied.**
/// Everything added here is *implied* -- by a capability, a component, an
/// entity facet -- rather than declared, so an artifact the project states
/// for itself answers the requirement and must not be declared a second time.
/// Without it a Gradle project carrying `spring-boot-starter-web` of its own
/// is refused outright the moment an entity gains an HTTP facet, over a
/// dependency it already has.
///
/// **And a coordinate two sources imply is one entry, not two.**
///
/// Neither rule covers the model's *own* dependencies, which is why they are
/// seeded rather than required: those the reader declared through jails, so a
/// copy outside the marked block is two owners for one coordinate and the
/// adapter is right to refuse it.
///
/// Eight hand-written copies of that pair was eight chances for the ninth to
/// forget half of it, in a check whose failure is a refusal on a dependency
/// the project already has.
struct Requirements<'a> {
    /// Every `group:artifact` the captured build already declares.
    observed: &'a BTreeSet<String>,
    declared: Vec<BuildDependency>,
}

impl<'a> Requirements<'a> {
    fn seeded(observed: &'a BTreeSet<String>, declared: Vec<BuildDependency>) -> Self {
        Self { observed, declared }
    }

    /// Require one coordinate, unless the build has it or something already
    /// asked for it.
    fn require(&mut self, required: BuildDependency) {
        let coordinate = format!("{}:{}", required.group, required.artifact);
        if self.observed.contains(&coordinate)
            || self.declared.iter().any(|declared| {
                declared.group == required.group && declared.artifact == required.artifact
            })
        {
            return;
        }
        self.declared.push(required);
    }

    /// Require one coordinate only when the condition holds.
    fn when(&mut self, condition: bool, required: BuildDependency) {
        if condition {
            self.require(required);
        }
    }

    fn extend(&mut self, required: impl IntoIterator<Item = BuildDependency>) {
        for dependency in required {
            self.require(dependency);
        }
    }

    fn into_sorted(self) -> Vec<BuildDependency> {
        let mut declared = self.declared;
        declared.sort();
        declared
    }
}

/// A Spring Boot starter, whose version the parent manages.
///
/// **Versionless deliberately, and only correct under the parent.** A
/// `<dependency>` with no `<version>` is what `spring-boot-starter-parent`
/// expects and what Maven refuses to read a pom without, so every caller
/// gates on the project being Spring; the four that spelled the literal out
/// could each have got the group, the prefix or the `optional` flag wrong
/// alone.
fn spring_starter(name: &str, scope: DependencyScope) -> BuildDependency {
    BuildDependency {
        group: "org.springframework.boot".to_string(),
        artifact: format!("spring-boot-starter-{name}"),
        version: None,
        scope,
        optional: false,
    }
}

/// One refusal naming a list of ejection targets, agreeing with its own count.
///
/// The two ejection refusals differ in nothing but their words, and each
/// carried its own pair of inverted pluralisation ternaries -- the shape where
/// a copy gets the noun's `s` and the verb's `s` the same way round and reads
/// correctly for exactly one of the two lengths.
fn refuse_ejections<T: std::fmt::Display>(
    noun: &str,
    verb: &str,
    tail: &str,
    targets: impl IntoIterator<Item = T>,
) -> CompileError {
    let named = targets
        .into_iter()
        .map(|target| format!("`{target}`"))
        .collect::<Vec<_>>();
    let one = named.len() == 1;
    CompileError::new(format!(
        "{noun}{} {} {verb}{} {tail}",
        if one { "" } else { "s" },
        named.join(", "),
        if one { "s" } else { "" },
    ))
}

fn property_entries(
    model: &jails_model::AppModel,
    target: SettingTarget,
    spring_boot: Option<&str>,
    artifact_id: Option<&str>,
) -> Result<Vec<PropertyEntry>, CompileError> {
    let mut entries = model
        .settings
        .values()
        .filter(|setting| setting.target == target)
        .map(|setting| (setting.key.clone(), setting.value.clone()))
        .collect::<BTreeMap<_, _>>();
    for property in emit_capability::properties(model, target, spring_boot, artifact_id)
        .into_iter()
        .chain(emit_component::properties(model, target)?)
    {
        if let Some(reader_value) = entries.get(&property.key)
            && reader_value != &property.value
        {
            return Err(CompileError::new(format!(
                "canonical capability property `{}` conflicts with model value `{reader_value}`\n       fix: remove the duplicate setting or give it the capability-required value `{}`",
                property.key, property.value
            )));
        }
        entries.insert(property.key, property.value);
    }
    Ok(entries
        .into_iter()
        .map(|(key, value)| PropertyEntry { key, value })
        .collect())
}

#[cfg(test)]
mod tests {
    /// Compile the snapshot's own model with no evolution: the shape every
    /// test that is not about schema evolution wants.
    fn compile_current(snapshot: &WorkspaceSnapshot) -> Result<PlanDraft, CompileError> {
        Compiler::compile(snapshot, &snapshot.model.model, &Evolution::none())
    }

    /// Every closed component kind is either emitted or refused, and never
    /// silently dropped (JDL v1 §20.2).
    ///
    /// A kind that links, plans and applies while emitting nothing produces
    /// no unit, no file and no diagnostic. The count here is deliberate -- it
    /// fails when a kind is added and quietly filed as unserved, which is the
    /// same silence arriving a different way.
    #[test]
    fn every_component_kind_is_emitted_or_refused() {
        use jails_model::ComponentKind;
        let emitted = ComponentKind::ALL
            .iter()
            .filter(|kind| super::component_kind_is_emitted(**kind))
            .count();
        assert_eq!(ComponentKind::ALL.len(), 23);
        assert_eq!(emitted, 23, "every component kind has a compiler backend");

        // The refusal is reachable, not merely written down.
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage postgres\n}\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        for kind in ComponentKind::ALL
            .iter()
            .filter(|kind| !super::component_kind_is_emitted(**kind))
        {
            let mut next = snapshot.clone();
            next.model.model.components.insert(
                jails_model::ComponentId::parse(format!("cmp_{}", kind.label().replace('-', "_")))
                    .unwrap(),
                component(*kind),
            );
            let error = compile_current(&next).expect_err("an unserved component kind must refuse");
            assert!(
                error.to_string().contains("has no compiler backend yet"),
                "{kind:?}: {error}"
            );
        }
    }

    /// A delivery policy that links must be honoured or refused.
    ///
    /// Direct and outbox delivery are different promises: one is a write and a
    /// publish that can fail independently, the other makes the event part of
    /// the same transaction as the row. Compiling a model that asks for the
    /// stronger one and quietly emitting the weaker is the silent failure this
    /// path exists to remove -- and it would be invisible, because the
    /// generated code compiles and the events do arrive, until one does not.
    ///
    /// So what is pinned here is the *difference*: the adapter stages rather
    /// than publishes, and it does so under `@Transactional`. An outbox that
    /// stages outside the statement's transaction has all of the machinery and
    /// none of the guarantee.
    #[test]
    fn outbox_delivery_stages_the_event_in_the_writing_transaction() {
        let model = jails_model::parse_jdl(OUTBOX_MODEL).unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let plan = compile_current(&snapshot).expect("`deliver outbox` has a backend");
        let file = |suffix: &str| {
            let file = plan
                .generated
                .files
                .iter()
                .find(|(path, _)| path.as_str().ends_with(suffix))
                .map(|(_, file)| file)
                .unwrap_or_else(|| panic!("no rendered file ends with `{suffix}`"));
            String::from_utf8(file.bytes.clone()).unwrap()
        };

        let command = file("/JdbcCreateCommand.java");
        assert!(command.contains("@Transactional"), "{command}");
        assert!(
            command.contains("outbox.stage(new TaskCreatedEvent("),
            "{command}"
        );
        assert!(
            !command.contains("publishEvent"),
            "staging and publishing are alternatives: {command}"
        );
        // The identity is minted rather than read off the row it describes.
        assert!(command.contains("TimeOrderedUuid.next()"), "{command}");
        assert!(command.contains("result.title()"), "{command}");
        // ... and the class that mints it is emitted, though no field default
        // asked for one.
        file("/TimeOrderedUuid.java");

        // The event record has to be able to hold what the command stages.
        let event = file("/TaskCreatedEvent.java");
        assert!(event.contains("UUID id"), "{event}");

        // The relay, its store, the port that makes it extensible, and the
        // scheduling that runs it at all.
        assert!(file("/JdbcCreateOutbox.java").contains("insert into create_outbox"));
        assert!(file("/CreateOutboxSink.java").contains("interface CreateOutboxSink"));
        assert!(file("/CreateOutboxWorker.java").contains("@Scheduled"));
        file("/SchedulingConfig.java");
        // A relay with no sink refuses to start, so a project that generates
        // clean has to have one.
        assert!(file("/CreateLoggingOutboxSink.java").contains("implements CreateOutboxSink"));

        let migration = plan
            .migrations
            .iter()
            .find(|migration| migration.logical_name == "create_create_outbox")
            .expect("the staged events need a table");
        assert!(
            String::from_utf8(migration.bytes.clone())
                .unwrap()
                .contains("delivered text[] not null")
        );
    }

    /// A migration is irreproducible, so compiling twice must not stage two
    /// `create table`s -- the second one is found by `flyway migrate`, in a
    /// project that was working yesterday.
    #[test]
    fn an_accepted_outbox_does_not_re_emit_its_table() {
        let model = jails_model::parse_jdl(OUTBOX_MODEL).unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model.clone());
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        snapshot.accepted_model = Some(model);
        let plan = compile_current(&snapshot).expect("`deliver outbox` has a backend");
        assert!(
            !plan
                .migrations
                .iter()
                .any(|migration| migration.logical_name == "create_create_outbox"),
            "an accepted outbox re-emitted its table"
        );
    }

    /// The store stages by `event.id()`, so an event without one would name an
    /// accessor its record does not have -- a compile error in the reader's
    /// project for a class they never wrote.
    #[test]
    fn an_outbox_event_that_projects_its_id_refuses_by_name() {
        let model = jails_model::parse_jdl(&OUTBOX_MODEL.replace(
            "event TaskCreated(id: uuid, title)",
            "event TaskCreated(id, title)",
        ))
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let error =
            compile_current(&snapshot).expect_err("an event id taken from the row must refuse");
        assert!(
            error
                .to_string()
                .contains("projects its `id` from the target row"),
            "{error}"
        );
    }

    /// One command, one outbox: the store is typed on a single payload.
    #[test]
    fn an_outbox_relaying_two_events_refuses_by_name() {
        let model = jails_model::parse_jdl(
            &OUTBOX_MODEL
                .replace(
                    "    emit TaskCreated\n",
                    "    emit TaskCreated\n    emit TaskFiled\n",
                )
                .replace(
                    "  event TaskCreated(id: uuid, title)\n",
                    "  event TaskCreated(id: uuid, title)\n  event TaskFiled(id: uuid, title)\n",
                ),
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let error =
            compile_current(&snapshot).expect_err("two events through one outbox must refuse");
        assert!(
            error
                .to_string()
                .contains("delivers 2 events through one outbox"),
            "{error}"
        );
    }

    /// The rendered store calls `Json.toJson`, so the capability that writes
    /// `Json` is a prerequisite -- named as a declaration the reader can make,
    /// not as a symbol they never asked for.
    #[test]
    fn an_outbox_without_the_json_capability_refuses_by_name() {
        let model = jails_model::parse_jdl(&OUTBOX_MODEL.replace("cap json\n", "")).unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let error = compile_current(&snapshot).expect_err("an outbox without `Json` must refuse");
        assert!(
            error.to_string().contains("fix: declare `cap json`"),
            "{error}"
        );
    }

    /// A sink is a plug, and `deliver outbox` renders the socket.
    ///
    /// What this pins is that the two halves fit: the generated class
    /// implements the port the outbox emitted, delivers the payload type the
    /// command stages, and hangs its settings off that command's own prefix so
    /// two sinks on two outboxes cannot collide.
    #[test]
    fn an_http_sink_implements_the_port_its_outbox_rendered() {
        let model = jails_model::parse_jdl(&format!(
            "{OUTBOX_MODEL}\ncomponent http-sink Provider {{\n  on create\n  yields task_created\n}}\n"
        ))
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let plan = compile_current(&snapshot).expect("an http sink has a backend");
        let file = |suffix: &str| {
            let file = plan
                .generated
                .files
                .iter()
                .find(|(path, _)| path.as_str().ends_with(suffix))
                .map(|(_, file)| file)
                .unwrap_or_else(|| panic!("no rendered file ends with `{suffix}`"));
            String::from_utf8(file.bytes.clone()).unwrap()
        };
        let sink = file("/ProviderHttpOutboxSink.java");
        assert!(sink.contains("implements CreateOutboxSink"), "{sink}");
        assert!(sink.contains("deliver(TaskCreatedEvent event)"), "{sink}");
        // The bounds that fail silently when they are missing.
        assert!(sink.contains("Redirect.NEVER"), "{sink}");
        assert!(sink.contains("\"Idempotency-Key\""), "{sink}");
        // Its settings hang off the command, so two outboxes cannot collide.
        assert!(
            sink.contains("${outbox.create.http.provider.url}"),
            "{sink}"
        );
        // The contract test is real rather than @Disabled: every component of
        // this payload is a builtin, so jails can sample all of them.
        let test = file("/ProviderHttpOutboxSinkTest.java");
        assert!(!test.contains("@Disabled"), "{test}");
    }

    /// A sink whose command publishes directly has no port to implement.
    #[test]
    fn an_http_sink_on_a_direct_command_refuses_by_name() {
        let model = jails_model::parse_jdl(&format!(
            "{}\ncomponent http-sink Provider {{\n  on create\n  yields task_created\n}}\n",
            // Direct delivery *and* a projected id, so the model's only
            // defect is the sink -- a minted id refuses first, and would
            // have this test pass on the wrong refusal.
            OUTBOX_MODEL.replace("    deliver outbox\n", "").replace(
                "event TaskCreated(id: uuid, title)",
                "event TaskCreated(id, title)"
            )
        ))
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let error =
            compile_current(&snapshot).expect_err("a sink with no outbox behind it must refuse");
        assert!(
            error.to_string().contains("which publishes directly"),
            "{error}"
        );
    }

    /// A traversal reaches the network only through a bounded fetcher.
    ///
    /// Every URL after the seed came off a page somebody else wrote, so the
    /// port is a security boundary rather than a convenience -- which is why
    /// `on` pointing anywhere else refuses. The rest of what this pins is the
    /// durability: the frontier is a table, and the claim that drains it is
    /// scheduled, so the config that turns scheduling on has to be there.
    #[test]
    fn an_http_workflow_traverses_through_its_fetcher_and_keeps_its_frontier_in_sql() {
        let model = jails_model::parse_jdl(WORKFLOW_MODEL).unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let plan = compile_current(&snapshot).expect("an http workflow has a backend");
        let file = |suffix: &str| {
            let file = plan
                .generated
                .files
                .iter()
                .find(|(path, _)| path.as_str().ends_with(suffix))
                .map(|(_, file)| file)
                .unwrap_or_else(|| panic!("no rendered file ends with `{suffix}`"));
            String::from_utf8(file.bytes.clone()).unwrap()
        };
        let workflow = file("/SweepWorkflow.java");
        assert!(workflow.contains("SiteFetcher"), "{workflow}");
        assert!(workflow.contains("sweep_frontier"), "{workflow}");
        file("/SweepWorkflowController.java");
        file("/SweepWorkflowIT.java");
        // The claim runs on a schedule, and without this the run sits QUEUED
        // forever with nothing to say why.
        file("/SchedulingConfig.java");
        let migration = plan
            .migrations
            .iter()
            .find(|migration| migration.logical_name == "create_sweep_workflow")
            .expect("the frontier needs its tables");
        let sql = String::from_utf8(migration.bytes.clone()).unwrap();
        for table in ["sweep_runs", "sweep_frontier", "sweep_pages"] {
            assert!(sql.contains(&format!("create table {table}")), "{sql}");
        }
    }

    /// The fetcher is the bound, so anything else in its place refuses.
    #[test]
    fn an_http_workflow_traversing_through_a_client_refuses_by_name() {
        let model = jails_model::parse_jdl(&WORKFLOW_MODEL.replace(
            "component fetcher Site {\n}",
            "component client Site {\n  route GET \"/pages\"\n}",
        ))
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let error = compile_current(&snapshot)
            .expect_err("a traversal through an unbounded client must refuse");
        assert!(
            error
                .to_string()
                .contains("which is a client rather than a fetcher"),
            "{error}"
        );
    }

    /// A renamed layer is renamed for *every* artifact, not most of them.
    ///
    /// `jails.toml`'s `[layout]` is the reader saying where their code lives,
    /// and it has to reach entities, capabilities and source units alike: a
    /// project that calls its domain `core` and gets `core` for its records
    /// but `domain` for its sealed types has two packages for one layer, in
    /// one tree, with nothing to report it.
    #[test]
    fn a_renamed_layer_moves_source_units_and_not_only_entities() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo {\n pkg com.example.demo\n java 26\n platform spring\n \
             build maven\n storage postgres\n}\nentity Note {\n use repo\n id: uuid @pk\n \
             title: string\n}\n\ncomponent sealed Outcome {\n  variant Accepted\n  \
             variant Rejected\n}\n\ncomponent service Notifier {\n}\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        snapshot.project.layout =
            jails_model::Layout::parse("[layout]\ndomain = \"core\"\nservice = \"usecases\"\n")
                .unwrap();
        let plan = compile_current(&snapshot).expect("a renamed layout still compiles");
        let paths = plan
            .generated
            .files
            .keys()
            .map(|path| path.as_str().to_string())
            .collect::<Vec<_>>();
        let holding = |needle: &str| {
            paths
                .iter()
                .filter(|path| path.contains(needle))
                .cloned()
                .collect::<Vec<_>>()
        };
        // The entity honours the rename.
        assert!(
            holding("/core/Note.java").len() == 1,
            "the record did not move to the renamed layer: {paths:?}"
        );
        // So do the source units.
        assert!(
            holding("/core/Outcome.java").len() == 1
                && holding("/core/OutcomeTest.java").len() == 1,
            "a sealed type or its test ignored the layer rename: {paths:?}"
        );
        assert!(
            holding("/usecases/NotifierService.java").len() == 1
                && holding("/usecases/NotifierServiceTest.java").len() == 1,
            "a service and its test did not both move with the rename: {paths:?}"
        );
        // ... and nothing is left behind under the default names, which is
        // the half that makes the tree incoherent rather than merely oddly
        // placed.
        assert!(
            holding("/domain/").is_empty() && holding("/service/").is_empty(),
            "artifacts remain under the pre-rename layer names: {paths:?}"
        );
    }

    /// A durable job runs an existing command later, and proves a retry is
    /// not a repeat.
    ///
    /// The recovery check is what this is really about. A process can die
    /// after the command commits and before the queue row is acknowledged, so
    /// the expired lease hands the same item to the next worker -- and without
    /// asking the repository first, at-least-once *delivery* becomes
    /// at-least-once *effect*.
    #[test]
    fn a_durable_job_executes_its_command_and_will_not_repeat_a_committed_effect() {
        let model = jails_model::parse_jdl(DURABLE_MODEL).unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let plan = compile_current(&snapshot).expect("a durable job has a backend");
        let file = |suffix: &str| {
            let file = plan
                .generated
                .files
                .iter()
                .find(|(path, _)| path.as_str().ends_with(suffix))
                .map(|(_, file)| file)
                .unwrap_or_else(|| panic!("no rendered file ends with `{suffix}`"));
            String::from_utf8(file.bytes.clone()).unwrap()
        };
        let worker = file("/DispatchWorker.java");
        assert!(
            worker.contains("if (results.findById(claimed.id()).isEmpty())"),
            "{worker}"
        );
        assert!(
            worker.contains("command.execute(claimed.work())"),
            "{worker}"
        );
        // The payload is the command's own Input, so a queued item cannot
        // describe work the command could not do.
        let queue = file("/DispatchQueue.java");
        assert!(
            queue.contains("void enqueue(UUID id, CreateCommand.Input work)"),
            "{queue}"
        );
        let store = file("/JdbcDispatchStore.java");
        assert!(store.contains("insert into dispatch_jobs"), "{store}");
        assert!(store.contains("for update skip locked"), "{store}");
        file("/DispatchJobController.java");
        file("/SchedulingConfig.java");
        // The conflict test is rendered, with a second payload that differs.
        let test = file("/DispatchJobIT.java");
        assert!(test.contains("IdempotencyConflictException"), "{test}");
        assert!(test.contains("\"other\""), "{test}");
        // ... and no placeholder survived into it.
        assert!(!test.contains("{{"), "{test}");
        let migration = plan
            .migrations
            .iter()
            .find(|migration| migration.logical_name == "create_dispatch_jobs")
            .expect("the queue needs a table");
        assert!(
            String::from_utf8(migration.bytes.clone())
                .unwrap()
                .contains("payload jsonb not null")
        );
    }

    /// The recovery proof is a repository lookup, so an entity without one
    /// leaves the worker unable to tell a retry from a repeat.
    #[test]
    fn a_durable_job_yielding_an_unstored_entity_refuses_by_name() {
        // A *second* entity with no repository, so the command's own target
        // stays stored and this is the only defect in the model.
        let model = jails_model::parse_jdl(&format!(
            "{}\nentity Note {{\n  id: uuid @pk\n  body: string\n}}\n",
            DURABLE_MODEL.replace("yields task", "yields note")
        ))
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let error = compile_current(&snapshot)
            .expect_err("a durable job with no recovery proof must refuse");
        assert!(
            error.to_string().contains("which has no repository"),
            "{error}"
        );
    }

    /// A command run later, its target, and the queue between them.
    const DURABLE_MODEL: &str = "jdl 1\napp Demo {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage postgres\n}\n\ncap json\n\n\
         entity Task {\n  use repo\n  id: uuid @pk\n  title: string\n\n  \
         command Create(title)\n}\n\ncomponent durable-job Dispatch {\n  on create\n  \
         yields task\n}\n";

    /// A traversal and the fetcher it goes through.
    ///
    /// Named `Sweep` rather than `Crawl` because `tests/genericity.rs` bans
    /// the proof apps' vocabulary from every crate's source, tests included --
    /// core is domain-blind, and a fixture named after `examples/web-crawler`
    /// is the first sign that a feature was designed around one domain.
    const WORKFLOW_MODEL: &str = "jdl 1\napp Demo {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage postgres\n}\n\n\
         component fetcher Site {\n}\n\ncomponent http-workflow Sweep {\n  on site\n}\n";

    /// One task, staged through an outbox: the model every test above varies.
    const OUTBOX_MODEL: &str = "jdl 1\napp Demo {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage postgres\n}\n\ncap json\n\n\
         entity Task {\n  use repo\n  id: uuid @pk\n  title: string\n\n  \
         command Create(title) {\n    emit TaskCreated\n    deliver outbox\n  }\n\n  \
         event TaskCreated(id: uuid, title)\n}\n";

    /// A policy about how events travel, on a command with none, does nothing.
    #[test]
    fn outbox_delivery_without_events_is_a_link_diagnostic() {
        let error = jails_model::parse_jdl(
            "jdl 1\napp Demo {\n  pkg com.example.demo\n  java 26\n  platform spring\n  \
             build maven\n  storage postgres\n}\n\nentity Task {\n  use repo\n  id: uuid @pk\n  \
             title: string\n\n  command Create(title) {\n    deliver outbox\n  }\n}\n",
        )
        .unwrap_err();
        assert!(
            format!("{error:?}").contains("needs at least one event to deliver"),
            "{error:?}"
        );
    }

    /// `use seed` writes the data, the loader, and the test that reads it.
    ///
    /// The two guards on the loader are what this pins. `@Profile("seed")`
    /// means it never runs where nobody asked, and the empty-table check means
    /// it never runs twice -- an edited seed row cannot be told from a change
    /// somebody made in the database, so re-applying one silently reverts
    /// their work.
    #[test]
    fn a_seed_projection_writes_its_data_a_guarded_loader_and_the_test_that_reads_it() {
        let model = jails_model::parse_jdl(SEED_MODEL).unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        let plan = compile_current(&snapshot).expect("`use seed` has a backend");
        let file = |suffix: &str| {
            let file = plan
                .generated
                .files
                .iter()
                .find(|(path, _)| path.as_str().ends_with(suffix))
                .map(|(_, file)| file)
                .unwrap_or_else(|| {
                    panic!(
                        "no rendered file ends with `{suffix}`; emitted:\n{}",
                        plan.generated
                            .files
                            .keys()
                            .map(|path| path.as_str().to_string())
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                });
            String::from_utf8(file.bytes.clone()).unwrap()
        };
        let data = file("/db/seeds/notes.json");
        assert!(data.contains("\"title\": \"sample\""), "{data}");
        assert!(data.contains("\"id\": \"00000000-"), "{data}");
        let seeder = file("/NoteSeeder.java");
        assert!(seeder.contains("@Profile(\"seed\")"), "{seeder}");
        assert!(
            seeder.contains("if (!repository.findAll().isEmpty()) {"),
            "{seeder}"
        );
        // Through the port, so a row the record rejects fails at start-up
        // rather than sitting in the table.
        assert!(seeder.contains("repository.save(row)"), "{seeder}");
        let test = file("/NoteSeederTest.java");
        assert!(!test.contains("@Disabled"), "{test}");
        assert!(!test.contains("{{"), "{test}");
    }

    /// The loader reads through the `json` capability's class, so its absence
    /// is a declaration to make rather than a symbol the reader never named.
    #[test]
    fn a_seed_without_the_json_capability_refuses_by_name() {
        let model = jails_model::parse_jdl(&SEED_MODEL.replace("cap json\n", "")).unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        let error = compile_current(&snapshot).expect_err("a seed with no reader must refuse");
        assert!(
            error.to_string().contains("fix: declare `cap json`"),
            "{error}"
        );
    }

    /// A stored entity with development data.
    const SEED_MODEL: &str = "jdl 1\napp Demo {\n pkg com.example.demo\n java 26\n \
         platform spring\n build maven\n storage postgres\n}\ncap json\n\
         entity Note {\n id: uuid @pk\n title: string\n}\nuse repo for Note\n\
         use seed for Note\n";

    /// A projection that links must render *its own* artifact, not the
    /// nearest one.
    ///
    /// `Facet` is the emitter's dispatch key, so a `ProjectionKind` mapped
    /// onto the wrong facet emits a file nobody asked for and reports
    /// success -- a worse failure than a missing file, because there is
    /// nothing to notice. Seed emits three files and a factory is not among
    /// them.
    #[test]
    fn a_seed_projection_does_not_emit_a_factory() {
        let model = jails_model::parse_jdl(SEED_MODEL)
            .expect("the grammar accepts `use seed`, which is how this defect arrived");
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        // `use seed` wants Spring on the build -- the capture says so, not the
        // model.
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        let plan = compile_current(&snapshot).expect("`use seed` has a backend");
        let paths = plan
            .generated
            .files
            .keys()
            .map(|path| path.as_str().to_string())
            .collect::<Vec<_>>();
        assert!(
            !paths.iter().any(|path| path.ends_with("/NoteFactory.java")),
            "seed emitted the factory's artifact: {paths:?}"
        );
    }

    fn component(kind: jails_model::ComponentKind) -> jails_model::Component {
        jails_model::Component {
            id: jails_model::ComponentId::parse(format!("cmp_{}", kind.label().replace('-', "_")))
                .unwrap(),
            label: "probe".to_string(),
            name: "Probe".to_string(),
            kind,
            parameters: Vec::new(),
            on: None,
            yields: None,
            route: None,
            bindings: Vec::new(),
            variants: Vec::new(),
            source: None,
        }
    }

    /// A versionless dependency is correct under a Boot parent and fatal
    /// without one.
    ///
    /// `storage postgres` on a plain Maven project with `version: None` is a
    /// pom Maven refuses to *read* -- every goal fails, `validate` included.
    /// The two version boundaries are Boot's own: `flyway-database-postgresql`
    /// is managed from 3.3, and `spring-boot-flyway` exists only from 4.0,
    /// where omitting it means the migrations never run and nothing says so.
    #[test]
    fn storage_dependencies_follow_the_boot_the_project_actually_has() {
        let boot_4 = super::storage::storage_dependencies(Some("4.0.0"));
        assert!(
            boot_4
                .iter()
                .any(|dependency| dependency.artifact == "spring-boot-flyway"),
            "Boot 4 needs Flyway's split-out auto-configuration: {boot_4:?}"
        );
        assert!(
            boot_4.iter().all(|dependency| dependency.version.is_none()),
            "the Boot parent manages every one of them: {boot_4:?}"
        );

        let boot_31 = super::storage::storage_dependencies(Some("3.1.0"));
        assert!(
            !boot_31
                .iter()
                .any(|dependency| dependency.artifact == "spring-boot-flyway"),
            "the module does not exist below Boot 4: {boot_31:?}"
        );
        let flyway = boot_31
            .iter()
            .filter(|dependency| dependency.group == "org.flywaydb")
            .collect::<Vec<_>>();
        assert_eq!(flyway.len(), 2);
        assert!(
            flyway
                .iter()
                .all(|dependency| dependency.version.as_deref() == Some("12.8.1")),
            "below 3.3 Boot manages neither, and the pair moves together: {flyway:?}"
        );

        assert!(
            super::storage::storage_dependencies(Some("3.3.0"))
                .iter()
                .filter(|dependency| dependency.group == "org.flywaydb")
                .all(|dependency| dependency.version.is_none()),
            "3.3 is where Boot starts managing `flyway-database-postgresql`"
        );
    }

    use super::*;
    use jails_contracts::{BuildSystem, ContentDigest, MigrationRecord, WorkspaceSnapshot};
    use jails_model::{FieldAddPolicy, FieldId};

    /// One entity with a command, a query, a transition and an event, over
    /// a `fake` cap and no storage: the model every test below varies.
    ///
    /// `storage none` because most of these compile a detached snapshot
    /// with no build, and a modelled storage declares dependencies that
    /// need one; the database tests switch it on by replacement.
    const MODEL: &str = "jdl 1\n\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  \
         java 26\n  platform spring\n  build maven\n  storage none\n}\n\n\
         cap fake @id(cap_fake)\n\n\
         entity Note @id(ent_note) {\n  use repo, service, http\n  \
         id: uuid @id(fld_note_id) @pk\n  title: string @id(fld_note_title) @notBlank\n\n  \
         command CreateNote(title) @id(op_create_note) {\n    route POST \"/notes\"\n  }\n\n  \
         query OpenNotes(title) @id(op_open_notes) {\n    order by [id]\n    limit 50\n    \
         route GET \"/notes\"\n  }\n\n  \
         transition RenameNote(title) @id(op_rename_note) {\n    update [title]\n    \
         emit NoteCreated\n    route PATCH \"/notes/{id}\"\n  }\n\n  \
         event NoteCreated(id, title) @id(op_note_created)\n}\n";

    /// [`MODEL`] with its storage materialised.
    fn stored_model() -> String {
        MODEL.replace("storage none", "storage postgres")
    }

    /// [`stored_model`] with one more field, declared as `summary`.
    fn stored_model_with_summary(summary: &str) -> String {
        stored_model().replace(
            "  title: string @id(fld_note_title) @notBlank\n",
            &format!(
                "  title: string @id(fld_note_title) @notBlank\n  {summary} @id(fld_note_summary)\n"
            ),
        )
    }

    #[test]
    fn data_capabilities_lower_from_one_declarative_pack_registry() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\ncap csv Dataset @id(cap_csv)\ncap json @id(cap_json)\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let draft = compile_current(&snapshot).unwrap();
        let packs = draft
            .generated
            .files
            .iter()
            .filter(|(_, file)| {
                file.provenance
                    .compiler_pass
                    .starts_with("capability-pack-")
            })
            .collect::<Vec<_>>();
        assert_eq!(packs.len(), 4);
        assert!(draft.generated.files.keys().any(|path| {
            path.as_str()
                .ends_with("com/example/demo/adapters/DatasetReader.java")
        }));
        assert!(draft.generated.files.keys().any(|path| {
            path.as_str()
                .ends_with("com/example/demo/adapters/JsonTest.java")
        }));
        assert!(packs.iter().all(|(_, file)| file.provenance.ejectable));
        assert_eq!(
            packs
                .iter()
                .map(|(_, file)| file.provenance.ejection_target())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["cap_csv", "cap_json"])
        );
        let dependencies = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileDependencies { dependencies } => Some(dependencies),
                _ => None,
            })
            .unwrap();
        let csv = dependencies
            .iter()
            .find(|dependency| dependency.artifact == "commons-csv")
            .unwrap();
        assert_eq!(csv.version.as_deref(), Some("1.14.1"));
        let json = dependencies
            .iter()
            .find(|dependency| dependency.artifact == "jackson-databind")
            .unwrap();
        assert_eq!(json.version, None, "Spring owns the Jackson version");
    }

    #[test]
    fn test_packs_share_the_same_file_dependency_and_ejection_engine() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\ncap fake @id(cap_fake)\ncap http Admin @id(cap_http)\ncap testkit @id(cap_testkit)\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        // Deliberately no Spring Boot: these packs are the plain-Maven ones,
        // and the pinned AssertJ version below is what a project with no
        // parent to manage it must receive.
        let draft = compile_current(&snapshot).unwrap();
        let expected = [
            ".jails/generated/test/java/com/example/demo/testkit/Fake.java",
            ".jails/generated/test/java/com/example/demo/testkit/FakeTest.java",
            ".jails/generated/main/java/com/example/demo/api/AdminServer.java",
            ".jails/generated/test/java/com/example/demo/api/AdminServerTest.java",
            ".jails/generated/test/java/com/example/demo/testkit/Clocks.java",
            ".jails/generated/test/java/com/example/demo/testkit/Ids.java",
            ".jails/generated/test/java/com/example/demo/testkit/Fixtures.java",
            ".jails/generated/test/java/com/example/demo/testkit/Cli.java",
            ".jails/generated/test/java/com/example/demo/testkit/TestkitTest.java",
            ".jails/generated/test/resources/fixtures/example.json",
        ];
        for path in expected {
            let file = draft
                .generated
                .files
                .get(&ProjectPath::parse(path).unwrap())
                .unwrap_or_else(|| panic!("missing {path}"));
            let boundary = if path.contains("Fake") {
                "cap_fake"
            } else if path.contains("Admin") {
                "cap_http"
            } else {
                "cap_testkit"
            };
            assert_eq!(file.provenance.ejection_id.as_deref(), Some(boundary));
            assert!(file.provenance.ejectable);
        }
        let dependencies = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileDependencies { dependencies } => Some(dependencies),
                _ => None,
            });
        let dependencies = dependencies.expect("test packs declare their test library");
        let assertj = dependencies
            .iter()
            .find(|dependency| dependency.artifact == "assertj-core")
            .expect("missing AssertJ test dependency");
        assert_eq!(assertj.scope, DependencyScope::Test);
        assert_eq!(assertj.version.as_deref(), Some("3.27.7"));
        assert_eq!(
            dependencies
                .iter()
                .filter(|dependency| dependency.artifact == "assertj-core")
                .count(),
            1
        );
        assert!(draft.reader_document_intents.iter().any(|intent| {
            matches!(intent, DocumentIntent::EnsureMavenSourceRoots { roots }
                if roots
                    .iter()
                    .any(|root| root.source_set == JavaSourceSet::TestResources))
        }));
    }

    #[test]
    fn sqlite_pack_projects_java_roles_and_an_append_only_migration() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\ncap sqlite Store @id(cap_sqlite)\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let draft = compile_current(&snapshot).unwrap();
        let expected = [
            ".jails/generated/main/java/com/example/demo/adapters/StoreDatabase.java",
            ".jails/generated/main/java/com/example/demo/adapters/StoreMigrations.java",
            ".jails/generated/test/java/com/example/demo/adapters/StoreDatabaseTest.java",
        ];
        for path in expected {
            let file = draft
                .generated
                .files
                .get(&ProjectPath::parse(path).unwrap())
                .unwrap_or_else(|| panic!("missing {path}"));
            assert_eq!(file.provenance.ejection_id.as_deref(), Some("cap_sqlite"));
        }
        assert!(!draft.reader_document_intents.iter().any(|intent| {
            matches!(intent, DocumentIntent::EnsureMavenSourceRoots { roots }
                if roots
                    .iter()
                    .any(|root| root.source_set == JavaSourceSet::MainResources))
        }));
        assert_eq!(draft.migrations.len(), 1);
        assert_eq!(draft.migrations[0].logical_name, "sqlite_init");
        assert_eq!(
            String::from_utf8(draft.migrations[0].bytes.clone()).unwrap(),
            "-- Applied once, in filename order, by Migrations.applyAll.\ncreate table if not exists item (\n    id integer primary key autoincrement,\n    name text not null,\n    qty integer not null default 0\n);\n"
        );
        assert_eq!(
            draft.migrations[0].semantic_ids,
            BTreeSet::from(["cap_sqlite".to_string()])
        );
        let dependencies = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileDependencies { dependencies } => Some(dependencies),
                _ => None,
            })
            .unwrap();
        let sqlite = dependencies
            .iter()
            .find(|dependency| dependency.artifact == "sqlite-jdbc")
            .unwrap();
        assert_eq!(sqlite.version.as_deref(), Some("3.49.1.0"));
    }

    #[test]
    fn h2_pack_projects_one_test_dependency_set_and_two_property_targets() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage h2\n}\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        let draft = compile_current(&snapshot).unwrap();
        let test = draft
            .generated
            .files
            .get(
                &ProjectPath::parse(
                    ".jails/generated/test/java/com/example/demo/adapters/H2DatabaseTest.java",
                )
                .unwrap(),
            )
            .expect("missing H2 database test");
        assert_eq!(test.provenance.ejection_id.as_deref(), Some("cap_h2"));

        let dependencies = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileDependencies { dependencies } => Some(dependencies),
                _ => None,
            })
            .unwrap();
        for artifact in ["spring-boot-starter-jdbc", "h2", "spring-boot-h2console"] {
            assert!(
                dependencies
                    .iter()
                    .any(|dependency| dependency.artifact == artifact),
                "missing {artifact}"
            );
        }
        let h2 = dependencies
            .iter()
            .find(|dependency| dependency.artifact == "h2")
            .unwrap();
        assert_eq!(h2.scope, DependencyScope::Runtime);
        assert_eq!(h2.version, None);

        let properties = draft
            .reader_document_intents
            .iter()
            .filter_map(|intent| match intent {
                DocumentIntent::ReconcileProperties { path, desired, .. } => {
                    Some((path.as_str(), desired))
                }
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let main = properties
            .get("src/main/resources/application.properties")
            .unwrap();
        assert!(main.iter().any(|entry| {
            entry.key == "spring.persistence.exceptiontranslation.enabled" && entry.value == "false"
        }));
        assert!(main.iter().any(|entry| {
            entry.key == "spring.datasource.url"
                && entry.value == "jdbc:h2:file:./data/app;AUTO_SERVER=TRUE"
        }));
        let test = properties
            .get("src/test/resources/config/application.properties")
            .unwrap();
        assert_eq!(test.len(), 1);
        assert_eq!(test[0].key, "spring.datasource.url");
        assert_eq!(test[0].value, "jdbc:h2:mem:test;DB_CLOSE_DELAY=-1");
    }

    #[test]
    fn actuator_pack_projects_one_ejectable_test_dependency_and_owned_properties() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\ncap actuator @id(cap_actuator)\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        let draft = compile_current(&snapshot).unwrap();
        let test = draft
            .generated
            .files
            .get(
                &ProjectPath::parse(
                    ".jails/generated/test/java/com/example/demo/ActuatorEndpointsTest.java",
                )
                .unwrap(),
            )
            .expect("missing Actuator endpoint contract test");
        assert_eq!(test.provenance.ejection_id.as_deref(), Some("cap_actuator"));
        assert!(
            String::from_utf8_lossy(&test.bytes)
                .contains("healthIsExposedOnASeparateManagementConnector")
        );

        let dependencies = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileDependencies { dependencies } => Some(dependencies),
                _ => None,
            })
            .unwrap();
        let actuator = dependencies
            .iter()
            .find(|dependency| dependency.artifact == "spring-boot-starter-actuator")
            .expect("missing Actuator starter");
        assert_eq!(actuator.version, None);

        let properties = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileProperties { path, desired, .. }
                    if path.as_str() == "src/main/resources/application.properties" =>
                {
                    Some(desired)
                }
                _ => None,
            })
            .expect("missing main properties reconciliation");
        assert_eq!(properties.len(), 9);
        assert!(properties.iter().any(|entry| {
            entry.key == "management.endpoints.web.exposure.include"
                && entry.value == "health,info,prometheus,threaddump"
        }));
        assert!(properties.iter().any(|entry| {
            entry.key == "management.endpoint.health.group.liveness.include"
                && entry.value == "ping"
        }));
        assert!(!properties.iter().any(|entry| entry.value == "*"));
    }

    #[test]
    fn cache_pack_projects_two_ejectable_files_dependencies_and_bounded_configuration() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\ncap cache @id(cap_cache)\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        let draft = compile_current(&snapshot).unwrap();
        for path in [
            ".jails/generated/main/java/com/example/demo/CacheConfig.java",
            ".jails/generated/test/java/com/example/demo/CacheConfigTest.java",
        ] {
            let file = draft
                .generated
                .files
                .get(&ProjectPath::parse(path).unwrap())
                .unwrap_or_else(|| panic!("missing {path}"));
            assert_eq!(file.provenance.ejection_id.as_deref(), Some("cap_cache"));
        }

        let dependencies = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileDependencies { dependencies } => Some(dependencies),
                _ => None,
            })
            .unwrap();
        for artifact in ["spring-boot-starter-cache", "caffeine"] {
            let dependency = dependencies
                .iter()
                .find(|dependency| dependency.artifact == artifact)
                .unwrap_or_else(|| panic!("missing {artifact}"));
            assert_eq!(dependency.version, None);
        }

        let properties = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileProperties { path, desired, .. }
                    if path.as_str() == "src/main/resources/application.properties" =>
                {
                    Some(desired)
                }
                _ => None,
            })
            .expect("missing cache property reconciliation");
        assert_eq!(properties.len(), 2);
        assert!(
            properties
                .iter()
                .any(|entry| { entry.key == "spring.cache.type" && entry.value == "caffeine" })
        );
        assert!(properties.iter().any(|entry| {
            entry.key == "spring.cache.caffeine.spec"
                && entry.value == "maximumSize=1000,expireAfterWrite=60s"
        }));
    }

    #[test]
    fn cors_pack_selects_boot_specific_tests_and_owns_one_property() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\ncap cors @id(cap_cors)\n",
        )
        .unwrap();
        let compile = |version: &str| {
            let mut snapshot = WorkspaceSnapshot::detached(model.clone());
            snapshot.project.build_system = BuildSystem::Maven;
            snapshot.project.spring_boot = Some("4.0.0".to_string());
            snapshot.project.spring_boot = Some(version.to_string());
            compile_current(&snapshot).unwrap()
        };

        let modern = compile("4.1.0");
        let classic = compile("3.4.0");
        for draft in [&modern, &classic] {
            for path in [
                ".jails/generated/main/java/com/example/demo/CorsConfig.java",
                ".jails/generated/test/java/com/example/demo/CorsConfigTest.java",
            ] {
                let file = draft
                    .generated
                    .files
                    .get(&ProjectPath::parse(path).unwrap())
                    .unwrap_or_else(|| panic!("missing {path}"));
                assert_eq!(file.provenance.ejection_id.as_deref(), Some("cap_cors"));
            }
            let properties = draft
                .reader_document_intents
                .iter()
                .find_map(|intent| match intent {
                    DocumentIntent::ReconcileProperties { path, desired, .. }
                        if path.as_str() == "src/main/resources/application.properties" =>
                    {
                        Some(desired)
                    }
                    _ => None,
                })
                .expect("missing CORS property reconciliation");
            assert!(properties.iter().any(|entry| {
                entry.key == "app.cors.allowed-origins" && entry.value == "https://example.invalid"
            }));
        }

        let has_webmvc_test = |draft: &PlanDraft| {
            draft
                .reader_document_intents
                .iter()
                .find_map(|intent| match intent {
                    DocumentIntent::ReconcileDependencies { dependencies } => Some(dependencies),
                    _ => None,
                })
                .expect("missing dependency reconciliation")
                .iter()
                .any(|dependency| {
                    dependency.artifact == "spring-boot-starter-webmvc-test"
                        && dependency.scope == DependencyScope::Test
                        && dependency.version.is_none()
                })
        };
        assert!(has_webmvc_test(&modern));
        assert!(!has_webmvc_test(&classic));

        let test_source = |draft: &PlanDraft| {
            String::from_utf8(
                draft
                    .generated
                    .files
                    .get(
                        &ProjectPath::parse(
                            ".jails/generated/test/java/com/example/demo/CorsConfigTest.java",
                        )
                        .unwrap(),
                    )
                    .unwrap()
                    .bytes
                    .clone(),
            )
            .unwrap()
        };
        let modern = test_source(&modern);
        assert!(modern.contains("servlet.assertj.MockMvcTester"), "{modern}");
        assert!(
            modern.contains(
                "org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc"
            ),
            "{modern}"
        );
        let classic = test_source(&classic);
        assert!(classic.contains("servlet.MockMvc"), "{classic}");
        assert!(
            !classic.contains("org.springframework.test.web.servlet.assertj.MockMvcTester"),
            "{classic}"
        );
        assert!(
            classic.contains(
                "org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc"
            ),
            "{classic}"
        );
    }

    #[test]
    fn observability_pack_projects_versioned_metrics_dependencies_and_bounded_properties() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\ncap observability @id(cap_observability)\n",
        )
        .unwrap();
        let compile = |version: &str| {
            let mut snapshot = WorkspaceSnapshot::detached(model.clone());
            snapshot.project.build_system = BuildSystem::Maven;
            snapshot.project.spring_boot = Some("4.0.0".to_string());
            snapshot.project.spring_boot = Some(version.to_string());
            compile_current(&snapshot).unwrap()
        };
        let modern = compile("4.1.0");
        let classic = compile("3.4.0");
        for draft in [&modern, &classic] {
            for path in [
                ".jails/generated/main/java/com/example/demo/MetricsConfig.java",
                ".jails/generated/main/java/com/example/demo/AppMetrics.java",
                ".jails/generated/test/java/com/example/demo/AppMetricsTest.java",
                ".jails/generated/test/java/com/example/demo/PrometheusScrapeTest.java",
            ] {
                let file = draft
                    .generated
                    .files
                    .get(&ProjectPath::parse(path).unwrap())
                    .unwrap_or_else(|| panic!("missing {path}"));
                assert_eq!(
                    file.provenance.ejection_id.as_deref(),
                    Some("cap_observability")
                );
            }
        }
        let source = |draft: &PlanDraft| {
            String::from_utf8(
                draft
                    .generated
                    .files
                    .get(
                        &ProjectPath::parse(
                            ".jails/generated/main/java/com/example/demo/MetricsConfig.java",
                        )
                        .unwrap(),
                    )
                    .unwrap()
                    .bytes
                    .clone(),
            )
            .unwrap()
        };
        assert!(source(&modern).contains(
            "org.springframework.boot.micrometer.metrics.autoconfigure.MeterRegistryCustomizer"
        ));
        assert!(source(&classic).contains(
            "org.springframework.boot.actuate.autoconfigure.metrics.MeterRegistryCustomizer"
        ));

        let dependencies = modern
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileDependencies { dependencies } => Some(dependencies),
                _ => None,
            })
            .unwrap();
        for artifact in [
            "spring-boot-starter-actuator",
            "micrometer-registry-prometheus",
        ] {
            assert!(dependencies.iter().any(|dependency| {
                dependency.artifact == artifact && dependency.version.is_none()
            }));
        }
        let properties = modern
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileProperties { path, desired, .. }
                    if path.as_str() == "src/main/resources/application.properties" =>
                {
                    Some(desired)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(properties.len(), 24);
        assert!(properties.iter().any(|entry| {
            entry.key == "management.metrics.distribution.slo.http.server.requests"
                && entry.value == "100ms,250ms,500ms,1s,2s,5s,10s"
        }));
        assert!(properties.iter().any(|entry| {
            entry.key == "management.tracing.sampling.probability" && entry.value == "0.1"
        }));
        assert!(!properties.iter().any(|entry| entry.value == "*"));
    }

    #[test]
    fn security_pack_projects_one_ejectable_boundary_and_enforces_its_boot_floor() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\ncap security @id(cap_security)\n",
        )
        .unwrap();
        let compile = |version: &str| {
            let mut snapshot = WorkspaceSnapshot::detached(model.clone());
            snapshot.project.build_system = BuildSystem::Maven;
            snapshot.project.spring_boot = Some("4.0.0".to_string());
            snapshot.project.spring_boot = Some(version.to_string());
            compile_current(&snapshot)
        };
        let refused = compile("2.7.18").unwrap_err().to_string();
        assert!(refused.contains("needs Spring Boot 3"), "{refused}");

        let modern = compile("4.1.0").unwrap();
        let classic = compile("3.4.0").unwrap();
        for draft in [&modern, &classic] {
            for path in [
                ".jails/generated/main/java/com/example/demo/SecurityConfig.java",
                ".jails/generated/main/java/com/example/demo/ProductionSecurityConfig.java",
                ".jails/generated/main/java/com/example/demo/ScopeAuthorizer.java",
                ".jails/generated/test/java/com/example/demo/SecurityConfigTest.java",
                ".jails/generated/test/java/com/example/demo/ScopeAuthorizerTest.java",
            ] {
                let file = draft
                    .generated
                    .files
                    .get(&ProjectPath::parse(path).unwrap())
                    .unwrap_or_else(|| panic!("missing {path}"));
                assert_eq!(file.provenance.ejection_id.as_deref(), Some("cap_security"));
            }
        }
        let test_source = |draft: &PlanDraft| {
            String::from_utf8(
                draft
                    .generated
                    .files
                    .get(
                        &ProjectPath::parse(
                            ".jails/generated/test/java/com/example/demo/SecurityConfigTest.java",
                        )
                        .unwrap(),
                    )
                    .unwrap()
                    .bytes
                    .clone(),
            )
            .unwrap()
        };
        assert!(
            test_source(&modern)
                .contains("org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest")
        );
        assert!(
            test_source(&classic)
                .contains("org.springframework.boot.test.autoconfigure.web.servlet.WebMvcTest")
        );
        let dependencies = |draft: &PlanDraft| {
            draft
                .reader_document_intents
                .iter()
                .find_map(|intent| match intent {
                    DocumentIntent::ReconcileDependencies { dependencies } => Some(dependencies),
                    _ => None,
                })
                .unwrap()
                .iter()
                .map(|dependency| dependency.artifact.clone())
                .collect::<BTreeSet<_>>()
        };
        let modern_dependencies = dependencies(&modern);
        for artifact in [
            "spring-boot-starter-security",
            "spring-boot-starter-oauth2-resource-server",
            "spring-security-test",
            "spring-boot-starter-webmvc-test",
        ] {
            assert!(modern_dependencies.contains(artifact), "missing {artifact}");
        }
        assert!(!dependencies(&classic).contains("spring-boot-starter-webmvc-test"));
    }

    #[test]
    fn sse_pack_projects_one_multi_package_iterative_boundary() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\ncap sse @id(cap_sse)\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        let draft = compile_current(&snapshot).unwrap();

        for path in [
            ".jails/generated/main/java/com/example/demo/EventHub.java",
            ".jails/generated/main/java/com/example/demo/SchedulingConfig.java",
            ".jails/generated/main/java/com/example/demo/web/EventStreamController.java",
            ".jails/generated/test/java/com/example/demo/EventHubTest.java",
        ] {
            let file = draft
                .generated
                .files
                .get(&ProjectPath::parse(path).unwrap())
                .unwrap_or_else(|| panic!("missing {path}"));
            assert_eq!(file.provenance.ejection_id.as_deref(), Some("cap_sse"));
        }
        let controller = String::from_utf8(
            draft
                .generated
                .files
                .get(
                    &ProjectPath::parse(
                        ".jails/generated/main/java/com/example/demo/web/EventStreamController.java",
                    )
                    .unwrap(),
                )
                .unwrap()
                .bytes
                .clone(),
        )
        .unwrap();
        assert!(controller.contains("import com.example.demo.EventHub;"));
        assert!(controller.contains("/events/{topic}/stream"));

        let dependencies = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileDependencies { dependencies } => Some(dependencies),
                _ => None,
            })
            .unwrap();
        assert!(dependencies.iter().any(|dependency| {
            dependency.artifact == "spring-boot-starter-web" && dependency.version.is_none()
        }));
        let properties = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileProperties { path, desired, .. }
                    if path.as_str() == "src/main/resources/application.properties" =>
                {
                    Some(desired)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].key, "spring.task.scheduling.pool.size");
        assert_eq!(properties[0].value, "4");
    }

    #[test]
    fn redis_pack_projects_merge_managed_source_compose_and_integration_build() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\ncap redis @id(cap_redis)\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        let draft = compile_current(&snapshot).unwrap();

        for path in [
            ".jails/generated/main/java/com/example/demo/adapters/KeyValueStore.java",
            ".jails/generated/test/java/com/example/demo/adapters/KeyValueStoreIT.java",
        ] {
            let file = draft
                .generated
                .files
                .get(&ProjectPath::parse(path).unwrap())
                .unwrap_or_else(|| panic!("missing {path}"));
            assert_eq!(file.provenance.ejection_id.as_deref(), Some("cap_redis"));
        }
        let facet = draft
            .generated
            .reader_facets
            .get("doc_cap_redis_compose_redis")
            .unwrap();
        assert_eq!(facet.path.as_str(), "compose.yaml");
        assert!(String::from_utf8_lossy(&facet.bytes).contains("image: redis:7-alpine"));

        let dependencies = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileDependencies { dependencies } => Some(dependencies),
                _ => None,
            })
            .unwrap();
        for artifact in [
            "spring-boot-starter-data-redis",
            "testcontainers",
            "spring-boot-testcontainers",
        ] {
            assert!(
                dependencies
                    .iter()
                    .any(|dependency| dependency.artifact == artifact),
                "missing {artifact}"
            );
        }
        assert!(draft.reader_document_intents.iter().any(|intent| matches!(
            intent,
            DocumentIntent::ReconcileBuildFeatures { features }
                if features.contains(&BuildFeature::IntegrationTests)
        )));
        let properties = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileProperties { path, desired, .. }
                    if path.as_str() == "src/main/resources/application.properties" =>
                {
                    Some(desired)
                }
                _ => None,
            })
            .unwrap();
        assert!(
            properties
                .iter()
                .any(|entry| { entry.key == "app.redis.default-ttl" && entry.value == "PT10M" })
        );
    }

    #[test]
    fn kafka_pack_projects_spring_sources_plain_client_and_one_compose_facet() {
        let source = "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\ncap kafka @id(cap_kafka)\n";
        let model = jails_model::parse_jdl(source).unwrap();
        let mut spring = WorkspaceSnapshot::detached(model.clone());
        spring.project.build_system = BuildSystem::Maven;
        spring.project.spring_boot = Some("4.1.0".to_string());
        let draft = compile_current(&spring).unwrap();

        for path in [
            ".jails/generated/main/java/com/example/demo/messaging/KafkaConfig.java",
            ".jails/generated/main/java/com/example/demo/messaging/NonRetryableException.java",
            ".jails/generated/test/java/com/example/demo/messaging/KafkaConfigTest.java",
            ".jails/generated/test/java/com/example/demo/KafkaTestcontainersConfig.java",
        ] {
            let file = draft
                .generated
                .files
                .get(&ProjectPath::parse(path).unwrap())
                .unwrap_or_else(|| panic!("missing {path}"));
            assert_eq!(file.provenance.ejection_id.as_deref(), Some("cap_kafka"));
            assert!(!String::from_utf8_lossy(&file.bytes).contains("{{"));
        }
        let facet = draft
            .generated
            .reader_facets
            .get("doc_cap_kafka_compose_kafka")
            .unwrap();
        assert_eq!(facet.path.as_str(), "compose.yaml");
        assert!(String::from_utf8_lossy(&facet.bytes).contains("apache/kafka:4.1.0"));

        let dependencies = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileDependencies { dependencies } => Some(dependencies),
                _ => None,
            })
            .unwrap();
        for artifact in [
            "spring-boot-starter-kafka",
            "micrometer-core",
            "spring-boot-testcontainers",
            "testcontainers-kafka",
            "testcontainers-junit-jupiter",
            "awaitility",
        ] {
            assert!(
                dependencies
                    .iter()
                    .any(|dependency| dependency.artifact == artifact),
                "missing {artifact}"
            );
        }
        let properties = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileProperties { path, desired, .. }
                    if path.as_str() == "src/main/resources/application.properties" =>
                {
                    Some(desired)
                }
                _ => None,
            })
            .unwrap();
        assert!(properties.iter().any(|entry| {
            entry.key == "spring.kafka.consumer.group-id" && entry.value == "demo"
        }));
        assert!(properties.iter().any(|entry| {
            entry.key == "spring.kafka.consumer.properties.spring.json.trusted.packages"
                && entry.value == "com.example.demo,com.example.demo.*"
        }));

        let mut plain = WorkspaceSnapshot::detached(model);
        plain.project.build_system = BuildSystem::Maven;
        let draft = compile_current(&plain).unwrap();
        assert!(draft.generated.files.is_empty());
        assert!(
            draft
                .generated
                .reader_facets
                .contains_key("doc_cap_kafka_compose_kafka")
        );
        let dependencies = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileDependencies { dependencies } => Some(dependencies),
                _ => None,
            })
            .unwrap();
        assert_eq!(dependencies.len(), 1);
        assert_eq!(dependencies[0].artifact, "kafka-clients");
        assert_eq!(dependencies[0].version.as_deref(), Some("4.1.0"));
    }

    #[test]
    fn mail_pack_projects_merge_managed_source_compose_and_boot_specific_tests() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\ncap mail @id(cap_mail)\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model.clone());
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        let draft = compile_current(&snapshot).unwrap();

        for path in [
            ".jails/generated/main/java/com/example/demo/Mailer.java",
            ".jails/generated/test/java/com/example/demo/MailerIT.java",
        ] {
            let file = draft
                .generated
                .files
                .get(&ProjectPath::parse(path).unwrap())
                .unwrap_or_else(|| panic!("missing {path}"));
            assert_eq!(file.provenance.ejection_id.as_deref(), Some("cap_mail"));
            assert!(!String::from_utf8_lossy(&file.bytes).contains("{{"));
        }
        let facet = draft
            .generated
            .reader_facets
            .get("doc_cap_mail_compose_mail")
            .unwrap();
        assert!(String::from_utf8_lossy(&facet.bytes).contains("mailpit:"));
        assert!(String::from_utf8_lossy(&facet.bytes).contains("axllent/mailpit:v1.21"));

        let dependencies = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileDependencies { dependencies } => Some(dependencies),
                _ => None,
            })
            .unwrap();
        for artifact in [
            "spring-boot-starter-mail",
            "spring-boot-starter-mail-test",
            "awaitility",
            "testcontainers",
            "testcontainers-junit-jupiter",
        ] {
            assert!(
                dependencies
                    .iter()
                    .any(|dependency| dependency.artifact == artifact),
                "missing {artifact}"
            );
        }
        assert!(
            !dependencies
                .iter()
                .any(|dependency| dependency.artifact == "spring-boot-starter-test")
        );
        assert!(draft.reader_document_intents.iter().any(|intent| matches!(
            intent,
            DocumentIntent::ReconcileBuildFeatures { features }
                if features.contains(&BuildFeature::IntegrationTests)
        )));
        let properties = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileProperties { path, desired, .. }
                    if path.as_str() == "src/main/resources/application.properties" =>
                {
                    Some(desired)
                }
                _ => None,
            })
            .unwrap();
        for (key, value) in [
            ("spring.mail.host", "localhost"),
            ("spring.mail.port", "1025"),
            ("app.mail.from", "no-reply@example.com"),
        ] {
            assert!(
                properties
                    .iter()
                    .any(|entry| entry.key == key && entry.value == value)
            );
        }

        snapshot.project.spring_boot = Some("3.5.0".to_string());
        let draft = compile_current(&snapshot).unwrap();
        let dependencies = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileDependencies { dependencies } => Some(dependencies),
                _ => None,
            })
            .unwrap();
        assert!(
            dependencies
                .iter()
                .any(|dependency| dependency.artifact == "spring-boot-starter-test")
        );
        assert!(
            !dependencies
                .iter()
                .any(|dependency| dependency.artifact == "spring-boot-starter-mail-test")
        );
    }

    #[test]
    fn toxiproxy_pack_projects_merge_managed_testkit_sources_and_exact_test_dependencies() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\ncap toxiproxy @id(cap_toxiproxy)\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let draft = compile_current(&snapshot).unwrap();

        for path in [
            ".jails/generated/test/java/com/example/demo/testkit/Faults.java",
            ".jails/generated/test/java/com/example/demo/testkit/FaultsTest.java",
        ] {
            let file = draft
                .generated
                .files
                .get(&ProjectPath::parse(path).unwrap())
                .unwrap_or_else(|| panic!("missing {path}"));
            assert_eq!(
                file.provenance.ejection_id.as_deref(),
                Some("cap_toxiproxy")
            );
            assert!(!String::from_utf8_lossy(&file.bytes).contains("{{"));
        }

        let dependencies = draft
            .reader_document_intents
            .iter()
            .find_map(|intent| match intent {
                DocumentIntent::ReconcileDependencies { dependencies } => Some(dependencies),
                _ => None,
            })
            .unwrap();
        for (artifact, version) in [
            ("testcontainers-toxiproxy", "2.0.5"),
            ("toxiproxy-java", "2.1.11"),
        ] {
            let dependency = dependencies
                .iter()
                .find(|dependency| dependency.artifact == artifact)
                .unwrap_or_else(|| panic!("missing {artifact}"));
            assert_eq!(dependency.version.as_deref(), Some(version));
            assert_eq!(dependency.scope, DependencyScope::Test);
        }
        assert!(!draft.reader_document_intents.iter().any(|intent| matches!(
            intent,
            DocumentIntent::ReconcileBuildFeatures { features }
                if features.contains(&BuildFeature::IntegrationTests)
        )));
    }

    #[test]
    fn coverage_pack_is_a_pure_build_feature_without_generated_files() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\ncap coverage @id(cap_coverage)\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let draft = compile_current(&snapshot).unwrap();

        assert!(draft.generated.files.is_empty());
        assert!(draft.generated.reader_facets.is_empty());
        assert!(draft.reader_document_intents.iter().any(|intent| matches!(
            intent,
            DocumentIntent::ReconcileBuildFeatures { features }
                if features == &BTreeSet::from([BuildFeature::Coverage])
        )));
    }

    #[test]
    fn loadtest_projects_six_merge_managed_files_from_typed_controller_routes() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\ncomponent controller Health\ncap loadtest @id(cap_loadtest)\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let draft = compile_current(&snapshot).unwrap();

        assert_eq!(draft.generated.reader_facets.len(), 6);
        assert!(
            draft
                .generated
                .files
                .iter()
                .any(|(path, _)| { path.as_str().ends_with("/web/HealthController.java") })
        );
        let api = draft
            .generated
            .reader_facets
            .values()
            .find(|facet| facet.path.as_str() == "load-tests/api.js")
            .expect("missing load-test API projection");
        assert!(matches!(
            api.kind,
            jails_contracts::ReaderFacetKind::ManagedFile {
                mode: jails_contracts::FileMode::Regular
            }
        ));
        let api = String::from_utf8_lossy(&api.bytes);
        assert!(
            api.contains(
                "{ method: \"GET\", path: \"/health\", handler: \"HealthController#get\" }"
            ),
            "{api}"
        );
        assert_eq!(external_project_paths(&draft.next_model).len(), 6);
    }

    #[test]
    fn factory_is_an_ejectable_test_projection_of_the_entity_fields() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  java 26\n  platform spring\n  build maven\n  storage postgres\n}\n\nentity Note {\n  use factory\n  id: uuid @pk\n  title: string @notBlank\n  publishedAt: instant?\n}\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        let draft = compile_current(&snapshot).unwrap();
        let (path, file) = draft
            .generated
            .files
            .iter()
            .find(|(_, file)| file.provenance.artifact_id == "art_ent_note_factory")
            .unwrap();
        assert_eq!(
            path.as_str(),
            ".jails/generated/test/java/com/example/notes/testkit/NoteFactory.java"
        );
        assert_eq!(file.kind, FileKind::JavaTest);
        assert!(file.provenance.ejectable);
        let source = String::from_utf8(file.bytes.clone()).unwrap();
        assert!(source.contains("public static NoteFactory aNote()"));
        assert!(source.contains("withTitle(String value)"));
        assert!(source.contains("Optional<Instant> publishedAt = Optional.empty();"));
        assert!(source.contains("return new Note("));
    }

    #[test]
    fn dto_is_three_independently_mergeable_managed_abi_files() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  java 26\n  platform spring\n  build maven\n  storage postgres\n}\n\nentity Note {\n  use dto\n  id: uuid @pk\n  title: string @notBlank\n  publishedAt: instant?\n}\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let draft = compile_current(&snapshot).unwrap();
        let dto = draft
            .generated
            .files
            .iter()
            .filter(|(_, file)| file.provenance.compiler_pass.starts_with("dto-"))
            .collect::<Vec<_>>();
        assert_eq!(dto.len(), 3);
        assert_eq!(
            dto.iter()
                .map(|(_, file)| file.provenance.artifact_id.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "art_ent_note_dto_request",
                "art_ent_note_dto_response",
                "art_ent_note_dto_test",
            ])
        );
        assert!(
            dto.iter().all(
                |(_, file)| !file.provenance.ejectable && file.provenance.ejection_id.is_none()
            )
        );
        let request = dto
            .iter()
            .find(|(_, file)| file.provenance.artifact_id == "art_ent_note_dto_request")
            .map(|(_, file)| String::from_utf8(file.bytes.clone()).unwrap())
            .unwrap();
        // **The request asks for what the caller may set, and nothing else.**
        // `id` carries a server-assigned default (`uuid7`), so declaring it as
        // a request component would let a caller choose the primary key of the
        // row they are creating -- mass assignment at the HTTP boundary. It is
        // supplied by `toDomain` instead, from the same generator the domain
        // record uses.
        assert!(!request.contains("UUID id"), "{request}");
        assert!(request.contains("TimeOrderedUuid.next()"), "{request}");
        assert!(request.contains("@NotBlank String title"), "{request}");
        assert!(request.contains("Instant publishedAt"), "{request}");
        assert!(
            request.contains("Optional.ofNullable(publishedAt)"),
            "{request}"
        );
        let response = dto
            .iter()
            .find(|(_, file)| file.provenance.artifact_id == "art_ent_note_dto_response")
            .map(|(_, file)| String::from_utf8(file.bytes.clone()).unwrap())
            .unwrap();
        assert!(response.contains("note.publishedAt().orElse(null)"));
        assert!(draft.reader_document_intents.iter().any(|intent| {
            matches!(intent, DocumentIntent::ReconcileDependencies { dependencies }
                if dependencies.iter().any(|dependency| dependency.artifact == "spring-boot-starter-validation"))
        }));
    }

    /// **A required primitive component stays primitive on the wire.**
    ///
    /// A `boolean` domain component
    /// arriving as a `Boolean` in the request and the response, with
    /// `@NotNull` compensating for the boxing, is three defects wearing one
    /// coat -- a wire type that admits a null the domain record cannot hold,
    /// an annotation that only exists because of the boxing, and a response
    /// that can serialise `null` for a field that is never absent.
    ///
    /// The optionality already decides this once, in
    /// [`jails_model::BuiltinType::java_type`], and both DTO faces read it
    /// through `emit_java::java_type`. What this test pins is that they keep
    /// reading it: a boxing regression compiles, passes every golden that has
    /// no primitive field in it, and is visible only in the emitted bytes.
    #[test]
    fn a_required_primitive_is_not_boxed_or_annotated_on_either_dto_face() {
        let model = jails_model::parse_jdl(
            "jdl 1\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  java 26\n  platform spring\n  build maven\n  storage postgres\n}\n\nentity Note {\n  use dto\n  id: uuid @pk\n  done: boolean\n  attempts: int\n  note: string?\n}\n",
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        snapshot.project.build_system = BuildSystem::Maven;
        let draft = compile_current(&snapshot).unwrap();
        let face = |artifact: &str| {
            draft
                .generated
                .files
                .values()
                .find(|file| file.provenance.artifact_id == artifact)
                .map(|file| String::from_utf8(file.bytes.clone()).unwrap())
                .unwrap_or_else(|| panic!("`{artifact}` was not emitted"))
        };
        for artifact in ["art_ent_note_dto_request", "art_ent_note_dto_response"] {
            let source = face(artifact);
            assert!(source.contains("boolean done"), "{artifact}: {source}");
            assert!(source.contains("int attempts"), "{artifact}: {source}");
            // The boxed spellings would still contain the primitive ones as a
            // substring, so the check has to be for the box itself.
            assert!(!source.contains("Boolean done"), "{artifact}: {source}");
            assert!(!source.contains("Integer attempts"), "{artifact}: {source}");
            // `@NotNull` on a primitive is noise at best; Hibernate Validator
            // rejects some constraint/type pairings outright.
            assert!(!source.contains("@NotNull"), "{artifact}: {source}");
        }
        // The optional component is the control: it *is* nullable on the wire,
        // so the absence of boxing above is about optionality rather than
        // about the emitter having stopped boxing anything at all.
        let response = face("art_ent_note_dto_response");
        assert!(response.contains("String note"), "{response}");
        assert!(response.contains("note().orElse(null)"), "{response}");
    }

    #[test]
    fn rich_field_semantics_lower_into_java_and_initial_postgres_schema() {
        let model = jails_model::parse_jdl(
            r#"jdl 1
app Metrics {
  pkg com.example.metrics
  java 26
  platform spring
  build maven
  storage postgres
}

entity Metric {
  use scaffold
  id: long @pk
  score: int @positive
  balance: decimal? @nonnegative
  version: long @version @nonnegative
  createdAt: instant @default(now())
  updatedAt: instant @default(now()) @updated

  command CreateMetric(score) {}

  transition Rescore(score, version) {
    update [score]
    if-match required
  }
}
"#,
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let draft = compile_current(&snapshot).unwrap();

        let record = draft
            .generated
            .files
            .iter()
            .find(|(path, _)| path.as_str().ends_with("/domain/Metric.java"))
            .map(|(_, file)| String::from_utf8(file.bytes.clone()).unwrap())
            .expect("record projection");
        assert!(record.contains("if (score <= 0)"), "{record}");
        assert!(record.contains("score must be positive"), "{record}");
        assert!(
            record.contains("balance.isPresent() && (balance.orElseThrow().signum() < 0)"),
            "{record}"
        );

        let sql = String::from_utf8(draft.migrations[0].bytes.clone()).unwrap();
        assert!(
            sql.contains("id bigint generated always as identity not null primary key"),
            "{sql}"
        );
        assert!(
            sql.contains("score integer not null check (score > 0)"),
            "{sql}"
        );
        assert!(
            sql.contains("balance numeric check (balance >= 0)"),
            "{sql}"
        );
        assert!(sql.contains("version bigint default 0 not null"), "{sql}");
        assert!(
            sql.contains("created_at timestamptz default current_timestamp not null"),
            "{sql}"
        );
        assert!(
            sql.contains("updated_at timestamptz default current_timestamp not null"),
            "{sql}"
        );

        let command = draft
            .generated
            .files
            .iter()
            .find(|(path, _)| {
                path.as_str()
                    .ends_with("/adapters/jdbc/JdbcCreateMetricCommand.java")
            })
            .map(|(_, file)| String::from_utf8(file.bytes.clone()).unwrap())
            .expect("default-aware command adapter");
        assert!(
            command.contains(
                "insert into metrics (score, balance, updated_at) values (:score, :balance, current_timestamp) returning id, score, balance, version, created_at, updated_at"
            ),
            "{command}"
        );
        assert!(!command.contains("param(\"id\""), "{command}");
        assert!(!command.contains("param(\"version\""), "{command}");
        assert!(!command.contains("param(\"created_at\""), "{command}");

        let transition = draft
            .generated
            .files
            .iter()
            .find(|(path, _)| {
                path.as_str()
                    .ends_with("/adapters/jdbc/JdbcRescoreTransition.java")
            })
            .map(|(_, file)| String::from_utf8(file.bytes.clone()).unwrap())
            .expect("versioned transition adapter");
        assert!(
            transition
                .contains("score = :score, version = version + 1, updated_at = current_timestamp"),
            "{transition}"
        );
        // `if-match required` binds the version through the optimistic-lock
        // parameter, not through the ordinary `guard_<column>` equality path a
        // plain version parameter takes; asserting the guard spelling here
        // would pin the wrong one of the two.
        assert!(
            transition.contains("version = :expected_version"),
            "{transition}"
        );
        assert!(
            transition.contains(r#"statement.param("expected_version", expectedVersion)"#),
            "{transition}"
        );
    }

    #[test]
    fn scoped_operations_lower_execution_context_through_every_managed_boundary() {
        let model = jails_model::parse_jdl(
            r#"jdl 1
app Work {
  pkg com.example.work
  java 26
  platform spring
  build maven
  storage postgres
}

cap api
cap security

entity Task {
  use scaffold
  id: uuid @pk
  tenantId: uuid @scope(claim: "tenant")
  title: string
  version: long @version @nonnegative
  updatedAt: instant @default(now()) @updated

  command Create(title) {
    route POST "/tasks"
  }

  query All() {
    route GET "/tasks"
  }

  transition Rename(version, title) {
    update [title]
    if-match required
    route PATCH "/tasks/{id}"
  }
}
"#,
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        let draft = compile_current(&snapshot).unwrap();
        let source = |suffix: &str| {
            draft
                .generated
                .files
                .iter()
                .find(|(path, _)| path.as_str().ends_with(suffix))
                .map(|(_, file)| String::from_utf8(file.bytes.clone()).unwrap())
                .unwrap_or_else(|| panic!("missing generated source `{suffix}`"))
        };

        let context_path = ProjectPath::parse(
            ".jails/generated/main/java/com/example/work/application/ExecutionContext.java",
        )
        .unwrap();
        let context = draft.generated.files.get(&context_path).unwrap();
        assert_eq!(context.provenance.artifact_id, "art_app_execution_context");
        assert!(!context.provenance.ejectable);
        assert!(
            String::from_utf8(context.bytes.clone())
                .unwrap()
                .contains("public String claim(String name)")
        );

        let command_port = source("/application/commands/CreateCommand.java");
        assert!(
            command_port.contains("Task execute(ExecutionContext context, Input input)"),
            "{command_port}"
        );
        let command = source("/adapters/jdbc/JdbcCreateCommand.java");
        assert!(
            command.contains("UUID.fromString(context.claim(\"tenant\"))"),
            "{command}"
        );
        assert!(command.contains("tenant_id"), "{command}");

        let query = source("/adapters/jdbc/JdbcAllQuery.java");
        assert!(query.contains("tenant_id = :scope_tenant_id"), "{query}");
        assert!(
            query.contains(
                "statement.param(\"scope_tenant_id\", UUID.fromString(context.claim(\"tenant\")))"
            ),
            "{query}"
        );

        let transition = source("/adapters/jdbc/JdbcRenameTransition.java");
        assert!(
            transition.contains("tenant_id = :scope_tenant_id"),
            "{transition}"
        );
        assert!(
            transition.contains(
                "execute(ExecutionContext context, UUID id, RenameTransition.Input input, \
                 long expectedVersion)"
            ),
            "{transition}"
        );

        let controller = source("/adapters/http/CreateController.java");
        assert!(
            controller.contains("Authentication authentication"),
            "{controller}"
        );
        assert!(
            controller.contains("Map.entry(\"tenant\", scopes.claim(authentication, \"tenant\"))"),
            "{controller}"
        );
        assert!(
            controller.contains("return operation.execute(context, input)"),
            "{controller}"
        );
        let authorizer = source("/ScopeAuthorizer.java");
        assert!(
            authorizer.contains("public String claim(Authentication authentication, String claim)"),
            "{authorizer}"
        );
    }

    #[test]
    fn constant_assignments_lower_from_rich_command_and_transition_nodes() {
        let model = jails_model::parse_jdl(
            r#"jdl 1
app Work {
  pkg com.example.work
  java 26
  platform spring
  build maven
  storage postgres
}

entity Task {
  use scaffold
  id: uuid @pk
  title: string
  status: string
  updatedAt: instant @updated

  command Open(title) {
    set status = OPEN
  }

  transition Archive() {
    set status = ARCHIVED
    if-match none
  }
}
"#,
        )
        .unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let draft = compile_current(&snapshot).unwrap();
        let source = |suffix: &str| {
            draft
                .generated
                .files
                .iter()
                .find(|(path, _)| path.as_str().ends_with(suffix))
                .map(|(_, file)| String::from_utf8(file.bytes.clone()).unwrap())
                .unwrap_or_else(|| panic!("missing generated source `{suffix}`"))
        };

        let command = source("/adapters/jdbc/JdbcOpenCommand.java");
        assert!(
            command.contains(
                "insert into tasks (id, title, status, updated_at) values (:id, :title, 'OPEN', current_timestamp)"
            ),
            "{command}"
        );
        assert!(!command.contains("param(\"status\""), "{command}");

        let transition = source("/adapters/jdbc/JdbcArchiveTransition.java");
        assert!(
            transition.contains(
                "update tasks set status = 'ARCHIVED', updated_at = current_timestamp where"
            ),
            "{transition}"
        );
        assert!(!transition.contains("param(\"status\""), "{transition}");
    }
    #[test]
    fn a_new_ejection_transfers_matching_units_and_removes_them_from_the_tree() {
        let model = jails_model::parse_jdl(MODEL).unwrap();
        let snapshot = WorkspaceSnapshot::detached(model);
        let before = compile_current(&snapshot).unwrap();
        let ejection = jails_model::Ejection {
            id: jails_model::EjectionId::parse("eject_ent_note").unwrap(),
            label: "note".to_string(),
            target: "art_ent_note_repository_memory".to_string(),
        };
        let mut next = snapshot.model.model.clone();
        next.ejections.insert(ejection.id.clone(), ejection);
        let draft = Compiler::compile(&snapshot, &next, &Evolution::none()).unwrap();
        assert_eq!(
            draft.generated.files.len() + 1,
            before.generated.files.len()
        );
        let transferred = draft
            .reader_document_intents
            .iter()
            .filter(|intent| matches!(intent, DocumentIntent::EjectFile { .. }))
            .count();
        assert_eq!(transferred, 1);
        assert!(
            draft
                .generated
                .files
                .values()
                .any(|file| file.provenance.artifact_id == "art_ent_note_record")
        );
        assert!(draft.generated.files.values().any(|file| {
            file.provenance.artifact_id == "art_cap_fake_script"
                && file.provenance.ejection_id.as_deref() == Some("cap_fake")
        }));
    }

    #[test]
    fn compilation_refuses_a_snapshot_model_disagreement() {
        let model = jails_model::parse_jdl(MODEL).unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.java_release = 21;
        let error = compile_current(&snapshot).unwrap_err();
        assert!(error.to_string().contains("disagrees"));
    }

    #[test]
    fn accepted_projection_is_the_exact_merge_base_across_emitter_versions() {
        let model = jails_model::parse_jdl(MODEL).unwrap();
        let first = compile_current(&WorkspaceSnapshot::detached(model.clone())).unwrap();
        let mut old_projection = first.generated.clone();
        let record = old_projection
            .files
            .values_mut()
            .find(|file| file.provenance.artifact_id == "art_ent_note_record")
            .unwrap();
        record
            .bytes
            .extend_from_slice(b"// old emitter projection\n");

        let mut upgraded = WorkspaceSnapshot::detached(model.clone());
        upgraded.accepted_model = Some(model);
        upgraded.accepted_projection = Some(old_projection.clone());
        upgraded.accepted_compiler = Some("0.0.0-old".to_string());
        let draft = compile_current(&upgraded).unwrap();

        assert_eq!(draft.baseline, old_projection);
        assert_ne!(draft.baseline, draft.generated);
    }

    #[test]
    fn database_capability_lowers_ejectable_operation_implementations() {
        let source = stored_model().replace("cap fake @id(cap_fake)\n\n", "");
        let model = jails_model::parse_jdl(&source).unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let draft = compile_current(&snapshot).unwrap();
        let (path, file) = draft
            .generated
            .files
            .iter()
            .find(|(_, file)| file.provenance.artifact_id == "art_cap_db_op_open_notes_query")
            .unwrap();
        assert_eq!(
            path.as_str(),
            ".jails/generated/main/java/com/example/notes/adapters/jdbc/JdbcOpenNotesQuery.java"
        );
        assert!(file.provenance.ejectable);
        assert_eq!(file.provenance.compiler_pass, "capability-db-query");
        let source = String::from_utf8(file.bytes.clone()).unwrap();
        assert!(source.contains("implements OpenNotesQuery"), "{source}");
        assert!(source.contains("select id, title from note"), "{source}");
        assert!(source.contains("title = :title"), "{source}");
        assert!(source.contains("order by id"), "{source}");
        assert!(source.contains("limit 50"), "{source}");
        assert!(
            source.contains("statement.query(Note.class).list()"),
            "{source}"
        );

        let command = draft
            .generated
            .files
            .values()
            .find(|file| file.provenance.artifact_id == "art_cap_db_op_create_note_command")
            .unwrap();
        assert!(command.provenance.ejectable);
        assert_eq!(command.provenance.compiler_pass, "capability-db-command");
        let command = String::from_utf8(command.bytes.clone()).unwrap();
        assert!(
            command.contains("implements CreateNoteCommand"),
            "{command}"
        );
        assert!(
            command
                .contains("insert into notes (id, title) values (:id, :title) returning id, title"),
            "{command}"
        );
        assert!(command.contains("TimeOrderedUuid.next()"), "{command}");
        let uuid7 = draft
            .generated
            .files
            .values()
            .find(|file| file.provenance.artifact_id == "art_app_time_ordered_uuid")
            .map(|file| String::from_utf8(file.bytes.clone()).unwrap())
            .expect("time-ordered UUID support");
        assert!(uuid7.contains("| 0x70"), "{uuid7}");
        assert!(uuid7.contains("| 0x80"), "{uuid7}");

        let transition = draft
            .generated
            .files
            .values()
            .find(|file| file.provenance.artifact_id == "art_cap_db_op_rename_note_transition")
            .unwrap();
        assert!(transition.provenance.ejectable);
        assert_eq!(
            transition.provenance.compiler_pass,
            "capability-db-transition"
        );
        let transition = String::from_utf8(transition.bytes.clone()).unwrap();
        assert!(
            transition.contains("implements RenameNoteTransition"),
            "{transition}"
        );
        assert!(
            transition.contains("update notes set title = :title where"),
            "{transition}"
        );
        assert!(transition.contains("@Transactional"), "{transition}");
        assert!(
            transition
                .contains("events.publishEvent(new NoteCreatedEvent(result.id(), result.title()))"),
            "{transition}"
        );
    }

    #[test]
    fn equal_inputs_produce_equal_drafts() {
        let model = jails_model::parse_jdl(MODEL).unwrap();
        let snapshot = WorkspaceSnapshot::detached(model);
        let first = compile_current(&snapshot).unwrap();
        let second = compile_current(&snapshot).unwrap();
        assert_eq!(first, second);
        assert!(
            first
                .generated
                .files
                .values()
                .map(|file| file.provenance.artifact_id.as_str())
                .collect::<BTreeSet<_>>()
                .len()
                == first.generated.files.len()
        );
        assert!(first.generated.files.keys().any(|path| {
            path.as_str()
                .ends_with("application/commands/CreateNoteCommand.java")
        }));
        assert!(first.generated.files.keys().any(|path| {
            path.as_str()
                .ends_with("adapters/memory/InMemoryNoteRepository.java")
        }));
        assert!(first.generated.files.keys().any(|path| {
            path.as_str()
                .ends_with("test/java/com/example/notes/testkit/Fake.java")
        }));
        assert!(first.generated.files.keys().any(|path| {
            path.as_str()
                .ends_with("test/java/com/example/notes/testkit/FakeTest.java")
        }));
    }

    #[test]
    fn required_direct_model_edit_refuses_without_an_explicit_backfill_policy() {
        let accepted = jails_model::parse_jdl(&stored_model()).unwrap();
        let next = jails_model::parse_jdl(&stored_model_with_summary("summary: string")).unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(next);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        snapshot.accepted_model = Some(accepted);
        let error = compile_current(&snapshot).unwrap_err();
        assert!(error.to_string().contains("needs a backfill"), "{error}");
    }

    #[test]
    fn accepted_schema_and_field_policy_lower_one_forward_add_column() {
        let model = jails_model::parse_jdl(&stored_model()).unwrap();
        let next = jails_model::parse_jdl(&stored_model_with_summary("summary: string?")).unwrap();
        let field_id = FieldId::parse("fld_note_summary").unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model.clone());
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        snapshot.accepted_model = Some(model);
        snapshot.migration_history.records.push(MigrationRecord {
            version: "1".to_string(),
            path: ProjectPath::parse("src/main/resources/db/migration/V001__create_note.sql")
                .unwrap(),
            digest: ContentDigest::parse(format!("sha256:{}", "0".repeat(64))).unwrap(),
        });
        let draft = Compiler::compile(
            &snapshot,
            &next,
            &Evolution::one(jails_model::EvolutionStep::AddField {
                field: field_id,
                policy: FieldAddPolicy::Nullable,
            }),
        )
        .unwrap();
        assert_eq!(draft.migrations.len(), 1);
        assert_eq!(draft.migrations[0].logical_name, "add_summary_to_notes");
        let sql = String::from_utf8(draft.migrations[0].bytes.clone()).unwrap();
        assert_eq!(
            sql,
            "-- Generated by jails from the accepted semantic schema.\nalter table notes add column summary text;\n"
        );
    }

    #[test]
    fn database_capability_lowers_storage_dependencies_adapter_and_initial_schema() {
        let model = jails_model::parse_jdl(&stored_model()).unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        // The adapters this asserts on are `JdbcClient` classes annotated
        // `@Repository`, so the capability needs a Boot project to compile
        // into; without a parent, a versionless dependency set reaches a pom
        // nothing manages.
        snapshot.project.spring_boot = Some("4.0.0".to_string());
        let draft = compile_current(&snapshot).unwrap();
        assert_eq!(draft.migrations.len(), 1);
        assert_eq!(draft.migrations[0].logical_name, "create_notes");
        let sql = String::from_utf8(draft.migrations[0].bytes.clone()).unwrap();
        assert!(sql.contains("create table notes"), "{sql}");
        assert!(sql.contains("id uuid not null primary key"), "{sql}");
        assert!(draft.generated.files.keys().any(|path| {
            path.as_str()
                .ends_with("adapters/jdbc/JdbcNoteRepository.java")
        }));
        assert!(draft.reader_document_intents.iter().any(|intent| {
            matches!(intent, DocumentIntent::ReconcileDependencies { dependencies }
                if dependencies.iter().any(|dependency| dependency.artifact == "spring-boot-starter-jdbc"))
        }));
    }

    #[test]
    fn managed_abi_cannot_be_ejected() {
        let source = format!("{MODEL}\neject id(art_ent_note_record) @id(eject_ent_note)\n");
        let model = jails_model::parse_jdl(&source).unwrap();
        let snapshot = WorkspaceSnapshot::detached(model);
        let error = compile_current(&snapshot).unwrap_err();
        assert!(error.to_string().contains("managed ABI"), "{error}");
    }

    #[test]
    fn an_ejection_that_emits_nothing_is_rejected() {
        let source = format!("{MODEL}\neject id(art_missing_repository) @id(eject_database)\n");
        let model = jails_model::parse_jdl(&source).unwrap();
        let snapshot = WorkspaceSnapshot::detached(model);
        let error = compile_current(&snapshot).unwrap_err();
        assert!(
            error.to_string().contains("emits no ejectable Java"),
            "{error}"
        );
    }
}
