//! Everything named by a Maven coordinate.
//!
//! Split out of `resource.rs` by secret, not by size: a coordinate, a version,
//! a scope, a dependency and a plugin block are one vocabulary — what a POM
//! declares — while what surrounds them there is a different one: who may
//! claim a thing and what happens when the last claimant leaves. The two
//! changed for different reasons every time either changed.
//!
//! ## Why the plugin block stays opaque text
//!
//! plan.md §R1.1: *"children remain opaque because Maven plugin configuration
//! is intentionally open-ended. This is safer and simpler than a partial
//! plugin AST."* What is validated is the *envelope* — one element, no
//! surrounding document, canonical line endings — because that is what a
//! splice depends on, and a partial parse that silently dropped an
//! unrecognised child would corrupt a file the reader owns.

use crate::Result;
use crate::identity::{ManagedVersion, MavenId};
use jails_support::codec::{Decoder, Encoder};

/// The `(groupId, artifactId)` pair a managed dependency or plugin is
/// identified by. The version is deliberately not part of it: two rows for one
/// coordinate at two versions is the drift jails exists to prevent.
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

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.group_id.encode(encoder)?;
        self.artifact_id.encode(encoder)
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            group_id: MavenId::decode(decoder)?,
            artifact_id: MavenId::decode(decoder)?,
        })
    }
}

impl std::fmt::Display for MavenCoordinate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.group_id, self.artifact_id)
    }
}

/// Where a dependency's version comes from.
///
/// `Managed` is not "unknown" — it is the assertion that the parent POM or a
/// BOM supplies it, which is *correct* under `spring-boot-starter-parent` and
/// fatal without one (CLAUDE.md: Maven refuses to read the POM at all). The
/// two cases are therefore distinct values, never an `Option<String>`.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum MavenVersion {
    Managed,
    Pinned(ManagedVersion),
}

impl MavenVersion {
    fn tag(&self) -> u8 {
        match self {
            Self::Managed => 0,
            Self::Pinned(_) => 1,
        }
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Managed => Ok(()),
            Self::Pinned(version) => version.encode(encoder),
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.tag()? {
            0 => Ok(Self::Managed),
            1 => Ok(Self::Pinned(ManagedVersion::decode(decoder)?)),
            other => Err(format!("unknown Maven version tag {other}")),
        }
    }
}

/// The three scopes jails emits. Absent normalises to `Compile` at the
/// boundary, so the recorded value is always explicit.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum MavenScope {
    #[default]
    Compile,
    Runtime,
    Test,
}

impl MavenScope {
    /// An absent `<scope>` is Maven's `compile`, so parsing accepts the empty
    /// spelling rather than making every caller normalise it.
    pub fn parse(text: &str) -> Result<Self> {
        match text.trim() {
            "" | "compile" => Ok(Self::Compile),
            "runtime" => Ok(Self::Runtime),
            "test" => Ok(Self::Test),
            other => Err(format!(
                "unsupported Maven scope `{other}`; jails emits compile, runtime and test"
            )),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Compile => "compile",
            Self::Runtime => "runtime",
            Self::Test => "test",
        }
    }

    fn tag(self) -> u8 {
        match self {
            Self::Compile => 0,
            Self::Runtime => 1,
            Self::Test => 2,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            0 => Ok(Self::Compile),
            1 => Ok(Self::Runtime),
            2 => Ok(Self::Test),
            other => Err(format!("unknown Maven scope tag {other}")),
        }
    }
}

/// One managed `<dependency>`. Carries no root path and no rendered POM bytes:
/// the format owner renders it, so the same record produces the same line in
/// any project.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct DependencySpec {
    pub coordinate: MavenCoordinate,
    pub version: MavenVersion,
    pub scope: MavenScope,
    pub optional: bool,
}

impl DependencySpec {
    pub fn managed(coordinate: MavenCoordinate) -> Self {
        Self {
            coordinate,
            version: MavenVersion::Managed,
            scope: MavenScope::Compile,
            optional: false,
        }
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.coordinate.encode(encoder)?;
        self.version.encode(encoder)?;
        encoder.tag(self.scope.tag());
        encoder.bool(self.optional);
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            coordinate: MavenCoordinate::decode(decoder)?,
            version: MavenVersion::decode(decoder)?,
            scope: MavenScope::from_tag(decoder.tag()?)?,
            optional: decoder.bool()?,
        })
    }
}

/// Maven's own default group for a plugin with no `<groupId>`.
pub const DEFAULT_PLUGIN_GROUP: &str = "org.apache.maven.plugins";

/// Exactly one `<plugin>` element, LF-terminated, with no surrounding POM.
///
/// The children are not parsed. What is checked is that this is one complete
/// element and nothing else, because that is precisely what a splice into a
/// `<plugins>` list can rely on — and because a plugin block that carried a
/// second element would install something nobody recorded.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct CanonicalPluginXml(String);

