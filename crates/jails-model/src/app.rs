//! Typed application-level compiler intent.

use crate::ProjectId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectIntent {
    pub id: ProjectId,
    pub name: String,
    pub base_package: String,
    pub java_release: u16,
    pub dialect: String,
    #[serde(default = "default_platform")]
    pub platform: String,
    #[serde(default = "default_build")]
    pub build: String,
    /// What this project calls each layer.
    ///
    /// A declaration, not an observation: `jails adopt` writes it, and
    /// `jails.toml` is jails' own manifest rather than a file the reader
    /// maintains. It reaches the model from that manifest for now, the way
    /// `.jails/model.toml` reaches it -- a compatibility input until JDL
    /// declares the layout itself.
    ///
    /// `#[serde(default)]` because the defaults are the names the compiler
    /// already used, so a project that renamed nothing encodes exactly as it
    /// did before this field existed.
    #[serde(default)]
    pub layout: crate::Layout,
}

fn default_platform() -> String {
    "spring".to_string()
}

fn default_build() -> String {
    "maven".to_string()
}

impl ProjectIntent {
    /// This project's Java package for a dotted layer suffix.
    ///
    /// **The one place the compiler turns a layer into a package name.** It
    /// was 28 `format!("{}.adapters.jdbc", base_package)` sites, none of which
    /// could apply a rename because none of them knew there was one.
    ///
    /// Only the *head* segment renames: a reader who called their adapters
    /// `persistence` means `persistence.jdbc`, not that the JDBC adapter has
    /// moved somewhere else. A head with no rename key -- `repository`,
    /// `ports`, `application` are the compiler's own facet packages and have no
    /// `jails.toml` entry -- passes through unchanged, which is the honest
    /// answer rather than an invented mapping onto a legacy layer.
    ///
    /// An empty suffix is the base package itself.
    pub fn package_for(&self, suffix: &str) -> String {
        if suffix.is_empty() {
            return self.base_package.clone();
        }
        let (head, tail) = match suffix.split_once('.') {
            Some((head, tail)) => (head, Some(tail)),
            None => (suffix, None),
        };
        let head = self.layout.segment(head);
        match tail {
            Some(tail) => format!("{}.{head}.{tail}", self.base_package),
            None => format!("{}.{head}", self.base_package),
        }
    }
}

#[cfg(test)]
mod package_tests {
    use super::*;
    use crate::{Layout, ProjectId};

    fn project(layout: Layout) -> ProjectIntent {
        ProjectIntent {
            id: ProjectId::parse("project_orders").unwrap(),
            name: "Orders".to_string(),
            base_package: "net.acme.legacy".to_string(),
            java_release: 26,
            dialect: "postgresql".to_string(),
            platform: default_platform(),
            build: default_build(),
            layout,
        }
    }

    #[test]
    fn a_project_with_no_renames_gets_the_names_the_compiler_always_used() {
        let project = project(Layout::default());
        assert_eq!(project.package_for("domain"), "net.acme.legacy.domain");
        assert_eq!(
            project.package_for("adapters.jdbc"),
            "net.acme.legacy.adapters.jdbc"
        );
        assert_eq!(project.package_for(""), "net.acme.legacy");
    }

    /// `bugs.md` B59: the rename applies to the head and leaves the tail alone.
    #[test]
    fn a_renamed_layer_renames_its_head_and_keeps_its_tail() {
        let project = project(Layout::parse("[layout]\nadapters = \"persistence\"\n").unwrap());
        assert_eq!(
            project.package_for("adapters.jdbc"),
            "net.acme.legacy.persistence.jdbc"
        );
        assert_eq!(
            project.package_for("adapters"),
            "net.acme.legacy.persistence"
        );
        assert_eq!(project.package_for("domain"), "net.acme.legacy.domain");
    }

    /// The compiler's own facet packages have no `jails.toml` key, so they
    /// pass through rather than being mapped onto a legacy layer by guess.
    #[test]
    fn a_facet_package_with_no_rename_key_is_left_alone() {
        let project = project(Layout::parse("[layout]\nadapters = \"persistence\"\n").unwrap());
        assert_eq!(
            project.package_for("repository"),
            "net.acme.legacy.repository"
        );
        assert_eq!(
            project.package_for("ports.http"),
            "net.acme.legacy.ports.http"
        );
    }
}
