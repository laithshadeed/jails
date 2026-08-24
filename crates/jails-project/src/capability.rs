//! What a capability *is*, once the CLI's parameters have been resolved.
//!
//! `jails add csv --name Order` and `jails add csv --name Invoice` are two
//! capabilities, not one installed twice; `jails add actuator --package ops`
//! is the same capability placed somewhere else; and `jails add ci --name X`
//! is a mistake. plan.md §R1.1 fixes those three classes and
//! [`jails_protocol::entity::CapabilityId::resolve`] enforces them -- this
//! module is the half that needs a project to answer: which package a named
//! instance actually resolves to, and how the resulting identity is written
//! back into `jails.toml`.
//!
//! **The package in a `Named` identity is resolved, never the raw override.**
//! `--package ''` is a real placement (the base package), so an identity that
//! stored the override would put "the caller said nothing" and "the caller
//! said flat" in the same slot -- and two installs into two different packages
//! would share one identity and reconcile each other's files away.
//! `CapabilitySpec.placement` is the slot that keeps the override, which is
//! what lets [`Declaration`] reconstruct the line a human wrote.

use jails_protocol::entity::{CapabilityId, CapabilityInstance, CapabilitySpec};
use jails_protocol::identity::{Name, Package};
use jails_support::Result;

use crate::model::{Layer, Project};
use crate::spec::kind::Capability;

/// Where a multi-instance capability's own classes are placed.
///
/// `None` for every other class, and that is the whole distinction: a
/// singleton's placement is not part of what it is, so nothing has to resolve
/// it before an identity can be formed. Adding a capability without deciding
/// this is a compile error, and `every_named_capability_declares_its_layer`
/// fails when a class and a layer disagree.
pub fn layer(kind: Capability) -> Option<Layer> {
    use Capability::*;
    match kind {
        Csv | Sqlite | Json => Some(Layer::Adapters),
        Http => Some(Layer::Api),
        _ => None,
    }
}

/// Resolve `--name`/`--package` into the identity and spec of one capability.
///
/// The refusals come from [`CapabilityId::resolve`], so a parameter a class
/// has no meaning for is reported at the constructor rather than ignored by
/// the recipe that happens not to read it.
pub fn identity(
    project: &Project,
    kind: Capability,
    name: Option<&str>,
    package: Option<&str>,
) -> Result<(CapabilityId, CapabilitySpec)> {
    let name = name.map(Name::parse).transpose()?;
    let placement = package.map(Package::parse).transpose()?;
    // Only a named capability needs the resolved answer, and only a named
    // capability has a layer to resolve it against.
    let resolved = layer(kind)
        .map(|layer| Package::parse(&project.package(layer, package)))
        .transpose()?;
    let id = CapabilityId::resolve(
        kind,
        name.as_ref(),
        resolved.as_ref().or(placement.as_ref()),
    )?;
    Ok((id, CapabilitySpec { placement }))
}

/// What `jails.toml` says about one capability: the kind, plus the parameters
/// the caller actually passed.
///
/// This is the human file's shape, not the identity's. A declaration with no
/// parameters is one entry in `[project] capabilities`; anything else is a
/// `[[capability]]` table. Round-tripping through the *declaration* rather
/// than the identity is what keeps `add csv` writing `"csv"` instead of a
/// three-line table spelling out a package the reader never mentioned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Declaration {
    pub kind: Capability,
    pub name: Option<String>,
    pub package: Option<String>,
}

impl Declaration {
    /// The bare form: what `jails add <kind>` with no parameters declares.
    pub fn plain(kind: Capability) -> Self {
        Self {
            kind,
            name: None,
            package: None,
        }
    }

    /// Reconstruct the declaration behind a recorded identity.
    ///
    /// A `Named` instance whose name is the class's own default was not named
    /// by anybody, so it is not written back as one; the placement comes from
    /// the spec, which is the only field that distinguishes "no `--package`"
    /// from `--package ''`.
    pub fn of(id: &CapabilityId, spec: &CapabilitySpec) -> Self {
        let name = match &id.instance {
            CapabilityInstance::Named { name, .. }
                if Some(name) != default_name(id.kind).as_ref() =>
            {
                Some(name.to_string())
            }
            _ => None,
        };
        Self {
            kind: id.kind,
            name,
            package: spec.placement.as_ref().map(|p| p.as_str().to_string()),
        }
    }

