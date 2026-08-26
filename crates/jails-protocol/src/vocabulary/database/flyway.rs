//! Typed, credential-free Flyway history and frozen migration inputs.

use crate::Result;
use crate::database::ResolvedDatasource;
use crate::identity::{ObjectId, ProjectPath};
use crate::lifecycle::MigrationVersion;
use jails_support::codec::{self, Codec, Decoder, Encoder};

/// One row observed from Flyway's append-only schema history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlywayAppliedMigrationV1 {
    pub installed_rank: u32,
    pub version: Option<MigrationVersion>,
    pub description: String,
    pub script: String,
    pub checksum: Option<i32>,
    pub success: bool,
}

impl FlywayAppliedMigrationV1 {
    pub fn new(
        installed_rank: u32,
        version: Option<MigrationVersion>,
        description: impl Into<String>,
        script: impl Into<String>,
        checksum: Option<i32>,
        success: bool,
    ) -> Result<Self> {
        let description = description.into();
        let script = script.into();
        if installed_rank == 0
            || description.is_empty()
            || script.is_empty()
            || description.chars().any(char::is_control)
            || script.chars().any(char::is_control)
        {
            return Err(concat!(
                "a Flyway history row is invalid.\n       ",
                "fix: retain its nonzero rank and nonempty single-line description and script."
            )
            .into());
        }
        Ok(Self {
            installed_rank,
            version,
            description,
            script,
            checksum,
            success,
        })
    }
}

impl Codec for FlywayAppliedMigrationV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.u32(self.installed_rank);
        encoder.option(self.version.as_ref(), |encoder, version| {
            version.encode(encoder)
        })?;
        encoder.string(&self.description)?;
        encoder.string(&self.script)?;
        encoder.option(self.checksum.as_ref(), |encoder, checksum| {
            encoder.u32(*checksum as u32);
            Ok(())
        })?;
        encoder.bool(self.success);
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::new(
            decoder.u32()?,
            decoder.option(MigrationVersion::decode)?,
            decoder.string()?,
            decoder.string()?,
            decoder.option(|decoder| Ok(decoder.u32()? as i32))?,
            decoder.bool()?,
        )
    }
}

/// One consumer-reachable datasource and the exact Flyway history it exposed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlywayHistoryV1 {
    pub datasource: ResolvedDatasource,
    pub applied: Vec<FlywayAppliedMigrationV1>,
    pub history_digest: ObjectId,
}

impl FlywayHistoryV1 {
    pub fn new(
        datasource: ResolvedDatasource,
        applied: Vec<FlywayAppliedMigrationV1>,
    ) -> Result<Self> {
        let mut prior = 0;
        for row in &applied {
            if row.installed_rank <= prior {
                return Err(concat!(
                    "Flyway history is not in strict installed-rank order.\n       ",
                    "fix: order the observed rows before publishing live evidence."
                )
                .into());
            }
            prior = row.installed_rank;
        }
        let history_digest = history_digest(&datasource, &applied)?;
        Ok(Self {
            datasource,
            applied,
            history_digest,
        })
    }
}

impl Codec for FlywayHistoryV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.datasource.encode(encoder)?;
        encode_rows(encoder, &self.applied)?;
        self.history_digest.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let datasource = ResolvedDatasource::decode(decoder)?;
        let applied = decode_rows(decoder)?;
        let stored = ObjectId::decode(decoder)?;
        let history = Self::new(datasource, applied)?;
        if history.history_digest != stored {
            return Err(concat!(
                "Flyway history digest does not match its rows.\n       ",
                "fix: discard the altered evidence and observe the datasource again."
            )
            .into());
        }
        Ok(history)
    }
}

/// One migration file frozen into an explicit post-commit application.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MigrationInputV1 {
    pub version: Option<MigrationVersion>,
    pub path: ProjectPath,
    pub content_digest: ObjectId,
}

impl MigrationInputV1 {
    pub fn new(path: ProjectPath, content_digest: ObjectId) -> Result<Self> {
        if !path
            .as_str()
            .starts_with("src/main/resources/db/migration/")
            || !path.as_str().ends_with(".sql")
        {
            return Err(format!(
                "migration effect input `{path}` is outside Flyway history.\n       fix: select a Flyway SQL file beneath `src/main/resources/db/migration`."
            )
            .into());
        }
        let file = path.as_str().rsplit('/').next().unwrap_or_default();
        let version = if let Some((raw, _)) = file
            .strip_prefix('V')
            .and_then(|rest| rest.split_once("__"))
        {
            Some(MigrationVersion::new(raw.parse().map_err(|_| {
                format!(
                    "migration effect input `{path}` has a nonnumeric version.\n       fix: use `V<number>__description.sql` or `R__description.sql`."
                )
            })?)?)
        } else if file.starts_with("R__") {
            None
        } else {
            return Err(format!(
                "migration effect input `{path}` is not a Flyway migration.\n       fix: use `V<number>__description.sql` or `R__description.sql`."
            )
            .into());
        };
        Ok(Self {
            version,
            path,
            content_digest,
        })
    }
}

impl Codec for MigrationInputV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.path.encode(encoder)?;
        self.content_digest.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::new(ProjectPath::decode(decoder)?, ObjectId::decode(decoder)?)
    }
}

fn history_digest(
    datasource: &ResolvedDatasource,
    applied: &[FlywayAppliedMigrationV1],
) -> Result<ObjectId> {
    let mut encoder = Encoder::new();
    datasource.encode(&mut encoder)?;
    encode_rows(&mut encoder, applied)?;
    Ok(ObjectId::from_bytes(codec::domain_hash(
        "JAILS-FLYWAY-HISTORY-1",
        &encoder.finish()?,
    )))
}

fn encode_rows(encoder: &mut Encoder, rows: &[FlywayAppliedMigrationV1]) -> Result<()> {
    encoder.count(rows.len())?;
    for row in rows {
        row.encode(encoder)?;
    }
    Ok(())
}

fn decode_rows(decoder: &mut Decoder<'_>) -> Result<Vec<FlywayAppliedMigrationV1>> {
    let count = decoder.count()?;
    let mut rows = Vec::with_capacity(count as usize);
    for _ in 0..count {
        rows.push(FlywayAppliedMigrationV1::decode(decoder)?);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{DatasourceSource, RedactedEndpoint, SqlDialect};

    fn datasource() -> ResolvedDatasource {
        ResolvedDatasource::new(
            SqlDialect::PostgreSql,
            DatasourceSource::ExplicitEnvironment {
                variable: "DEV_DATABASE_URL".to_string(),
            },
            RedactedEndpoint::new("127.0.0.1", 5432).unwrap(),
            17,
        )
        .unwrap()
    }

    #[test]
    fn flyway_history_round_trips_and_is_bound_to_the_datasource() {
        let history = FlywayHistoryV1::new(
            datasource(),
            vec![
                FlywayAppliedMigrationV1::new(
                    1,
                    Some(MigrationVersion::new(1).unwrap()),
                    "create tasks",
                    "V001__create_tasks.sql",
                    Some(-42),
                    true,
                )
                .unwrap(),
            ],
        )
        .unwrap();
        let mut encoder = Encoder::new();
        history.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(FlywayHistoryV1::decode(&mut decoder).unwrap(), history);
        decoder.finish().unwrap();
    }

    #[test]
    fn migration_inputs_are_bounded_to_the_flyway_directory() {
        assert!(
            MigrationInputV1::new(
                ProjectPath::parse("notes/V001__no.sql").unwrap(),
                ObjectId::from_bytes([1; 32]),
            )
            .is_err()
        );
    }
}
