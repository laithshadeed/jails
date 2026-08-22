//! Pure project and intent values shared by planning, rendering, and apply.
//!
//! The command modules used to pass a bare `root: &Path` and rediscover the
//! POM, flavor, base package, configured layers, and installed capabilities at
//! every layer of the call graph. `Project` is the single resolved snapshot
//! handed to planners instead. It is deliberately loaded at the CLI boundary;
//! planning code must not reach back into the filesystem for facts already
//! represented here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::Result;
use crate::compose::Service as ComposeService;
use crate::config::Config;
use crate::pom::{self, Dependency, Flavor};

/// One file a recipe intends to create.
///
/// The rendered string is deliberately still eager at rung 2. Rung 4 changes
/// it to `Body` after every producer uses this one shape; doing both migrations
/// at once would make a behavioral regression harder to localise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Artifact {
    pub(crate) kind: &'static str,
    pub(crate) path: PathBuf,
    pub(crate) contents: String,
}

impl Artifact {
    pub(crate) fn rendered(path: PathBuf, contents: String) -> Self {
        Self {
            kind: "capability file",
            path,
            contents,
        }
    }
}

/// A test-classpath import owned by a capability change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SpringTestImport {
    pub(crate) pkg: String,
    pub(crate) class: &'static str,
}

impl SpringTestImport {
    pub(crate) fn fqcn(&self) -> String {
        format!("{}.{}", self.pkg, self.class)
    }
}

/// Everything one recipe intends to change, computed before it is applied.
///
/// This is the shared command value. Capabilities use all of it; generators
/// initially use the file subset and then migrate their dependency/codemod
/// tails into the same value.
#[derive(Clone, Debug, Default)]
pub(crate) struct Change {
    pub(crate) deps: Vec<Dependency>,
    pub(crate) plugins: Vec<(&'static str, String)>,
    pub(crate) files: Vec<Artifact>,
    pub(crate) compose: Vec<ComposeService>,
    pub(crate) properties: Vec<String>,
    pub(crate) legacy_deps: Vec<Dependency>,
    pub(crate) spring_test_import: Option<SpringTestImport>,
}

impl Change {
    /// Associatively combine independently planned recipe changes.
    ///
    /// Equal contributions collapse; two different contributions claiming
    /// the same identity are rejected before either reaches disk. This is the
    /// algebra used by multi-capability and whole-manifest planning.
    pub(crate) fn merge(mut self, other: Self) -> Result<Self> {
        for dep in other.deps {
            match self.deps.iter().find(|current| {
                current.group_id == dep.group_id && current.artifact_id == dep.artifact_id
            }) {
                Some(current) if current != &dep => {
                    return Err(format!(
                        "conflicting dependency plans for {}:{}",
                        dep.group_id, dep.artifact_id
                    ));
                }
                Some(_) => {}
                None => self.deps.push(dep),
            }
        }
        for (artifact_id, body) in other.plugins {
            match self
                .plugins
                .iter()
                .find(|(current, _)| *current == artifact_id)
            {
                Some((_, current)) if current != &body => {
                    return Err(format!("conflicting plugin plans for {artifact_id}"));
                }
                Some(_) => {}
                None => self.plugins.push((artifact_id, body)),
            }
        }
        for file in other.files {
            match self.files.iter().find(|current| current.path == file.path) {
                Some(current) if current.contents != file.contents => {
                    return Err(format!(
                        "two recipes would write different contents to {}",
                        file.path.display()
                    ));
                }
                Some(_) => {}
                None => self.files.push(file),
            }
        }
        for service in other.compose {
            match self
                .compose
                .iter()
                .find(|current| current.name == service.name)
            {
                Some(current) if current != &service => {
                    return Err(format!(
                        "conflicting compose service plans for {}",
                        service.name
                    ));
                }
                Some(_) => {}
                None => self.compose.push(service),
            }
        }
        for property in other.properties {
            if !self.properties.contains(&property) {
                self.properties.push(property);
            }
        }
        for dep in other.legacy_deps {
            if !self.legacy_deps.iter().any(|current| {
                current.group_id == dep.group_id && current.artifact_id == dep.artifact_id
            }) {
                self.legacy_deps.push(dep);
            }
        }
        self.spring_test_import = match (self.spring_test_import, other.spring_test_import) {
            (Some(current), Some(next)) if current != next => {
                return Err("two recipes require different Spring test imports".to_string());
            }
            (Some(current), _) => Some(current),
            (None, next) => next,
        };
        Ok(self)
    }
}

/// The conventional package roles understood by jails.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Layer {
    Domain,
    App,
    Service,
    Web,
    Api,
    Messaging,
    Cli,
    Clients,
    Jobs,
    Adapters,
    Testkit,
}

impl Layer {
    const ALL: [Self; 11] = [
        Self::Domain,
        Self::App,
        Self::Service,
        Self::Web,
        Self::Api,
        Self::Messaging,
        Self::Cli,
        Self::Clients,
        Self::Jobs,
        Self::Adapters,
        Self::Testkit,
    ];

    pub(crate) const fn key(self) -> &'static str {
        use crate::generate::layout;
        match self {
            Self::Domain => layout::DOMAIN,
            Self::App => layout::APP,
            Self::Service => layout::SERVICE,
            Self::Web => layout::WEB,
            Self::Api => layout::API,
            Self::Messaging => layout::MESSAGING,
            Self::Cli => layout::CLI,
            Self::Clients => layout::CLIENTS,
            Self::Jobs => layout::JOBS,
            Self::Adapters => layout::ADAPTERS,
            Self::Testkit => layout::TESTKIT,
        }
    }
}

/// Layer package names with every `jails.toml` override already applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Layers {
    packages: BTreeMap<String, String>,
}

