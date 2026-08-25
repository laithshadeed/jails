//! A resource the reader asked for by name -- `add dependency` and `set`.
//!
//! Its own module because it is its own secret: every other entity is
//! something jails knows the *meaning* of. A capability knows what a library
//! is for -- `add db` installs Flyway and Testcontainers and a compose service
//! because it knows what a database is. An intent knows what to render. This
//! knows neither, and the whole type exists to carry an ask jails cannot
//! interpret through the same ownership machinery as one it can.
//!
//! Recording it is the value. Before this existed the alternative was a
//! hand-edited `pom.xml` or `application.properties`, which is precisely the
//! file the format modules exist to edit surgically -- and a hand edit is
//! invisible to `remove`, to `sync`, and to the collision check that stops two
//! owners claiming one key.

use crate::Result;
use jails_support::codec::{Codec, Decoder, Encoder};

/// Which resource was asked for.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum DeclaredId {
    /// One artifact, keyed by coordinate. Version and scope are content, not
    /// identity: `jails add dependency` again with a different scope is an
    /// edit to a known entity, not a second claim on the same `<dependency>`.
    Dependency(crate::coordinate::MavenCoordinate),
    /// One setting in one file. The path is identity because the same key in
    /// `src/main/resources` and in `src/test/resources/config` are two
    /// independent settings -- which is exactly how a test-only override is
    /// expressed, and it must not collide with the value it overrides.
    Property {
        path: crate::identity::ProjectPath,
        key: crate::identity::PropertyKey,
    },
}

impl DeclaredId {
    fn tag(&self) -> u8 {
        match self {
            Self::Dependency(_) => 0,
            Self::Property { .. } => 1,
        }
    }
}
impl Codec for DeclaredId {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Dependency(coordinate) => coordinate.encode(encoder),
            Self::Property { path, key } => {
                path.encode(encoder)?;
                key.encode(encoder)
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.tag()? {
            0 => crate::coordinate::MavenCoordinate::decode(decoder).map(Self::Dependency),
            1 => Ok(Self::Property {
                path: crate::identity::ProjectPath::decode(decoder)?,
                key: crate::identity::PropertyKey::decode(decoder)?,
            }),
            other => Err(format!("unknown declared resource tag {other}")),
        }
    }
}

/// What a declared resource was asked to be.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeclaredSpec {
    Dependency(crate::coordinate::DependencySpec),
    Property(crate::resource::PropertySetting),
}

impl DeclaredSpec {
    fn tag(&self) -> u8 {
        match self {
            Self::Dependency(_) => 0,
            Self::Property(_) => 1,
        }
    }

    /// Whether this content belongs to that identity.
    ///
    /// The one place the discriminant check has to go a level deeper than
    /// `EntitySpec::matches` can: a `Dependency` identity paired with a
    /// `Property` setting type-checks and describes a claim nothing can write.
    /// The coordinate is checked too, for the reason
    /// `ResourceValue::agrees_with` checks its own -- it is recorded twice, so
    /// exactly one place has to confirm the two copies agree.
    pub fn matches(&self, id: &DeclaredId) -> bool {
        match (id, self) {
            (DeclaredId::Dependency(coordinate), Self::Dependency(dependency)) => {
                &dependency.coordinate == coordinate
            }
            (DeclaredId::Property { .. }, Self::Property(_)) => true,
            _ => false,
        }
    }
}
impl Codec for DeclaredSpec {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Dependency(spec) => spec.encode(encoder),
            Self::Property(setting) => setting.encode(encoder),
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.tag()? {
            0 => crate::coordinate::DependencySpec::decode(decoder).map(Self::Dependency),
            1 => crate::resource::PropertySetting::decode(decoder).map(Self::Property),
            other => Err(format!("unknown declared spec tag {other}")),
        }
    }
}
