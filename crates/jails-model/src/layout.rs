//! Where this project keeps each layer.
//!
//! `jails adopt` exists so a project jails did not write keeps its own
//! directory names, and records the renames in `jails.toml`. Anything
//! reporting or writing per layer goes through the project's renames, and
//! the compiler reads them through this.
//!
//! It is a captured fact, not a declaration. The reader owns `jails.toml`, so
//! the layout arrives on the workspace snapshot like every other external
//! fact and the compiler stays pure: equal snapshot in, equal packages out.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The repeated `jails.toml` table that records one reviewed exception to the
/// generated architecture suite.
///
/// The policy lives in the project's own manifest rather than under `.jails`,
/// because it is about the reader's code and is read by the reader's tests:
/// a project whose state directory is gone must still run the same suite and
/// reach the same verdict.
pub const ARCHITECTURE_ALLOW_TABLE: &str = "architecture.allow";

/// Its closed key set.
///
/// **Two crates read this file and they must know the same words.**
/// `jails_project::config` refuses an unknown key when the *tool* reads
/// `jails.toml`; the generated `ArchitectureTest` refuses one when the
/// *project's tests* read it. A key one accepts and the other does not is a
/// policy jails approves and the build it generated rejects, so both spell the
/// list from here -- the compiler's side through a test that reads the
/// template.
pub const ARCHITECTURE_ALLOW_KEYS: [&str; 5] = ["from", "to", "packages", "reason", "expires"];

/// The eleven layers JDL v1 §9.7 closes, as a value.
///
/// A closed set, because a `jails.toml` saying `adapter = "persistence"` that
/// silently kept writing to `adapters` would be worse than no file at all.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum Layer {
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
    /// Declaration order is JDL v1 §9.7's order.
    pub const ALL: [Layer; 11] = [
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

    /// The package segment this layer owns before any `jails.toml` rename.
    pub const fn package(self) -> &'static str {
        match self {
            Self::Domain => "domain",
            Self::App => "app",
            Self::Service => "service",
            Self::Web => "web",
            Self::Api => "api",
            Self::Messaging => "messaging",
            Self::Cli => "cli",
            Self::Clients => "clients",
            Self::Jobs => "jobs",
            Self::Adapters => "adapters",
            Self::Testkit => "testkit",
        }
    }

    /// The layer a default package segment denotes, `None` for anything
    /// JDL v1 §9.7 does not close. Not a rename lookup: a project that renamed `adapters`
    /// to `persistence` is still asking about the adapters *layer*.
    pub fn by_package(segment: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|layer| layer.package() == segment)
    }
}

/// The head segment of a package the compiler emits into.
///
/// The distinction is which renames, and it is in the type rather than in a
/// lookup miss. Looking every head up in the rename map and letting one with no
/// entry pass through makes `repository` and `application` reading their own
/// names back indistinguishable from a layer whose rename happens to be absent.
/// Saying it in the type is what makes the §9.7 divergence a list somebody
/// can read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Head {
    /// A JDL v1 §9.7 layer. The project's `jails.toml` renames it.
    Layer(Layer),
    /// A package the compiler owns and §9.7 does not close, so nothing renames
    /// it. Every one of these is a recorded divergence from §9.7, and
    /// reconciling them moves files in every project generated so far, which
    /// is why they are named here rather than fixed silently.
    Facet(&'static str),
}

/// Every package the compiler emits Java into, as one table.
///
/// JDL v1 §20.2's registry: an emitter asks for a `Package` and never
/// concatenates a package name itself. Closed on purpose -- a string literal
/// spelling `applications` for `application` compiles, writes a package no
/// other emitter imports, and fails at `javac`. It is also the only place the
/// §9.7 divergence can be *seen*: `Head::Facet` marks each row that is not a
/// layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Package {
    /// The base package itself.
    Base,
    Domain,
    DomainEvents,
    Service,
    Web,
    Api,
    Messaging,
    Cli,
    Clients,
    Jobs,
    Testkit,
    Adapters,
    AdaptersJdbc,
    AdaptersMemory,
    AdaptersHttp,
    Repository,
    Application,
    ApplicationCommands,
    ApplicationQueries,
    ApplicationTransitions,
    PortsHttp,
    PortsEvents,
    PortsSearch,
}

