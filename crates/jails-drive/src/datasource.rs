//! Resolve only already available PostgreSQL endpoints.
//!
//! This module never invokes Compose or Spring. Service startup remains the
//! explicit `jails start` lifecycle; live SQL consumers only prove that an
//! endpoint is reachable from their own process namespace.

use jails_project::compose::{self, PostgresConnect};
use jails_project::model::Project;
use jails_protocol::database::{
    DatasourceSource, RedactedEndpoint, ResolvedDatasource, SqlDialect,
};
use jails_protocol::identity::ProjectPath;
use jails_support::Result;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveServices {
    Existing,
    Start,
    None,
}

pub(crate) struct Candidate {
    pub connection: PostgresConnect,
    source: DatasourceSource,
    endpoint: RedactedEndpoint,
}

impl Candidate {
    pub fn finish(&self, server_major: u32) -> Result<ResolvedDatasource> {
        ResolvedDatasource::new(
            SqlDialect::PostgreSql,
            self.source.clone(),
            self.endpoint.clone(),
            u16::try_from(server_major).map_err(|_| {
                jails_support::Failure::Told(
                    concat!(
                        "PostgreSQL server major overflows u16.\n       ",
                        "fix: upgrade jails for this server version."
                    )
                    .to_string(),
                )
            })?,
        )
    }

    pub fn unavailable(&self, _failure: &jails_support::Failure) -> jails_support::Failure {
        format!(
            "service-unavailable: datasource at {} is not reachable from the jails command consumer.\n       fix: make that endpoint reachable here, or run `jails start` explicitly and retry with `--services existing`.",
            self.endpoint.label()
        )
        .into()
    }
}

pub(crate) fn select(
    project: &Project,
    reference: &str,
    services: LiveServices,
) -> Result<Candidate> {
    require_existing(services)?;

    // An explicitly named environment reference is first and authoritative:
    // if it exists but is malformed, silently falling through to Compose
    // would check a different database than the caller selected.
    if let Some(value) = std::env::var_os(reference) {
        let value = value.into_string().map_err(|_| {
            format!(
                "service-unavailable: environment datasource `{reference}` is not UTF-8.\n       fix: set it to a PostgreSQL URL without embedding it in command arguments."
            )
        })?;
        let connection = postgres_url(&value).map_err(|failure| {
            format!(
                "service-unavailable: environment datasource `{reference}` is invalid ({failure}).\n       fix: set `{reference}` to `postgresql://user:password@host:port/database`."
            )
        })?;
        let endpoint = RedactedEndpoint::new(&connection.host, connection.port)?;
        return Ok(Candidate {
            connection,
            source: DatasourceSource::ExplicitEnvironment {
                variable: reference.to_string(),
            },
            endpoint,
        });
    }

    let yaml = compose::read(project.root())?;
    if reference == "postgres"
        && let Some(connection) = compose::postgres_connect(&yaml)
    {
        let endpoint = RedactedEndpoint::new(&connection.host, connection.port)?;
        return Ok(Candidate {
            connection,
            source: DatasourceSource::DeclaredRunningService {
                declaration: ProjectPath::parse("compose.yaml")?,
                service: reference.to_string(),
            },
            endpoint,
        });
    }

    Err(format!(
        "service-unavailable: datasource `{reference}` could not be resolved; attempted environment variable `{reference}` and declared Compose service `{reference}`.\n       fix: name a set PostgreSQL URL environment variable, or declare and explicitly start the service before retrying."
    )
    .into())
}

fn require_existing(services: LiveServices) -> Result<()> {
    match services {
        LiveServices::Start => {
            return Err(concat!(
                "live database consumers do not start services.\n       ",
                "fix: run `jails start` explicitly, then retry with `--services existing`."
            )
            .into());
        }
        LiveServices::None => {
            return Err(concat!(
                "live database access is disabled by `--services none`.\n       ",
                "fix: select an explicit datasource and use `--services existing`."
            )
            .into());
        }
        LiveServices::Existing => {}
    }
    Ok(())
}