    /// True when this is the bare form, which is the array's shape.
    pub fn is_plain(&self) -> bool {
        self.name.is_none() && self.package.is_none()
    }

    /// Refuse a parameter this capability's class has no meaning for.
    ///
    /// The rules are [`CapabilityId::resolve`]'s, called for its refusals and
    /// not for its answer: a manifest that accepted `--name` on `db` and a
    /// CLI that refused it would be two tables of the same rule, and the file
    /// is the half nobody would notice was wrong until `sync` ran.
    pub fn validate(&self) -> Result<()> {
        let name = self.name.as_deref().map(Name::parse).transpose()?;
        let package = self.package.as_deref().map(Package::parse).transpose()?;
        CapabilityId::resolve(self.kind, name.as_ref(), package.as_ref())?;
        Ok(())
    }

    /// How this declaration reads back to a person, for an error that has to
    /// say which of two lines it means.
    pub fn display(&self) -> String {
        let mut out = self.kind.label().to_string();
        if let Some(name) = &self.name {
            out.push_str(&format!(" --name {name}"));
        }
        if let Some(package) = &self.package {
            out.push_str(&format!(" --package '{package}'"));
        }
        out
    }

    /// Canonical order for newly written tables: by kind, then by the
    /// parameters, so two projects that declared the same set produce the same
    /// file regardless of what order the commands were run in.
    pub fn sort_key(&self) -> (&'static str, &str, &str) {
        (
            self.kind.label(),
            self.name.as_deref().unwrap_or_default(),
            self.package.as_deref().unwrap_or_default(),
        )
    }
}

/// The name a multi-instance capability carries when nobody named it.
///
/// Derived from `CapabilityId::resolve` rather than restated, so the default
/// this compares against cannot drift from the default that was stored.
fn default_name(kind: Capability) -> Option<Name> {
    match CapabilityId::resolve(kind, None, None) {
        Ok(CapabilityId {
            instance: CapabilityInstance::Named { name, .. },
            ..
        }) => Some(name),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ValueEnum;
    use jails_protocol::entity::{CapabilityClass, capability_class};

    #[test]
    fn every_named_capability_declares_its_layer_and_no_other_one_does() {
        for kind in Capability::value_variants() {
            let named = capability_class(*kind) == CapabilityClass::MultiInstanceNamed;
            assert_eq!(
                named,
                layer(*kind).is_some(),
                "`{}` is {:?} but layer() says {:?}",
                kind.label(),
                capability_class(*kind),
                layer(*kind)
            );
        }
    }

    #[test]
    fn a_capability_nobody_parameterised_round_trips_as_the_bare_form() {
        for kind in Capability::value_variants() {
            let id = CapabilityId::resolve(*kind, None, None).unwrap();
            let spec = CapabilitySpec { placement: None };
            let declaration = Declaration::of(&id, &spec);
            assert_eq!(declaration, Declaration::plain(*kind));
            assert!(declaration.is_plain(), "{}", kind.label());
        }
    }

    #[test]
    fn a_named_instance_keeps_the_name_it_was_given() {
        let id = CapabilityId::resolve(
            Capability::Csv,
            Some(&Name::parse("Order").unwrap()),
            Some(&Package::parse("com.example.adapters").unwrap()),
        )
        .unwrap();
        let spec = CapabilitySpec {
            placement: Some(Package::parse("adapters").unwrap()),
        };
        assert_eq!(
            Declaration::of(&id, &spec),
            Declaration {
                kind: Capability::Csv,
                name: Some("Order".to_string()),
                package: Some("adapters".to_string()),
            }
        );
    }

    /// The distinction the identity cannot hold and the spec can.
    #[test]
    fn flat_placement_is_not_the_same_declaration_as_no_placement() {
        let id = CapabilityId::resolve(Capability::Actuator, None, None).unwrap();
        let flat = Declaration::of(
            &id,
            &CapabilitySpec {
                placement: Some(Package::base()),
            },
        );
        let unset = Declaration::of(&id, &CapabilitySpec { placement: None });
        assert_eq!(flat.package.as_deref(), Some(""));
        assert_eq!(unset.package, None);
        assert!(!flat.is_plain());
        assert!(unset.is_plain());
    }
}
