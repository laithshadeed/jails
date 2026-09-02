//! Typed application-level compiler intent.

use crate::ProjectId;
use crate::layout::Package;
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
    /// maintains. It reaches the model from that manifest, a compatibility
    /// input until JDL declares the layout itself.
    ///
    /// `#[serde(default)]` because the defaults are the compiler's own names,
    /// so a project that renamed nothing encodes without the field.
    ///
    /// `Compiler::compile` copies it off the snapshot onto the model rather
    /// than passing it beside one: every emitter holds an `AppModel` and
    /// nothing else, and a second value threaded through every emitter
    /// signature to answer "what does this project call its adapters" is
    /// parameter sprawl.
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
    /// This project's Java package for one entry in the [`Package`] registry.
    ///
    /// **The one place the compiler turns a package into a name.** An emitter
    /// concatenating a package itself can neither apply a rename nor say
    /// whether a head is a layer; `Package` closes both, and JDL v1 §20.2
    /// forbids the concatenation.
    ///
    /// Only the head renames: a reader who called their adapters `persistence`
    /// means `persistence.jdbc`, not that the JDBC adapter has moved somewhere
    /// else. A `Head::Facet` head is the compiler's own and renames to
    /// nothing.
    pub fn package_for(&self, package: Package) -> String {
        let (Some(head), tail) = package.placement() else {
            return self.base_package.clone();
        };
        let head = self.layout.head(head);
        if tail.is_empty() {
            format!("{}.{head}", self.base_package)
        } else {
            format!("{}.{head}.{tail}", self.base_package)
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
        assert_eq!(
            project.package_for(Package::Domain),
            "net.acme.legacy.domain"
        );
        assert_eq!(
            project.package_for(Package::AdaptersJdbc),
            "net.acme.legacy.adapters.jdbc"
        );
        assert_eq!(project.package_for(Package::Base), "net.acme.legacy");
    }

    /// The rename applies to the head and leaves the tail alone.
    #[test]
    fn a_renamed_layer_renames_its_head_and_keeps_its_tail() {
        let project = project(Layout::parse("[layout]\nadapters = \"persistence\"\n").unwrap());
        assert_eq!(
            project.package_for(Package::AdaptersJdbc),
            "net.acme.legacy.persistence.jdbc"
        );
        assert_eq!(
            project.package_for(Package::Adapters),
            "net.acme.legacy.persistence"
        );
        assert_eq!(
            project.package_for(Package::Domain),
            "net.acme.legacy.domain"
        );
    }

    /// The compiler's own facet packages have no `jails.toml` key, so they
    /// pass through rather than being mapped onto a layer by guess.
    #[test]
    fn a_facet_package_with_no_rename_key_is_left_alone() {
        let project = project(Layout::parse("[layout]\nadapters = \"persistence\"\n").unwrap());
        assert_eq!(
            project.package_for(Package::Repository),
            "net.acme.legacy.repository"
        );
        assert_eq!(
            project.package_for(Package::PortsHttp),
            "net.acme.legacy.ports.http"
        );
    }
}