fn postgres_url(value: &str) -> Result<PostgresConnect> {
    let remainder = value
        .strip_prefix("postgresql://")
        .or_else(|| value.strip_prefix("postgres://"))
        .ok_or_else(|| {
            jails_support::Failure::Told(
                concat!(
                    "the value is not a PostgreSQL URL.\n       ",
                    "fix: use the `postgresql://` scheme."
                )
                .to_string(),
            )
        })?;
    let (authority, tail) = remainder.split_once('/').ok_or_else(|| {
        jails_support::Failure::Told(
            concat!(
                "the PostgreSQL URL has no database path.\n       ",
                "fix: append `/database` to the endpoint."
            )
            .to_string(),
        )
    })?;
    let database = tail.split(['?', '#']).next().unwrap_or_default();
    if database.is_empty() {
        return Err(concat!(
            "the PostgreSQL URL has no database name.\n       ",
            "fix: append a nonempty `/database` path."
        )
        .into());
    }
    let (credentials, host_port) = authority.rsplit_once('@').ok_or_else(|| {
        jails_support::Failure::Told(
            concat!(
                "the PostgreSQL URL has no username.\n       ",
                "fix: use `postgresql://user:password@host:port/database`."
            )
            .to_string(),
        )
    })?;
    let (user, password) = credentials.split_once(':').unwrap_or((credentials, ""));
    if user.is_empty() {
        return Err(concat!(
            "the PostgreSQL URL has an empty username.\n       ",
            "fix: name the database user before `@`."
        )
        .into());
    }
    let (host, port) = host_and_port(host_port)?;
    Ok(PostgresConnect {
        host: percent_decode(host)?,
        port,
        user: percent_decode(user)?,
        password: percent_decode(password)?,
        database: percent_decode(database)?,
    })
}

fn host_and_port(value: &str) -> Result<(&str, u16)> {
    let (host, port) = if let Some(rest) = value.strip_prefix('[') {
        let (host, suffix) = rest.split_once(']').ok_or_else(|| {
            jails_support::Failure::Told(
                concat!(
                    "the PostgreSQL URL has an unclosed IPv6 host.\n       ",
                    "fix: close the address with `]` before the port."
                )
                .to_string(),
            )
        })?;
        let port = suffix.strip_prefix(':').unwrap_or("5432");
        (host, port)
    } else if let Some((host, port)) = value.rsplit_once(':') {
        (host, port)
    } else {
        (value, "5432")
    };
    if host.is_empty() {
        return Err(concat!(
            "the PostgreSQL URL has an empty host.\n       ",
            "fix: name a consumer-reachable host."
        )
        .into());
    }
    let port = port.parse::<u16>().map_err(|_| {
        jails_support::Failure::Told(
            concat!(
                "the PostgreSQL URL has an invalid port.\n       ",
                "fix: use a decimal port from 1 through 65535."
            )
            .to_string(),
        )
    })?;
    if port == 0 {
        return Err(concat!(
            "the PostgreSQL URL has port zero.\n       ",
            "fix: use a port from 1 through 65535."
        )
        .into());
    }
    Ok((host, port))
}

fn percent_decode(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let pair = bytes.get(index + 1..index + 3).ok_or_else(|| {
                jails_support::Failure::Told(
                    concat!(
                        "the PostgreSQL URL has a truncated percent escape.\n       ",
                        "fix: encode each escaped byte as `%HH`."
                    )
                    .to_string(),
                )
            })?;
            let byte = hex_nibble(pair[0])
                .and_then(|high| hex_nibble(pair[1]).map(|low| (high << 4) | low))
                .ok_or_else(|| {
                    jails_support::Failure::Told(
                        concat!(
                            "the PostgreSQL URL has an invalid percent escape.\n       ",
                            "fix: encode each escaped byte as `%HH`."
                        )
                        .to_string(),
                    )
                })?;
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| {
        concat!(
            "the PostgreSQL URL contains non-UTF-8 text.\n       ",
            "fix: percent-encode UTF-8 connection components."
        )
        .into()
    })
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_url_parses_and_redaction_drops_credentials_and_database() {
        let connection =
            postgres_url("postgresql://alice:s%40fe@[::1]:5544/orders?sslmode=require").unwrap();
        assert_eq!(connection.host, "::1");
        assert_eq!(connection.port, 5544);
        assert_eq!(connection.user, "alice");
        assert_eq!(connection.password, "s@fe");
        assert_eq!(connection.database, "orders");
        let endpoint = RedactedEndpoint::new(&connection.host, connection.port).unwrap();
        assert_eq!(endpoint.label(), "[::1]:5544");
        assert!(!endpoint.label().contains("alice"));
        assert!(!endpoint.label().contains("orders"));
    }

    #[test]
    fn live_consumer_never_accepts_service_startup() {
        let error = require_existing(LiveServices::Start).unwrap_err();
        assert!(error.contains("do not start services"), "{error}");
    }
}
