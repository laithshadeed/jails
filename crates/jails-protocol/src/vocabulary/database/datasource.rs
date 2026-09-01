//! Redacted, consumer-visible identities for already available datasources.

use crate::Result;
use crate::database::SqlDialect;
use crate::identity::{ObjectId, ProjectPath};
use jails_support::codec::{self, Codec, Decoder, Encoder};

/// Host and port only. Credentials, database names and full connection URLs
/// never enter a reportable datasource value.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RedactedEndpoint {
    host: String,
    port: u16,
}

impl RedactedEndpoint {
    pub fn new(host: impl Into<String>, port: u16) -> Result<Self> {
        let host = host.into();
        let invalid = host.is_empty()
            || port == 0
            || host
                .chars()
                .any(|ch| ch.is_control() || ch.is_whitespace() || matches!(ch, '@' | '/' | '?'));
        if invalid {
            return Err(concat!(
                "a redacted datasource endpoint is invalid.\n       ",
                "fix: retain only a nonempty host and nonzero consumer-visible port."
            )
            .into());
        }
        Ok(Self { host, port })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn label(&self) -> String {
        if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

impl Codec for RedactedEndpoint {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(&self.host)?;
        encoder.u32(u32::from(self.port));
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let host = decoder.string()?;
        let port = u16::try_from(decoder.u32()?).map_err(|_| {
            jails_support::Failure::Told(
                concat!(
                    "a datasource port overflows u16.\n       ",
                    "fix: restore the datasource record from a compatible source."
                )
                .to_string(),
            )
        })?;
        Self::new(host, port)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, jails_codec_derive::Codec)]
#[codec(unknown_fix = "upgrade jails or restore the record from a compatible source.")]
pub enum DatasourceSource {
    #[codec(tag = 0)]
    ExplicitEnvironment { variable: String },
    #[codec(tag = 1)]
    DeclaredRunningService {
        declaration: ProjectPath,
        service: String,
    },
    #[codec(tag = 2)]
    SpringTestConfiguration { source: ProjectPath },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedDatasource {
    pub dialect: SqlDialect,
    pub source: DatasourceSource,
    pub redacted_endpoint: RedactedEndpoint,
    pub server_major: u16,
    pub capability_digest: ObjectId,
}

impl ResolvedDatasource {
    pub fn new(
        dialect: SqlDialect,
        source: DatasourceSource,
        redacted_endpoint: RedactedEndpoint,
        server_major: u16,
    ) -> Result<Self> {
        if server_major == 0 {
            return Err(concat!(
                "a resolved datasource has server major zero.\n       ",
                "fix: probe the selected server before publishing its capability."
            )
            .into());
        }
        let capability_digest =
            capability_digest(dialect, &source, &redacted_endpoint, server_major)?;
        Ok(Self {
            dialect,
            source,
            redacted_endpoint,
            server_major,
            capability_digest,
        })
    }
}

impl Codec for ResolvedDatasource {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.dialect.encode(encoder)?;
        self.source.encode(encoder)?;
        self.redacted_endpoint.encode(encoder)?;
        encoder.u32(u32::from(self.server_major));
        self.capability_digest.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let dialect = SqlDialect::decode(decoder)?;
        let source = DatasourceSource::decode(decoder)?;
        let endpoint = RedactedEndpoint::decode(decoder)?;
        let server_major = u16::try_from(decoder.u32()?).map_err(|_| {
            jails_support::Failure::Told(
                concat!(
                    "a datasource server major overflows u16.\n       ",
                    "fix: restore the datasource record from a compatible source."
                )
                .to_string(),
            )
        })?;
        let stored = ObjectId::decode(decoder)?;
        let resolved = Self::new(dialect, source, endpoint, server_major)?;
        if resolved.capability_digest != stored {
            return Err(concat!(
                "a datasource capability digest does not match its fields.\n       ",
                "fix: discard the altered observation and resolve the datasource again."
            )
            .into());
        }
        Ok(resolved)
    }
}

fn capability_digest(
    dialect: SqlDialect,
    source: &DatasourceSource,
    endpoint: &RedactedEndpoint,
    server_major: u16,
) -> Result<ObjectId> {
    let mut encoder = Encoder::new();
    dialect.encode(&mut encoder)?;
    source.encode(&mut encoder)?;
    endpoint.encode(&mut encoder)?;
    encoder.u32(u32::from(server_major));
    Ok(ObjectId::from_bytes(codec::domain_hash(
        "JAILS-DATASOURCE-CAPABILITY-1",
        &encoder.finish()?,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datasource_capability_round_trips_without_credentials() {
        let value = ResolvedDatasource::new(
            SqlDialect::PostgreSql,
            DatasourceSource::ExplicitEnvironment {
                variable: "DEV_DATABASE_URL".to_string(),
            },
            RedactedEndpoint::new("127.0.0.1", 5432).unwrap(),
            17,
        )
        .unwrap();
        let mut encoder = Encoder::new();
        value.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("password"));
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(ResolvedDatasource::decode(&mut decoder).unwrap(), value);
        decoder.finish().unwrap();
    }

    #[test]
    fn endpoint_refuses_a_url_or_userinfo() {
        assert!(RedactedEndpoint::new("user@db.internal", 5432).is_err());
        assert!(RedactedEndpoint::new("postgresql://db.internal", 5432).is_err());
    }
}