impl CanonicalPluginXml {
    pub fn parse(text: &str) -> Result<Self> {
        // Checked before trimming: a CR in the trailing whitespace would be
        // silently dropped, which gives one canonical value two source
        // spellings — the thing this type exists to prevent.
        if text.contains('\r') {
            return Err("plugin block contains CR; canonical XML is LF-only".to_string());
        }
        let body = text.trim();
        if !body.starts_with("<plugin>") || !body.ends_with("</plugin>") {
            return Err(
                "plugin block must be exactly one <plugin> element with no surrounding POM bytes"
                    .to_string(),
            );
        }
        if body.matches("<plugin>").count() != 1 || body.matches("</plugin>").count() != 1 {
            return Err("plugin block contains more than one <plugin> element".to_string());
        }
        Ok(Self(body.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The coordinate the block itself declares.
    ///
    /// Only the prefix before the first `<configuration>`, `<dependencies>` or
    /// `<executions>` is scanned: a plugin's *configuration* routinely names
    /// other artifacts' group and artifact ids, and reading the first match in
    /// the whole block would attribute one of those to the plugin.
    pub fn declared_coordinate(&self) -> Result<MavenCoordinate> {
        let head = self
            .0
            .find("<configuration>")
            .into_iter()
            .chain(self.0.find("<dependencies>"))
            .chain(self.0.find("<executions>"))
            .min()
            .unwrap_or(self.0.len());
        let head = &self.0[..head];
        let group = element(head, "groupId").unwrap_or(DEFAULT_PLUGIN_GROUP);
        let artifact = element(head, "artifactId")
            .ok_or_else(|| "plugin block declares no <artifactId>".to_string())?;
        MavenCoordinate::parse(group, artifact)
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(&self.0)
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.string()?)
    }
}

fn element<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = source.find(&open)? + open.len();
    let end = source[start..].find(&close)? + start;
    Some(source[start..end].trim())
}

/// One managed `<plugin>`, keyed by coordinate and carrying its own block.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct PluginSpec {
    pub coordinate: MavenCoordinate,
    pub block: CanonicalPluginXml,
}

impl PluginSpec {
    /// Refuses a block whose own coordinate disagrees with the key. Two
    /// spellings of one plugin is how a `remove` deletes the wrong element.
    pub fn new(coordinate: MavenCoordinate, block: CanonicalPluginXml) -> Result<Self> {
        let declared = block.declared_coordinate()?;
        if declared != coordinate {
            return Err(format!(
                "plugin block declares {declared} but the resource key says {coordinate}"
            ));
        }
        Ok(Self { coordinate, block })
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.coordinate.encode(encoder)?;
        self.block.encode(encoder)
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let coordinate = MavenCoordinate::decode(decoder)?;
        let block = CanonicalPluginXml::decode(decoder)?;
        Self::new(coordinate, block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coordinate(group: &str, artifact: &str) -> MavenCoordinate {
        MavenCoordinate::parse(group, artifact).unwrap()
    }

    #[test]
    fn a_plugin_block_declaring_another_coordinate_is_refused() {
        let block = CanonicalPluginXml::parse(
            "<plugin>\n  <groupId>com.diffplug.spotless</groupId>\n  \
             <artifactId>spotless-maven-plugin</artifactId>\n</plugin>",
        )
        .unwrap();
        let error = PluginSpec::new(
            coordinate("org.apache.maven.plugins", "maven-failsafe-plugin"),
            block,
        )
        .unwrap_err();
        assert!(error.contains("declares"), "{error}");
    }

    #[test]
    fn a_coordinate_is_read_before_the_configuration_not_inside_it() {
        let block = CanonicalPluginXml::parse(
            "<plugin>\n  <artifactId>maven-failsafe-plugin</artifactId>\n  \
             <configuration>\n    <groupId>org.other</groupId>\n    \
             <artifactId>not-this-one</artifactId>\n  </configuration>\n</plugin>",
        )
        .unwrap();
        assert_eq!(
            block.declared_coordinate().unwrap(),
            coordinate(DEFAULT_PLUGIN_GROUP, "maven-failsafe-plugin"),
            "an omitted <groupId> is Maven's own default, not an error"
        );
    }

    #[test]
    fn a_plugin_block_must_be_one_element_and_nothing_else() {
        for bad in [
            "<plugins><plugin><artifactId>a</artifactId></plugin></plugins>",
            "<plugin><artifactId>a</artifactId></plugin>\n<plugin><artifactId>b</artifactId></plugin>",
            "<plugin><artifactId>a</artifactId></plugin>\r\n",
        ] {
            assert!(CanonicalPluginXml::parse(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn an_absent_scope_is_compile_and_an_unknown_one_is_an_error() {
        assert_eq!(MavenScope::parse("").unwrap(), MavenScope::Compile);
        assert_eq!(MavenScope::parse("test").unwrap(), MavenScope::Test);
        assert!(MavenScope::parse("provided").is_err());
    }
}
