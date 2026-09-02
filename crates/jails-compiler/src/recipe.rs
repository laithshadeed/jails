//! `Recipe`: the one declarative shape a model node renders through.
//!
//! A recipe is what a capability pack always was -- files, dependencies,
//! properties, compose services, build features and a placement rule, as one
//! `static` -- generalised over *which* model node it renders from. A
//! capability and a component are both a [`Node`]: something with a stable
//! id, a name, and a closed vocabulary of typed values a template may spell
//! as `{{key}}`. Rendering is one loop, [`render`], and every file it writes
//! goes through the one `JavaUnit` shell.
//!
//! **A role appears in exactly one recipe.** A file's `role` is the suffix of
//! its artifact id (`art_<node>_<role>`), which is what the merge is keyed on
//! and what `eject` names, and [`Import::Role`] resolves another file of the
//! same node by looking its role up in this recipe rather than by spelling
//! the class name a second time -- an emitter asking for a role the recipe
//! does not carry is a compile-time refusal, not a wrong import.
//!
//! What a template needs that is *structural* -- a list of registrations, a
//! block that belongs only when some other capability is declared -- is a
//! [`Fragment`] named on the recipe, rendered once per node and substituted
//! like any other key. The emitters that cannot be a recipe (SQL lowering,
//! the proof tests, anything that reaches across nodes for a sample) stay
//! functions, and `emit.rs` says which those are.

use crate::CompileError;
use crate::emit_java::JavaUnit;
use jails_contracts::{
    BuildFeature, FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedTree,
    WorkspaceSnapshot,
};
use jails_model::{AppModel, DependencyScope, Package, SettingTarget};
use std::collections::BTreeSet;

pub(crate) const MAIN_ROOT: &str = ".jails/generated/main/java";
pub(crate) const TEST_ROOT: &str = ".jails/generated/test/java";

