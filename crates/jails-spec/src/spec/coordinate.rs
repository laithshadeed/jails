//! A Maven coordinate as the CLI accepts it: `group:artifact` and a scope.

use jails_support::Result;
use jails_support::identity::MavenId;

/// `group:artifact`, both halves validated Maven identifiers.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct MavenCoordinate {
    pub group_id: MavenId,
    pub artifact_id: MavenId,
}

impl MavenCoordinate {
    pub fn parse(group_id: &str, artifact_id: &str) -> Result<Self> {
        Ok(Self {
            group_id: MavenId::parse(group_id)?,
            artifact_id: MavenId::parse(artifact_id)?,
        })
    }
}

impl std::fmt::Display for MavenCoordinate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.group_id, self.artifact_id)
    }
}
