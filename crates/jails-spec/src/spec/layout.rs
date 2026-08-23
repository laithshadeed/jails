// Where each kind of artifact lives, relative to the project's base package.
//
// A generated project should look like one a person laid out, and nobody
// lays out thirty classes as siblings of `App.java`. The names are the ones
// the Java ecosystem already uses, so the layout reads as conventional rather
// than as jails' invention -- and every one of them is a package a human
// would have created by hand on about the third file.
//
// This is a default, not a policy: `--package` overrides it, and `--package
// ''` puts everything back in the base package for a project small enough not
// to want the ceremony.

pub const DOMAIN: &str = "domain";
/// Ports -- the interfaces the application depends on, kept free of the
/// technology that implements them.
pub const APP: &str = "app";
pub const SERVICE: &str = "service";
pub const WEB: &str = "web";
pub const CLI: &str = "cli";
pub const ADAPTERS: &str = "adapters";
pub const API: &str = "api";
pub const TESTKIT: &str = "testkit";
/// Outbound HTTP: interfaces this application calls, kept apart from
/// `api` (what it serves) so the direction of a dependency is visible
/// from the package name alone.
pub const CLIENTS: &str = "clients";
/// Scheduled work.
pub const JOBS: &str = "jobs";
/// Events published to and consumed from a broker.
pub const MESSAGING: &str = "messaging";

/// The eleven layers, as a value.
///
/// plan.md §R2.1 wants a `Layer` rather than a `&str` wherever a layer is
/// meant: a layout edit that carried a string could name a layer that does not
/// exist, and `jails.toml`'s closed key set exists precisely so that cannot
/// happen. The package names are the constants above, so this enum and the
/// layout defaults cannot drift; `config::LAYERS_IN_ORDER` adds each layer's
/// report heading and is checked against this list by a test there.
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
    /// Declaration order is report order.
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

    /// The default subpackage this layer owns, before any `jails.toml` rename.
    pub fn package(self) -> &'static str {
        match self {
            Self::Domain => DOMAIN,
            Self::App => APP,
            Self::Service => SERVICE,
            Self::Web => WEB,
            Self::Api => API,
            Self::Messaging => MESSAGING,
            Self::Cli => CLI,
            Self::Clients => CLIENTS,
            Self::Jobs => JOBS,
            Self::Adapters => ADAPTERS,
            Self::Testkit => TESTKIT,
        }
    }

    /// The layer a default package name denotes. Not a rename lookup: a
    /// project that renamed `adapters` to `persistence` is still asking about
    /// the adapters *layer*, and the rename lives in `Config`.
    pub fn by_package(package: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|layer| layer.package() == package)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layer_has_a_distinct_default_package() {
        let mut seen = std::collections::BTreeSet::new();
        for layer in Layer::ALL {
            assert!(
                seen.insert(layer.package()),
                "{layer:?} duplicates a package"
            );
            assert_eq!(Layer::by_package(layer.package()), Some(layer));
        }
        assert_eq!(seen.len(), Layer::ALL.len());
    }
}