/// A model node a recipe renders from.
///
/// The trait carries only what differs between node kinds: how the node is
/// named in a refusal, which keys its templates may spell, and the provenance
/// its files carry. Everything else is the recipe's.
pub(crate) trait Node: 'static {
    /// The closed vocabulary of typed values this node kind's templates may
    /// spell. A recipe names the ones it needs; [`Node::key`] renders one.
    type Key: Copy + 'static;

    fn id(&self) -> &str;
    /// The Java-shaped name [`Naming`] derives class names from.
    fn name(&self) -> &str;
    /// How a refusal names this node: `component auth \`Session\``.
    fn describe(&self) -> String;
    /// Render one of this node's typed values as `(placeholder, value)`.
    fn key(&self, model: &AppModel, key: Self::Key)
    -> Result<(&'static str, String), CompileError>;
    /// Keys every file of this node gets, given the file's package and the
    /// class its template is written against.
    fn file_keys(&self, package: &str, template_class: &str) -> Vec<(&'static str, String)>;
    /// The provenance one of this node's files carries; `pass` is the
    /// recipe's [`Recipe::pass`], for a node kind more than one recipe
    /// renders.
    fn provenance(&self, artifact_id: String, ejectable: bool, pass: &'static str) -> Provenance;
    /// Whether the rendered file opens with the provenance header.
    ///
    /// A capability's Java carries none, because `remove` retires the whole
    /// file rather than reconciling its bytes; a generated component's does.
    fn header(&self) -> bool;
    /// Whether a test in this source set gets `@Import(TestcontainersConfig.class)`
    /// spliced in when the model declares `db`.
    fn splices_test_container(&self, source_set: SourceSet) -> bool;
}

/// One declarative renderer: everything a node kind emits, as data.
pub(crate) struct Recipe<N: Node> {
    /// What this recipe's own templates spell as `{{key}}`: an image tag, a
    /// URL, a route segment.
    ///
    /// **On the row, not in one bag every recipe is substituted through.** A
    /// shared list applies `redis:7-alpine` to `mail`'s templates and
    /// `axllent/mailpit` to `redis`'s -- harmless only for as long as no two
    /// recipes pick the same key, which is a property nothing checks and
    /// which the next pinned image breaks.
    pub(crate) substitutions: &'static [(&'static str, &'static str)],
    /// The node's typed values these templates spell.
    pub(crate) keys: &'static [N::Key],
    pub(crate) fragments: &'static [Fragment<N>],
    /// What must be true of the model before any of this renders.
    pub(crate) requires: &'static [Need],
    pub(crate) files: &'static [JavaFile<N>],
    pub(crate) files_when: BootCondition,
    pub(crate) resources: &'static [ResourceFile],
    pub(crate) dependencies: &'static [DependencySpec],
    pub(crate) properties: &'static [PropertySpec],
    pub(crate) compose_services: &'static [ComposeService],
    pub(crate) build_features: &'static [BuildFeature],
    /// Where a file placed at [`Placement::Default`] goes.
    pub(crate) default_package: fn(&AppModel, &N) -> String,
    /// The `compiler_pass` the provenance of this recipe's files names, where
    /// the node kind does not decide it.
    pub(crate) pass: &'static str,
    /// The Boot major this recipe's *main* source needs, and the type that
    /// needs it.
    ///
    /// **The type, because that is what the compiler would have said.** "this
    /// project uses Boot 2" is true of everything jails refuses on an old
    /// project; `ProblemDetail` is the one line a reader can act on.
    pub(crate) minimum_boot: Option<(u32, &'static str)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceSet {
    Main,
    Test,
    IntegrationTest,
}

impl SourceSet {
    pub(crate) fn root(self) -> &'static str {
        match self {
            Self::Main => MAIN_ROOT,
            Self::Test | Self::IntegrationTest => TEST_ROOT,
        }
    }

    pub(crate) fn kind(self) -> FileKind {
        match self {
            Self::Main => FileKind::JavaMain,
            Self::Test | Self::IntegrationTest => FileKind::JavaTest,
        }
    }
}

/// Which package a file lands in.
#[derive(Clone, Copy)]
pub(crate) enum Placement {
    /// The recipe's default package: a capability's `--package`, or the layer
    /// its rows name.
    Default,
    /// One layer's package, whatever the default is.
    Layer(Package),
}

/// How a file's class is named from its node.
pub(crate) enum Naming<N: Node> {
    /// The same name whatever the node is called.
    Fixed(&'static str),
    /// The node's name followed by this.
    Suffix(&'static str),
    /// The node's name between these two.
    Wrap(&'static str, &'static str),
    /// A rule the closed forms above cannot spell.
    By(fn(&N) -> String),
}

impl<N: Node> Naming<N> {
    pub(crate) fn resolve(&self, node: &N) -> String {
        match self {
            Self::Fixed(name) => (*name).to_string(),
            Self::Suffix(suffix) => format!("{}{suffix}", node.name()),
            Self::Wrap(prefix, suffix) => format!("{prefix}{}{suffix}", node.name()),
            Self::By(rule) => rule(node),
        }
    }
}

pub(crate) struct JavaFile<N: Node> {
    /// The suffix of the artifact id, and what [`Import::Role`] looks up.
    pub(crate) role: &'static str,
    pub(crate) template: crate::Template,
    pub(crate) before_boot: Option<(u32, crate::Template)>,
    /// What this file imports that its template cannot state for itself.
    pub(crate) imports: &'static [Import<N>],
    /// A file that belongs only to some nodes: a proof that needs an `id`
    /// to match on, a sink that needs a broker declared.
    pub(crate) only_when: Option<fn(&AppModel, &N) -> bool>,
    pub(crate) source_set: SourceSet,
    pub(crate) placement: Placement,
    /// Whether `model eject` may transfer this file into reader source. A
    /// port or a record is managed ABI and stays.
    pub(crate) ejectable: bool,
    pub(crate) class: Naming<N>,
    /// The class the template is written against, when a test's template
    /// spells the class under test rather than its own.
    pub(crate) template_class: Naming<N>,
}

/// An import a recipe's row names rather than its template.
///
/// **A `.java` template cannot carry a conditional import line**, and every
/// case below is conditional. Naming them on the row keeps the template a
/// real Java file and keeps the decision beside the rest of the recipe's
/// data, where the next reader looks.
pub(crate) enum Import<N: Node> {
    /// A class of the recipe's *default* package this file names. The
    /// statement is needed only when the file is placed somewhere else, which
    /// `JavaUnit::import_from` decides.
    Own(&'static str),
    /// Another file of the same node, by role: its package and class are
    /// looked up in the recipe rather than spelled here a second time.
    Role(&'static str),
    /// A class of one layer's package: something shared by every node of
    /// this kind rather than a file of this node.
    From(Package, &'static str),
    /// The class one of the node's keys names, in one layer's package: the
    /// event record a publisher publishes, the publisher a sink delivers to.
    Keyed(Package, N::Key),
    /// A type whose package the captured Boot version decides.
    Moved(MovedImport),
    /// What an integration test needs to reach the container config: either
    /// the `@Import` or `@Disabled`, with whatever each names, substituted as
    /// `{{container_annotation}}` and `{{annotation}}`.
    ContainerSupport,
}

/// A type whose package a Spring Boot major moved.
///
/// **A version fact read off the captured project, never assumed.** Boot 4
/// moved `@AutoConfigureMockMvc`, `@WebMvcTest` and `MeterRegistryCustomizer`
/// with no shim, so a file naming the wrong one fails on a package that does
/// not exist -- in a file the reader did not write, which is exactly the
/// compile error a generator exists to remove. Both spellings sit here side by
/// side so the pair cannot be edited one at a time.
#[derive(Clone, Copy)]
pub(crate) struct MovedImport {
    /// The Boot major that moved it.
    pub(crate) moved_at: u32,
    /// Where it lives from that major up.
    pub(crate) at_or_above: &'static str,
    /// Where it lived below -- and the answer when the version cannot be read
    /// at all, because a project too old to have the new package is exactly
    /// the project that would fail to compile.
    pub(crate) below: &'static str,
}

impl MovedImport {
    pub(crate) fn resolve(self, boot_major: Option<u32>) -> &'static str {
        match boot_major.is_some_and(|major| major >= self.moved_at) {
            true => self.at_or_above,
            false => self.below,
        }
    }
}

pub(crate) struct DependencySpec {
    pub(crate) group: &'static str,
    pub(crate) artifact: &'static str,
    pub(crate) version: Option<&'static str>,
    pub(crate) scope: DependencyScope,
    pub(crate) spring_managed_version: bool,
    pub(crate) only_when_build_exists: bool,
    /// Maven's `<optional>true</optional>`. Boot's own starters mark
    /// `spring-boot-docker-compose` and devtools this way and Spring
    /// Initializr copies them, so a pom that omits it differs from the one the
    /// same choices produce on start.spring.io.
    pub(crate) optional: bool,
    pub(crate) boot: BootCondition,
}

impl DependencySpec {
    /// A versionless compile dependency, which is what a generated component
    /// needs: correct under `spring-boot-starter-parent`, and a `<version>`
    /// invented here would pin a starter against the reader's Boot.
    pub(crate) const fn managed(group: &'static str, artifact: &'static str) -> Self {
        Self {
            group,
            artifact,
            version: None,
            scope: DependencyScope::Compile,
            spring_managed_version: true,
            only_when_build_exists: false,
            optional: false,
            boot: BootCondition::Any,
        }
    }
}

pub(crate) struct ResourceFile {
    pub(crate) suffix: &'static str,
    pub(crate) path: &'static str,
    pub(crate) bytes: &'static str,
    pub(crate) source_set: SourceSet,
}

/// One `application.properties` entry. Its key and value are substituted
/// with the node's keys, so `app.{{property}}.secret` names the node.
pub(crate) struct PropertySpec {
    pub(crate) key: &'static str,
    pub(crate) value: &'static str,
    pub(crate) target: SettingTarget,
    pub(crate) boot: BootCondition,
}

pub(crate) struct ComposeService {
    pub(crate) name: &'static str,
    pub(crate) marker: &'static str,
    pub(crate) body: &'static str,
}

#[derive(Clone, Copy)]
pub(crate) enum BootCondition {
    Any,
    Spring,
    Plain,
    AtLeast(u32),
    Before(u32),
}

impl BootCondition {
    pub(crate) fn matches(self, major: Option<u32>) -> bool {
        match self {
            Self::Any => true,
            Self::Spring => major.is_some(),
            Self::Plain => major.is_none(),
            Self::AtLeast(minimum) => major.is_some_and(|major| major >= minimum),
            Self::Before(limit) => major.is_some_and(|major| major < limit),
        }
    }
}

/// A block of a template that is rendered rather than substituted.
pub(crate) enum Fragment<N: Node> {
    /// A block that only belongs in the file when the model also declares
    /// some other capability.
    ///
    /// **The advice's `DuplicateKeyException` arm is why this exists.** jails
    /// puts `@unique` in the schema and generates an `ApiException.Conflict`
    /// documented "becomes a 409", and the arm is what joins the two --
    /// without it a duplicate insert answers 500, which is what alerting pages
    /// on and what clients retry. The arm cannot be unconditional:
    /// `DuplicateKeyException` is Spring's, from `spring-tx`, which arrives
    /// with the JDBC starter, and `api` does not require a database.
    ///
    /// There is no ordering trap: the compiler compiles the whole model at
    /// once, so "does this model declare `db`" is a question with one answer.
    WhenCapability {
        key: &'static str,
        capability: &'static str,
        body: &'static str,
    },
    /// A structural block -- a list, a switch, a sample argument list --
    /// rendered by one named function of the node and the model. The names
    /// it spells join the import set of every file that spells its key, and
    /// no other's.
    Rendered {
        key: &'static str,
        render: fn(&AppModel, &N) -> Result<Rendered, CompileError>,
    },
}

/// What a rendered fragment is: text, and the fully-qualified names the text
/// relies on.
///
/// **A literal is not import-free.** `UUID.fromString(..)` and
/// `Instant.parse(..)` are types, and a test given only the record's own
/// import compiles exactly as long as every component happens to be a
/// `String`.
pub(crate) struct Rendered {
    pub(crate) text: String,
    pub(crate) imports: BTreeSet<String>,
}

impl From<String> for Rendered {
    fn from(text: String) -> Self {
        Self {
            text,
            imports: BTreeSet::new(),
        }
    }
}

/// Something the model must declare before a recipe renders.
pub(crate) struct Need {
    pub(crate) want: Want,
    /// Why, in the refusal's words: `needs PostgreSQL/JDBC to keep receipts
    /// across restarts`.
    pub(crate) why: &'static str,
}

#[derive(Clone, Copy)]
pub(crate) enum Want {
    /// SQL reachable from this project, however it got there: the `db`
    /// capability or a JDBC starter the reader declared.
    Database,
    /// A capability, by kind.
    Capability(&'static str),
}

impl Want {
    fn satisfied(self, model: &AppModel) -> bool {
        match self {
            Self::Database => has_database(model),
            Self::Capability(kind) => declares(model, kind),
        }
    }

    fn fix(self) -> String {
        match self {
            Self::Database => {
                "declare `storage postgres` in the model, or run `jails add db`".to_string()
            }
            Self::Capability(kind) => {
                format!("declare `cap {kind}` in the model, or run `jails add {kind}`")
            }
        }
    }
}

/// Whether SQL is reachable from this project, however it got there.
///
/// **Not "did jails install the database".** A project can carry the JDBC
/// starter because the reader declared it -- a Spring application running on
/// an H2 file -- and a component that refuses there is refusing over a
/// database that is present. What the `db` capability additionally supplies
/// is a `TestcontainersConfig`, which is a separate question with a separate
/// answer in [`container_support`].
pub(crate) fn has_database(model: &AppModel) -> bool {
    declares(model, "db")
        || model
            .dependencies
            .values()
            .any(|dependency| JDBC_STARTERS.contains(&dependency.artifact.as_str()))
}

/// The artifacts that put a `DataSource` and `JdbcClient` on the classpath.
const JDBC_STARTERS: [&str; 2] = ["spring-boot-starter-jdbc", "spring-boot-starter-data-jdbc"];

pub(crate) fn declares(model: &AppModel, kind: &str) -> bool {
    model
        .capabilities
        .values()
        .any(|capability| capability.kind == kind)
}

/// What an integration test needs to reach a container config.
///
/// One decision: either the `@Import` or `@Disabled`, with whatever each
/// names. Emitting the annotation over a config the model never declared hands
/// the reader a `cannot find symbol` in a file they did not write, and
/// emitting nothing drops the coverage silently.
///
/// The imports are names for the unit's set rather than statements, so an
/// integration test that also imports something of its own gets one block.
pub(crate) fn container_support(model: &AppModel) -> ContainerSupport {
    if !declares(model, "db") {
        return ContainerSupport {
            container: None,
            annotation: "",
            disabled: "@Disabled(\"todo: run jails add db to generate TestcontainersConfig, \
                       or point this at the database this project already has\")\n",
        };
    }
    ContainerSupport {
        container: Some(model.project.package_for(Package::Base)),
        annotation: "@Import(TestcontainersConfig.class)\n",
        disabled: "",
    }
}

pub(crate) struct ContainerSupport {
    /// The package `TestcontainersConfig` is in, when there is one.
    container: Option<String>,
    pub(crate) annotation: &'static str,
    pub(crate) disabled: &'static str,
}

impl ContainerSupport {
    /// Add what the annotation this support renders names.
    pub(crate) fn declare(&self, unit: &mut JavaUnit) {
        match &self.container {
            Some(base) => {
                unit.import("org.springframework.context.annotation.Import");
                unit.import_from(base, "TestcontainersConfig");
            }
            None => unit.import("org.junit.jupiter.api.Disabled"),
        }
    }
}

/// Where one of a recipe's files goes for this node.
pub(crate) fn package_of<N: Node>(
    model: &AppModel,
    node: &N,
    recipe: &Recipe<N>,
    file: &JavaFile<N>,
) -> String {
    match file.placement {
        Placement::Default => (recipe.default_package)(model, node),
        Placement::Layer(package) => model.project.package_for(package),
    }
}

/// Refuse when the model lacks what this recipe needs.
pub(crate) fn check_needs<N: Node>(
    model: &AppModel,
    node: &N,
    recipe: &Recipe<N>,
) -> Result<(), CompileError> {
    for need in recipe.requires {
        if !need.want.satisfied(model) {
            return Err(CompileError::new(format!(
                "{} {}\n       fix: {}",
                node.describe(),
                need.why,
                need.want.fix()
            )));
        }
    }
    Ok(())
}

/// The node's typed values, rendered once per node.
pub(crate) fn node_keys<N: Node>(
    model: &AppModel,
    node: &N,
    recipe: &Recipe<N>,
) -> Result<Vec<(&'static str, String)>, CompileError> {
    recipe
        .keys
        .iter()
        .map(|key| node.key(model, *key))
        .collect()
}

/// Render every Java file of one node's recipe into the tree.
///
/// Resources, compose services and the reader-owned project files are not
/// Java and are rendered by the capability walk beside its rows; this is the
/// one loop for the part every node kind shares.
pub(crate) fn render<N: Node>(
    model: &AppModel,
    node: &N,
    recipe: &Recipe<N>,
    snapshot: &WorkspaceSnapshot,
    output: &mut RenderedTree,
) -> Result<(), CompileError> {
    check_needs(model, node, recipe)?;
    let boot_major = crate::emit_capability::boot_major(snapshot.project.spring_boot.as_deref());
    let default_package = (recipe.default_package)(model, node);
    let keys = node_keys(model, node, recipe)?;
    // Resolved once per node: a fragment whose capability the model does not
    // declare substitutes to nothing, rather than being left in the file as a
    // literal `{{key}}`.
    let fragments = recipe
        .fragments
        .iter()
        .map(|fragment| match fragment {
            Fragment::WhenCapability {
                key,
                capability,
                body,
            } => {
                let body = match declares(model, capability) {
                    true => (*body).to_string(),
                    false => String::new(),
                };
                Ok((*key, Rendered::from(body)))
            }
            Fragment::Rendered { key, render } => Ok((*key, render(model, node)?)),
        })
        .collect::<Result<Vec<_>, CompileError>>()?;
    for file in recipe
        .files
        .iter()
        .filter(|_| recipe.files_when.matches(boot_major))
        .filter(|file| file.only_when.is_none_or(|applies| applies(model, node)))
    {
        let package = package_of(model, node, recipe, file);
        let class = file.class.resolve(node);
        let template_class = file.template_class.resolve(node);
        let template = match file.before_boot {
            Some((limit, template)) if boot_major.is_some_and(|major| major < limit) => template,
            _ => file.template,
        };
        let mut text = template.resolve(&snapshot.template_overrides)?.to_string();
        // Substitution only: what varies structurally is a fragment rendered
        // above, and an import the file needs is a name on its row that
        // `JavaUnit` adds to the one import block -- never a placeholder here,
        // because a rendered `import` statement is exactly what makes two
        // emitters able to write one twice.
        let support = file
            .imports
            .iter()
            .any(|import| matches!(import, Import::ContainerSupport))
            .then(|| container_support(model));
        if let Some(support) = &support {
            text = text
                .replace("{{container_annotation}}", support.annotation)
                .replace("{{annotation}}", support.disabled);
        }
        let mut fragment_imports = BTreeSet::new();
        for (key, fragment) in &fragments {
            let placeholder = format!("{{{{{key}}}}}");
            if !text.contains(&placeholder) {
                continue;
            }
            text = text.replace(&placeholder, &fragment.text);
            fragment_imports.extend(fragment.imports.iter().cloned());
        }
        for (key, value) in recipe
            .substitutions
            .iter()
            .copied()
            .chain(keys.iter().map(|(key, value)| (*key, value.as_str())))
        {
            text = text.replace(&format!("{{{{{key}}}}}"), value);
        }
        text = text.replace("{{pkg}}", &package);
        for (key, value) in node.file_keys(&package, &template_class) {
            text = text.replace(&format!("{{{{{key}}}}}"), &value);
        }
        let mut unit = JavaUnit::from_source(&text);
        for import in file.imports {
            match import {
                Import::Own(class) => unit.import_from(&default_package, class),
                Import::Role(role) => {
                    let other = file_with_role(recipe, role);
                    unit.import_from(
                        &package_of(model, node, recipe, other),
                        &other.class.resolve(node),
                    );
                }
                Import::From(package, class) => {
                    unit.import_from(&model.project.package_for(*package), class);
                }
                Import::Keyed(package, key) => {
                    let (_, class) = node.key(model, *key)?;
                    unit.import_from(&model.project.package_for(*package), &class);
                }
                Import::Moved(moved) => unit.import(moved.resolve(boot_major)),
                Import::ContainerSupport => {}
            }
        }
        for name in fragment_imports {
            unit.import(name);
        }
        if let Some(support) = &support {
            support.declare(&mut unit);
        }
        if node.splices_test_container(file.source_set) {
            crate::emit_capability::imported_test_container(model, &mut unit);
        }
        let artifact_id = format!("art_{}_{}", node.id(), file.role);
        let bytes = match node.header() {
            true => unit.render(&artifact_id),
            false => unit.source(),
        };
        let path = ProjectPath::parse(format!(
            "{}/{}/{class}.java",
            file.source_set.root(),
            package.replace('.', "/")
        ))
        .map_err(CompileError::new)?;
        output
            .insert(
                path,
                RenderedFile {
                    kind: file.source_set.kind(),
                    mode: FileMode::Regular,
                    bytes: bytes.into_bytes(),
                    provenance: node.provenance(artifact_id, file.ejectable, recipe.pass),
                },
            )
            .map_err(CompileError::new)?;
    }
    Ok(())
}

/// The file of this recipe carrying `role`.
///
/// A role is registered exactly once, so a miss is a recipe row naming a
/// role its own table does not carry -- a programming error, and one the
/// exhaustiveness test in this module's tests reaches before any model does.
fn file_with_role<'a, N: Node>(recipe: &'a Recipe<N>, role: &str) -> &'a JavaFile<N> {
    recipe
        .files
        .iter()
        .find(|file| file.role == role)
        .unwrap_or_else(|| panic!("recipe row imports role `{role}`, which no file of it carries"))
}

/// The build dependencies one recipe declares for one project.
pub(crate) fn dependencies<N: Node>(
    recipe: &Recipe<N>,
    spring_boot: Option<&str>,
    build_exists: bool,
) -> impl Iterator<Item = jails_contracts::BuildDependency> {
    let boot_major = crate::emit_capability::boot_major(spring_boot);
    let spring = spring_boot.is_some();
    recipe
        .dependencies
        .iter()
        .filter(move |dependency| build_exists || !dependency.only_when_build_exists)
        .filter(move |dependency| dependency.boot.matches(boot_major))
        .map(move |dependency| jails_contracts::BuildDependency {
            group: dependency.group.to_string(),
            artifact: dependency.artifact.to_string(),
            version: dependency
                .version
                .filter(|_| !spring || !dependency.spring_managed_version)
                .map(str::to_string),
            scope: dependency.scope,
            optional: dependency.optional,
        })
}

/// The `application.properties` entries one recipe declares for one node,
/// its key and value spelled with the node's keys.
pub(crate) fn properties<N: Node>(
    model: &AppModel,
    node: &N,
    recipe: &Recipe<N>,
    target: SettingTarget,
) -> Result<Vec<jails_contracts::PropertyEntry>, CompileError> {
    let keys = node_keys(model, node, recipe)?;
    let spell = |text: &str| {
        keys.iter().fold(text.to_string(), |text, (key, value)| {
            text.replace(&format!("{{{{{key}}}}}"), value)
        })
    };
    Ok(recipe
        .properties
        .iter()
        .filter(|property| property.target == target)
        .map(|property| jails_contracts::PropertyEntry {
            key: spell(property.key),
            value: spell(property.value),
        })
        .collect())
}

/// The build features one recipe declares: its own rows, plus the Failsafe
/// plugin for any integration test it emits.
pub(crate) fn build_features<N: Node>(recipe: &Recipe<N>) -> BTreeSet<BuildFeature> {
    let mut features = recipe
        .build_features
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if recipe
        .files
        .iter()
        .any(|file| file.source_set == SourceSet::IntegrationTest)
    {
        features.insert(BuildFeature::IntegrationTests);
    }
    features
}