impl Layers {
    fn from_config(config: &Config) -> Self {
        Self {
            packages: Layer::ALL
                .into_iter()
                .map(|layer| {
                    (
                        layer.key().to_string(),
                        config.layer(layer.key()).to_string(),
                    )
                })
                .collect(),
        }
    }

    /// Resolve a typed conventional layer.
    pub(crate) fn get(&self, layer: Layer) -> &str {
        self.named(layer.key())
    }

    /// Transitional adapter for recipe code still expressed with the public
    /// layer key strings. Keeping it here makes the configuration decision a
    /// secret of `Layers` while those call sites move to [`Layer`].
    pub(crate) fn named<'a>(&'a self, default: &'a str) -> &'a str {
        self.packages
            .get(default)
            .map(String::as_str)
            .unwrap_or(default)
    }

    /// Compatibility spelling while renderer call sites move from `Config`
    /// to this resolved value.
    pub(crate) fn layer<'a>(&'a self, default: &'a str) -> &'a str {
        self.named(default)
    }
}

/// One immutable snapshot of the project facts every recipe needs.
#[derive(Clone, Debug)]
pub(crate) struct Project {
    root: PathBuf,
    base: String,
    flavor: Flavor,
    java_release: Option<u32>,
    layers: Layers,
    pom: String,
    installed: Vec<String>,
}

impl Project {
    /// Resolve project facts exactly once from a known Maven module root.
    pub(crate) fn load(root: &Path) -> Result<Self> {
        let pom = pom::read(root)?;
        let config = Config::load(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            base: crate::generate::base_package(root)?,
            flavor: pom::flavor(&pom),
            java_release: pom::release_level(&pom),
            layers: Layers::from_config(&config),
            installed: config.capabilities().to_vec(),
            pom,
        })
    }

    /// Discover the containing Maven module and resolve it once.
    pub(crate) fn discover() -> Result<Self> {
        Self::load(&crate::generate::find_project_root()?)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn base(&self) -> &str {
        &self.base
    }

    pub(crate) fn flavor(&self) -> Flavor {
        self.flavor
    }

    pub(crate) fn java_release(&self) -> Option<u32> {
        self.java_release
    }

    pub(crate) fn pom(&self) -> &str {
        &self.pom
    }

    pub(crate) fn layers(&self) -> &Layers {
        &self.layers
    }

    pub(crate) fn capabilities(&self) -> &[String] {
        &self.installed
    }

    /// Resolve a package override, or the configured conventional layer.
    pub(crate) fn package(&self, layer: Layer, package: Option<&str>) -> String {
        crate::generate::subpackage(
            &self.base,
            package.unwrap_or_else(|| self.layers.get(layer)),
        )
    }

    /// Transitional string-key form for recipes not yet moved to [`Layer`].
    pub(crate) fn package_named(&self, default: &str, package: Option<&str>) -> String {
        crate::generate::subpackage(&self.base, package.unwrap_or(self.layers.named(default)))
    }

    pub(crate) fn main(&self, layer: Layer, package: Option<&str>) -> PathBuf {
        crate::generate::main_dir(&self.root, &self.package(layer, package))
    }

    pub(crate) fn test(&self, layer: Layer, package: Option<&str>) -> PathBuf {
        crate::generate::test_dir(&self.root, &self.package(layer, package))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "jails-model-project-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src/main/java/com/example/demo")).unwrap();
        fs::write(
            root.join("pom.xml"),
            "<project><properties><maven.compiler.release>25</maven.compiler.release></properties></project>\n",
        )
        .unwrap();
        fs::write(
            root.join("src/main/java/com/example/demo/App.java"),
            "package com.example.demo;\npublic final class App {}\n",
        )
        .unwrap();
        fs::write(
            root.join("jails.toml"),
            "[layout]\ndomain = \"model\"\n\n[project]\ncapabilities = [\"json\"]\n",
        )
        .unwrap();
        root
    }

    #[test]
    fn resolves_project_facts_once_into_values() {
        let root = fixture();
        let project = Project::load(&root).unwrap();
        assert_eq!(project.root(), root);
        assert_eq!(project.base(), "com.example.demo");
        assert_eq!(project.flavor(), Flavor::PlainMaven);
        assert_eq!(project.java_release(), Some(25));
        assert_eq!(project.layers().get(Layer::Domain), "model");
        assert_eq!(project.capabilities(), &["json"]);
        assert_eq!(
            project.main(Layer::Domain, None),
            root.join("src/main/java/com/example/demo/model")
        );
    }

    #[test]
    fn change_merge_deduplicates_equal_contributions() {
        let path = PathBuf::from("src/main/java/Thing.java");
        let first = Change {
            files: vec![Artifact::rendered(path.clone(), "same\n".to_string())],
            properties: vec!["feature.enabled=true".to_string()],
            ..Change::default()
        };
        let second = Change {
            files: vec![Artifact::rendered(path, "same\n".to_string())],
            properties: vec!["feature.enabled=true".to_string()],
            ..Change::default()
        };
        let merged = first.merge(second).unwrap();
        assert_eq!(merged.files.len(), 1);
        assert_eq!(merged.properties.len(), 1);
    }

    #[test]
    fn change_merge_refuses_two_bodies_for_one_path() {
        let path = PathBuf::from("src/main/java/Thing.java");
        let first = Change {
            files: vec![Artifact::rendered(path.clone(), "one\n".to_string())],
            ..Change::default()
        };
        let second = Change {
            files: vec![Artifact::rendered(path, "two\n".to_string())],
            ..Change::default()
        };
        let error = first.merge(second).unwrap_err();
        assert!(error.contains("different contents"), "{error}");
    }
}
