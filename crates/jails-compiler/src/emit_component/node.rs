//! How a component spells itself to its templates.
//!
//! The one place a component's declaration becomes the `{{key}}`s a template
//! may name, and the provenance its files carry. The recipes in the parent
//! module say *which* keys each kind needs; this says what each one means.

use crate::Diagnostic;
use crate::recipe::{Node, SourceSet};
use jails_contracts::Provenance;
use jails_model::{AppModel, Component, Package, StableId};
use std::collections::BTreeSet;

/// The typed values of a component its templates may spell.
///
/// Every one is a projection of the declaration -- the stable label, the
/// route, the reference `on` names -- so `destroy` can find what `generate`
/// wrote and two projects spell the same declaration the same way.
#[derive(Clone, Copy)]
pub(crate) enum Key {
    /// `{{property}}`: the stable label with dashes, which is how a setting
    /// key and a metric tag spell this component.
    Property,
    /// `{{group}}`: the same spelling, where Spring's service-client
    /// properties call it a group.
    Group,
    /// `{{path}}`: the declared route, or this default with `{{property}}`
    /// filled in.
    Path(&'static str),
    /// `{{table}}`: the stable label plus this suffix, so a renamed Java
    /// type leaves the rows of a running deployment where they are.
    Table(&'static str),
    /// `{{word}}`: the lowercase name, which is how a CLI subcommand is typed.
    Word,
    /// `{{program}}`: the lowercase name, as the usage line names the binary.
    Program,
    /// `{{issuer}}`: `urn:<base package>`, the JWT issuer this project claims.
    Issuer,
    /// `{{timestamp_header}}` and `{{signature_header}}`: the two headers a
    /// webhook provider is told to send, named after the component.
    TimestampHeader,
    SignatureHeader,
    /// `{{fetcher}}`: the name of the `fetcher` component `on` points at.
    ///
    /// Refusing anything else is the security property, not a type check:
    /// every URL after the seed came off a page somebody else wrote, and
    /// `fetcher` is the component whose whole contract is refusing to follow
    /// one to a private address.
    Fetcher,
    /// `{{<layer>}}`: another layer's package, for a template that imports
    /// from it. A file's own package is always `{{pkg}}`.
    Layer(&'static str, Package),
}

impl Node for Component {
    type Key = Key;

    fn id(&self) -> &str {
        self.id.as_str()
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn describe(&self) -> String {
        format!("component {} `{}`", self.kind.label(), self.name)
    }

    fn key(&self, model: &AppModel, key: Key) -> Result<(&'static str, String), Diagnostic> {
        let property = self.label.replace('_', "-");
        Ok(match key {
            Key::Property => ("property", property),
            Key::Group => ("group", property),
            Key::Path(default) => (
                "path",
                self.route
                    .as_ref()
                    .map(|route| route.path.clone())
                    .unwrap_or_else(|| default.replace("{{property}}", &property)),
            ),
            Key::Table(suffix) => ("table", format!("{}{suffix}", self.label)),
            Key::Word => ("word", self.name.to_lowercase()),
            Key::Program => ("program", self.name.to_lowercase()),
            Key::Issuer => ("issuer", format!("urn:{}", model.project.base_package)),
            Key::TimestampHeader => ("timestamp_header", format!("X-{}-Timestamp", self.name)),
            Key::SignatureHeader => ("signature_header", format!("X-{}-Signature", self.name)),
            Key::Fetcher => (
                "fetcher",
                super::http_workflow::fetcher(model, self)?.name.clone(),
            ),
            Key::Layer(key, package) => (key, model.project.package_for(package)),
        })
    }

    fn file_keys(&self, _: &str, template_class: &str) -> Vec<(&'static str, String)> {
        vec![
            ("class", template_class.to_string()),
            ("name", self.name.clone()),
        ]
    }

    /// The artifact id is what the merge is keyed on, so it has to survive a
    /// rename: `art_<component id>_<role>` moves with the declaration where a
    /// path-derived id would look like a delete and an add.
    fn provenance(&self, artifact_id: String, ejectable: bool, _: &'static str) -> Provenance {
        Provenance {
            artifact_id,
            ejection_id: None,
            ejectable,
            semantic_ids: BTreeSet::from([self.id.as_str().to_string()]),
            compiler_pass: "components".to_string(),
        }
    }

    fn header(&self) -> bool {
        true
    }

    /// A component's tests are plain JUnit over the class, not
    /// `@SpringBootTest`s; the one integration test that reaches a container
    /// says so on its row with [`Import::ContainerSupport`].
    fn splices_test_container(&self, _: SourceSet) -> bool {
        false
    }
}
