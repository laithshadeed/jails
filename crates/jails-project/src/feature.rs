//! A build plugin, claimed by what it does rather than by its coordinate:
//! `jacoco-maven-plugin` is not a name Gradle resolves.

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum BuildFeature {
    IntegrationTests,
    Coverage,
    Formatting,
}

impl BuildFeature {
    pub fn purpose(self) -> &'static str {
        match self {
            Self::IntegrationTests => "run `*IT` integration tests",
            Self::Coverage => "enforce line coverage",
            Self::Formatting => "check formatting",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::IntegrationTests => "integration-tests",
            Self::Coverage => "coverage",
            Self::Formatting => "formatting",
        }
    }
}

impl std::fmt::Display for BuildFeature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}