impl Package {
    /// Declaration order, so `model explain` and this table cannot drift.
    pub const ALL: [Package; 23] = [
        Self::Base,
        Self::Domain,
        Self::DomainEvents,
        Self::Service,
        Self::Web,
        Self::Api,
        Self::Messaging,
        Self::Cli,
        Self::Clients,
        Self::Jobs,
        Self::Testkit,
        Self::Adapters,
        Self::AdaptersJdbc,
        Self::AdaptersMemory,
        Self::AdaptersHttp,
        Self::Repository,
        Self::Application,
        Self::ApplicationCommands,
        Self::ApplicationQueries,
        Self::ApplicationTransitions,
        Self::PortsHttp,
        Self::PortsEvents,
        Self::PortsSearch,
    ];

    /// This package's head and the segments below it, empty for neither.
    ///
    /// **One match, one row.** Two exhaustive matches over this enum -- a
    /// `head()` and a `tail()` -- would be two answers the compiler forces
    /// somebody to edit and then does not check against each other.
    pub const fn placement(self) -> (Option<Head>, &'static str) {
        match self {
            Self::Base => (None, ""),
            Self::Domain => (Some(Head::Layer(Layer::Domain)), ""),
            Self::DomainEvents => (Some(Head::Layer(Layer::Domain)), "events"),
            Self::Service => (Some(Head::Layer(Layer::Service)), ""),
            Self::Web => (Some(Head::Layer(Layer::Web)), ""),
            Self::Api => (Some(Head::Layer(Layer::Api)), ""),
            Self::Messaging => (Some(Head::Layer(Layer::Messaging)), ""),
            Self::Cli => (Some(Head::Layer(Layer::Cli)), ""),
            Self::Clients => (Some(Head::Layer(Layer::Clients)), ""),
            Self::Jobs => (Some(Head::Layer(Layer::Jobs)), ""),
            Self::Testkit => (Some(Head::Layer(Layer::Testkit)), ""),
            Self::Adapters => (Some(Head::Layer(Layer::Adapters)), ""),
            Self::AdaptersJdbc => (Some(Head::Layer(Layer::Adapters)), "jdbc"),
            Self::AdaptersMemory => (Some(Head::Layer(Layer::Adapters)), "memory"),
            Self::AdaptersHttp => (Some(Head::Layer(Layer::Adapters)), "http"),
            Self::Repository => (Some(Head::Facet("repository")), ""),
            Self::Application => (Some(Head::Facet("application")), ""),
            Self::ApplicationCommands => (Some(Head::Facet("application")), "commands"),
            Self::ApplicationQueries => (Some(Head::Facet("application")), "queries"),
            Self::ApplicationTransitions => (Some(Head::Facet("application")), "transitions"),
            Self::PortsHttp => (Some(Head::Facet("ports")), "http"),
            Self::PortsEvents => (Some(Head::Facet("ports")), "events"),
            Self::PortsSearch => (Some(Head::Facet("ports")), "search"),
        }
    }
}

/// A project's layer renames, empty when it has none.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Layout {
    renames: BTreeMap<String, String>,
}

impl Layout {
    #[cfg(test)]
    /// Whether this project renamed nothing, so its packages are the defaults.
    pub fn is_default(&self) -> bool {
        self.renames.is_empty()
    }

