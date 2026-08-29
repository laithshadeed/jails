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

/// Which resource was asked for.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, jails_codec_derive::Codec)]
pub enum DeclaredId {
    /// One artifact, keyed by coordinate. Version and scope are content, not
    /// identity: `jails add dependency` again with a different scope is an
    /// edit to a known entity, not a second claim on the same `<dependency>`.
    #[codec(tag = 0)]
    Dependency(crate::coordinate::MavenCoordinate),
    /// One setting in one file. The path is identity because the same key in
    /// `src/main/resources` and in `src/test/resources/config` are two
    /// independent settings -- which is exactly how a test-only override is
    /// expressed, and it must not collide with the value it overrides.
    #[codec(tag = 1)]
    Property {
        path: crate::identity::ProjectPath,
        key: crate::identity::PropertyKey,
    },
}

/// What a declared resource was asked to be.
#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub enum DeclaredSpec {
    #[codec(tag = 0)]
    Dependency(crate::coordinate::DependencySpec),
    #[codec(tag = 1)]
    Property(crate::resource::PropertySetting),
}

impl DeclaredSpec {
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