    /// The `[layout]` table of a `jails.toml`.
    ///
    /// Hand-parsed for the reason the rest of jails hand-parses this file, and
    /// **an unrecognised key is an error**: this is a file people edit, and a
    /// typo that reads as "no rename" produces a tree the reader did not ask
    /// for with nothing to say why.
    pub fn parse(source: &str) -> Result<Self, String> {
        let mut renames = BTreeMap::new();
        let mut in_layout = false;
        for line in source.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(table) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                in_layout = table.trim() == "layout";
                continue;
            }
            if !in_layout {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            if Layer::by_package(key).is_none() {
                return Err(format!(
                    "jails.toml [layout] has no layer `{key}`.\n       \
                     fix: use one of {}.",
                    Layer::ALL
                        .iter()
                        .map(|layer| layer.package())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            renames.insert(key.to_string(), value.to_string());
        }
        Ok(Self { renames })
    }

    /// This project's segment for a package's head.
    ///
    /// Only the head, so a nested package -- `adapters.jdbc`, `ports.http` --
    /// renames its head and keeps its tail: a reader who called their adapters
    /// `persistence` means `persistence.jdbc`, not that the JDBC adapter has
    /// moved. A [`Head::Facet`] is the compiler's own and renames to nothing.
    pub fn head(&self, head: Head) -> &str {
        match head {
            Head::Layer(layer) => self
                .renames
                .get(layer.package())
                .map(String::as_str)
                .unwrap_or(layer.package()),
            Head::Facet(facet) => facet,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unknown key is an error, because silently meaning "no rename"
    /// produces a tree nobody asked for.
    #[test]
    fn an_unknown_layer_is_refused_by_name() {
        let error = Layout::parse("[layout]\nadapter = \"persistence\"\n").unwrap_err();
        assert!(error.contains("no layer `adapter`"), "{error}");
        assert!(error.contains("fix:"), "{error}");
    }

    /// A `[layout]` table jails wrote is not the only thing in the file, and a
    /// key outside it belongs to somebody else.
    #[test]
    fn keys_outside_the_layout_table_are_not_renames() {
        let layout =
            Layout::parse("[project]\ncapabilities = [\"db\"]\n[layout]\nweb = \"http\"\n")
                .unwrap();
        assert_eq!(layout.head(Head::Layer(Layer::Web)), "http");
    }

    /// A facet head is the compiler's own, so nothing can rename it -- not
    /// even a `jails.toml` that happens to spell one. `Layout::parse` refuses
    /// the key, so the only way in would be a rename map built by hand.
    #[test]
    fn a_facet_head_is_never_renamed() {
        let layout = Layout::parse("[layout]\nadapters = \"persistence\"\n").unwrap();
        assert_eq!(layout.head(Head::Facet("repository")), "repository");
        assert_eq!(layout.head(Head::Facet("application")), "application");
    }

    /// Two packages rendering the same name would be one package with two
    /// spellings, and an emitter picking either would still compile.
    #[test]
    fn every_package_renders_a_distinct_suffix() {
        let mut seen = std::collections::BTreeSet::new();
        let layout = Layout::default();
        for package in Package::ALL {
            let (head, tail) = package.placement();
            let suffix = match (head, tail) {
                (None, _) => String::new(),
                (Some(head), "") => layout.head(head).to_string(),
                (Some(head), tail) => format!("{}.{tail}", layout.head(head)),
            };
            assert!(
                seen.insert(suffix.clone()),
                "{package:?} duplicates `{suffix}`"
            );
        }
    }
    #[test]
    fn a_rename_applies_to_its_layer_and_to_nothing_else() {
        let layout = Layout::parse(
            "# a comment\n[layout]\nadapters = \"persistence\"\n\n[project]\nadapters = \"ignored\"\n",
        )
        .unwrap();
        assert_eq!(layout.head(Head::Layer(Layer::Adapters)), "persistence");
        assert_eq!(layout.head(Head::Layer(Layer::Web)), "web");
        assert!(!layout.is_default());
    }

    #[test]
    fn a_project_with_no_file_keeps_every_default_name() {
        let layout = Layout::default();
        assert!(layout.is_default());
        for layer in Layer::ALL {
            assert_eq!(layout.head(Head::Layer(layer)), layer.package());
        }
    }
}
