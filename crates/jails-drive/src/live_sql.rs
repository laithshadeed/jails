//! Explicit, read-only PostgreSQL evidence for managed SQL queries.

use jails_project::compose::{self, PostgresConnect};
use jails_project::model::Project;
use jails_project::query_workspace::CheckedQuery;
use jails_protocol::database::{
    CatalogSnapshot, QualifiedSqlName, SchemaObject, SchemaObjectId, SchemaObjectKind,
    SchemaProvenance, SchemaSnapshot, SqlDialect, SqlTypeName,
};
use jails_protocol::identity::{ObjectId, SqlName};
use jails_support::Result;
use jails_support::codec::domain_hash;
use jails_support::process::{CommandSpec, Diagnostics, OutputMode};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveServices {
    Existing,
    Start,
    None,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveDescription {
    pub server_major: u32,
    pub columns: Vec<(String, String)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RowEvidence {
    pub schema: String,
    pub table: String,
    pub rows: u64,
}

pub fn check(
    project: &Project,
    datasource: &str,
    services: LiveServices,
    queries: &[CheckedQuery],
    debug: bool,
) -> Result<Vec<LiveDescription>> {
    let database = connect(project, datasource, services, debug)?;
    queries
        .iter()
        .map(|query| describe(&database.conn, query, database.server_major, debug))
        .collect()
}

struct LiveDatabase {
    conn: PostgresConnect,
    server_major: u32,
}

fn connect(
    project: &Project,
    datasource: &str,
    services: LiveServices,
    debug: bool,
) -> Result<LiveDatabase> {
    if datasource != "postgres" {
        return Err(format!(
            "unknown live datasource `{datasource}`.\n       fix: select the declared Compose datasource with `--datasource postgres`."
        )
        .into());
    }
    if services == LiveServices::None {
        return Err(
            "live SQL checking is disabled by `--services none`.\n       fix: use `--services existing`, or explicitly allow startup with `--services start`."
                .into(),
        );
    }
    if !crate::process::on_path("psql") {
        return Err(
            "psql not on PATH.\n       fix: install the PostgreSQL client and try again.".into(),
        );
    }
    let yaml = compose::read(project.root())?;
    let conn = compose::postgres_connect(&yaml).ok_or_else(|| {
        "datasource `postgres` is not declared in compose.yaml.\n       fix: run `jails add db`, or select a declared datasource."
            .to_string()
    })?;
    if services == LiveServices::Start {
        if !compose::up(project.root(), &["postgres"], debug) {
            return Err(
                "could not start datasource `postgres`.\n       fix: inspect the Compose error above, then retry."
                    .into(),
            );
        }
        wait_until_ready(&conn, debug)?;
    }

    let server_major = server_major(&conn, debug)?;
    Ok(LiveDatabase { conn, server_major })
}

/// Observe a bounded PostgreSQL catalog into stable identities. Observation is
/// one read-only transaction, excludes system and extension-owned objects by
/// explicit policy, and records no host, credential, OID, or database name.
pub fn observe(
    project: &Project,
    datasource: &str,
    services: LiveServices,
    schema: &str,
    debug: bool,
) -> Result<SchemaSnapshot> {
    let schema_name = SqlName::parse(schema)?;
    let database = connect(project, datasource, services, debug)?;
    let sql = OBSERVE_SQL.replace("__JAILS_SCHEMA__", schema_name.as_str());
    let output = psql(&database.conn, &sql, debug)?;
    let objects = parse_observed(&output)?;
    let fingerprint = ObjectId::from_bytes(domain_hash(
        "JAILS-POSTGRES-OBSERVATION-1",
        output.as_bytes(),
    ));
    Ok(SchemaSnapshot {
        catalog: CatalogSnapshot::new(SqlDialect::PostgreSql, objects, Vec::new())?,
        provenance: SchemaProvenance::Live {
            server_major: u16::try_from(database.server_major).map_err(|_| {
                "PostgreSQL server major overflows u16.\n       fix: upgrade jails for this server version."
            })?,
            database_fingerprint: fingerprint,
        },
        ignored_schemas: ["information_schema", "pg_catalog", "pg_toast"]
            .into_iter()
            .map(SqlName::parse)
            .collect::<Result<BTreeSet<_>>>()?,
        ignores_extension_owned_objects: true,
    })
}

/// Count rows only for explicitly named, canonically validated tables. The
/// statements run in one read-only transaction and return no row contents.
pub fn row_counts(
    project: &Project,
    datasource: &str,
    services: LiveServices,
    tables: &BTreeSet<(SqlName, SqlName)>,
    debug: bool,
) -> Result<Vec<RowEvidence>> {
    if tables.is_empty() {
        return Ok(Vec::new());
    }
    let database = connect(project, datasource, services, debug)?;
    let mut sql = String::from("BEGIN READ ONLY;\n");
    for (schema, table) in tables {
        sql.push_str(&format!(
            "SELECT '{}' || E'\\t' || '{}' || E'\\t' || count(*)::text FROM \"{}\".\"{}\";\n",
            schema.as_str(),
            table.as_str(),
            schema.as_str(),
            table.as_str()
        ));
    }
    sql.push_str("ROLLBACK;\n");
    psql(&database.conn, &sql, debug)?
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut fields = line.split('\t');
            let schema = fields.next().unwrap_or_default();
            let table = fields.next().unwrap_or_default();
            let rows = fields.next().unwrap_or_default();
            if fields.next().is_some() || schema.is_empty() || table.is_empty() {
                return Err(format!(
                    "postgres returned invalid row evidence `{line}`.\n       fix: verify the selected psql client and retry."
                )
                .into());
            }
            Ok(RowEvidence {
                schema: schema.to_string(),
                table: table.to_string(),
                rows: rows.parse().map_err(|_| {
                    format!(
                        "postgres returned invalid row count `{rows}`.\n       fix: verify SELECT permission on `{schema}.{table}`."
                    )
                })?,
            })
        })
        .collect()
}

/// Derive the expected server major from the explicitly declared PostgreSQL
/// image. This is stable checked project evidence, not container discovery.
pub fn declared_server_major(project: &Project, datasource: &str) -> Result<u32> {
    if datasource != "postgres" {
        return Err(format!(
            "unknown live datasource `{datasource}`.\n       fix: select the declared PostgreSQL datasource with `--datasource postgres`."
        )
        .into());
    }
    let yaml = compose::read(project.root())?;
    let image = yaml
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("image: postgres:"))
        .ok_or_else(|| {
            "the declared PostgreSQL image has no literal major version.\n       fix: pin it as `postgres:<major>` before using `--frozen --live`."
                .to_string()
        })?;
    image
        .split(|ch: char| !ch.is_ascii_digit())
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "the declared PostgreSQL image major is invalid.\n       fix: pin it as `postgres:<major>`."
                .to_string()
        })?
        .parse()
        .map_err(|_| "the declared PostgreSQL image major is invalid.".into())
}

fn parse_observed(output: &str) -> Result<BTreeMap<SchemaObjectId, SchemaObject>> {
    let mut objects = BTreeMap::new();
    for (number, line) in output.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 11 {
            return Err(format!(
                "PostgreSQL catalog row {} has {} fields, expected 11.\n       fix: verify psql field-separator settings and retry.",
                number + 1,
                fields.len()
            )
            .into());
        }
        let decoded = fields[1..]
            .iter()
            .map(|field| unhex(field))
            .collect::<Result<Vec<_>>>()?;
        let schema = &decoded[0];
        let name = &decoded[1];
        let parent = &decoded[2];
        let details = &decoded[3..];
        let (id, object) = observed_object(fields[0], schema, name, parent, details)?;
        if objects.insert(id.clone(), object).is_some() {
            return Err(format!(
                "PostgreSQL observation produced duplicate {:?} identity `{}.{}`.\n       fix: report this catalog normalization bug; no schema mutation is safe.",
                id.kind,
                id.namespace.as_str(),
                id.name.as_str()
            )
            .into());
        }
    }
    Ok(objects)
}

fn observed_object(
    kind: &str,
    schema: &str,
    name: &str,
    parent: &str,
    details: &[String],
) -> Result<(SchemaObjectId, SchemaObject)> {
    let object_kind = parse_kind(kind)?;
    let namespace = SqlName::parse(schema).map_err(|_| opaque_name_error(kind, schema, name))?;
    let object_name = SqlName::parse(name).map_err(|_| opaque_name_error(kind, schema, name))?;
    let parent_name = if parent.is_empty() {
        None
    } else {
        Some(QualifiedSqlName {
            namespace: Some(namespace.clone()),
            name: SqlName::parse(parent).map_err(|_| opaque_name_error(kind, schema, parent))?,
        })
    };
    let id = SchemaObjectId {
        dialect: SqlDialect::PostgreSql,
        namespace: namespace.clone(),
        kind: object_kind,
        name: object_name,
        parent: parent_name,
    };
    let object = match object_kind {
        SchemaObjectKind::Schema => SchemaObject::Schema,
        SchemaObjectKind::Table => SchemaObject::Table,
        SchemaObjectKind::Column => SchemaObject::Column {
            sql_type: SqlTypeName::parse(&details[0])?,
            nullable: parse_bool(&details[1])?,
            ordinal: details[2].parse().map_err(|_| {
                format!(
                    "invalid live column ordinal `{}`.\n       fix: report this PostgreSQL catalog normalization bug.",
                    details[2]
                )
            })?,
            default_expression: optional(&details[3]),
            generated: optional(&details[4]),
            identity: optional(&details[5]),
            comment: optional(&details[6]),
        },
        SchemaObjectKind::PrimaryKey => SchemaObject::PrimaryKey {
            columns: names(&details[0])?,
        },
        SchemaObjectKind::ForeignKey => SchemaObject::ForeignKey {
            definition: details[0].clone(),
            referenced_table: SchemaObjectId {
                dialect: SqlDialect::PostgreSql,
                namespace: SqlName::parse(&details[1])?,
                kind: SchemaObjectKind::Table,
                name: SqlName::parse(&details[2])?,
                parent: None,
            },
        },
        SchemaObjectKind::Unique => SchemaObject::Unique {
            definition: details[0].clone(),
        },
        SchemaObjectKind::Index => SchemaObject::Index {
            definition: details[0].clone(),
        },
        SchemaObjectKind::Check => SchemaObject::Check {
            definition: details[0].clone(),
        },
        SchemaObjectKind::Enum => SchemaObject::Enum {
            labels: details[0]
                .split(',')
                .filter(|label| !label.is_empty())
                .map(unhex)
                .collect::<Result<Vec<_>>>()?,
        },
        SchemaObjectKind::Domain => SchemaObject::Domain {
            definition: details[0].clone(),
        },
        SchemaObjectKind::View => SchemaObject::View {
            definition: details[0].clone(),
        },
        SchemaObjectKind::Routine => SchemaObject::Routine {
            definition: details[0].clone(),
        },
        SchemaObjectKind::Policy => SchemaObject::Policy {
            definition: details[0].clone(),
        },
        SchemaObjectKind::Opaque => SchemaObject::Opaque {
            definition: details[0].clone(),
        },
    };
    Ok((id, object))
}

fn optional(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

fn parse_bool(value: &str) -> Result<bool> {
    match value {
        "t" | "true" => Ok(true),
        "f" | "false" => Ok(false),
        other => Err(format!(
            "invalid live PostgreSQL boolean `{other}`.\n       fix: verify psql is using its canonical unaligned output."
        )
        .into()),
    }
}

fn names(value: &str) -> Result<Vec<SqlName>> {
    value
        .split(',')
        .filter(|name| !name.is_empty())
        .map(SqlName::parse)
        .collect()
}

fn parse_kind(kind: &str) -> Result<SchemaObjectKind> {
    match kind {
        "schema" => Ok(SchemaObjectKind::Schema),
        "table" => Ok(SchemaObjectKind::Table),
        "column" => Ok(SchemaObjectKind::Column),
        "primary_key" => Ok(SchemaObjectKind::PrimaryKey),
        "foreign_key" => Ok(SchemaObjectKind::ForeignKey),
        "unique" => Ok(SchemaObjectKind::Unique),
        "index" => Ok(SchemaObjectKind::Index),
        "check" => Ok(SchemaObjectKind::Check),
        "enum" => Ok(SchemaObjectKind::Enum),
        "domain" => Ok(SchemaObjectKind::Domain),
        "view" => Ok(SchemaObjectKind::View),
        "routine" => Ok(SchemaObjectKind::Routine),
        "policy" => Ok(SchemaObjectKind::Policy),
        "opaque" => Ok(SchemaObjectKind::Opaque),
        other => Err(format!(
            "unknown PostgreSQL catalog row kind `{other}`.\n       fix: upgrade jails or report this observer bug."
        )
        .into()),
    }
}

fn opaque_name_error(kind: &str, schema: &str, name: &str) -> jails_support::Failure {
    format!(
        "live {kind} `{schema}.{name}` uses a quoted or noncanonical identifier.\n       fix: ignore it explicitly or rename it before schema reconciliation."
    )
    .into()
}

fn unhex(value: &str) -> Result<String> {
    if !value.len().is_multiple_of(2) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(
            "PostgreSQL catalog emitted invalid hexadecimal data.\n       fix: verify the selected psql client and retry."
                .into(),
        );
    }
    let bytes = value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).expect("hex is ASCII");
            u8::from_str_radix(text, 16).expect("validated hex")
        })
        .collect::<Vec<_>>();
    String::from_utf8(bytes).map_err(|_| {
        "PostgreSQL catalog text is not UTF-8.\n       fix: use a UTF-8 database encoding for schema observation."
            .into()
    })
}

fn server_major(conn: &PostgresConnect, debug: bool) -> Result<u32> {
    let output = psql(conn, "SHOW server_version_num;\n", debug)?;
    let version = output.trim().parse::<u32>().map_err(|_| {
        format!(
            "postgres returned an invalid server_version_num `{}`.\n       fix: verify the selected datasource is PostgreSQL.",
            output.trim()
        )
    })?;
    Ok(version / 10_000)
}

fn describe(
    conn: &PostgresConnect,
    query: &CheckedQuery,
    server_major: u32,
    debug: bool,
) -> Result<LiveDescription> {
    let statement = jails_project::query_compiler::live_description_sql(&query.source)?;
    let script = format!("BEGIN READ ONLY;\n{statement}\n\\gdesc\nROLLBACK;\n");
    let output = psql(conn, &script, debug)?;
    let columns = output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.split_once('\t')
                .map(|(name, sql_type)| (name.to_string(), sql_type.to_string()))
                .ok_or_else(|| {
                    format!(
                        "postgres returned an invalid description row `{line}`.\n       fix: check that psql is compatible with the selected server."
                    )
                    .into()
                })
        })
        .collect::<Result<Vec<_>>>()?;
    let expected = query
        .contract
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>();
    let actual = columns
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    if actual != expected {
        return Err(format!(
            "live result columns for `{}.{}` differ (expected: {}; live: {}).\n       fix: reconcile the query or migrations before generating Java.",
            query.source.id.slice.as_str(),
            query.source.id.name.as_str(),
            expected.join(", "),
            actual.join(", ")
        )
        .into());
    }
    Ok(LiveDescription {
        server_major,
        columns,
    })
}

fn psql(conn: &PostgresConnect, sql: &str, debug: bool) -> Result<String> {
    let port = conn.port.to_string();
    let spec = CommandSpec::new("psql")
        .args([
            "-h",
            conn.host.as_str(),
            "-p",
            port.as_str(),
            "-U",
            conn.user.as_str(),
            "-d",
            conn.database.as_str(),
            "-v",
            "ON_ERROR_STOP=1",
            "--no-psqlrc",
            "-qAt",
            "-F",
            "\t",
            "-f",
            "-",
        ])
        .secret_env("PGPASSWORD", &conn.password)
        .stdin(sql.as_bytes().to_vec())
        .output(OutputMode::Capture);
    let done = crate::process::run(&spec, Diagnostics::from_flag(debug))?;
    if !done.status.success() {
        return Err(format!(
            "live PostgreSQL description failed: {}.\n       fix: reconcile the query and selected datasource, then retry.",
            String::from_utf8_lossy(&done.stderr).trim()
        )
        .into());
    }
    Ok(done.stdout_string())
}

// Every text field is UTF-8 hex, so an identifier, comment, expression, or
// enum label containing tabs/newlines cannot corrupt the row protocol. OIDs
// are used only for joins inside one snapshot and never leave the server.
const OBSERVE_SQL: &str = r#"BEGIN READ ONLY;
WITH observed(kind, schema_name, object_name, parent_name, d1, d2, d3, d4, d5, d6, d7) AS (
  SELECT 'schema', n.nspname, n.nspname, '', '', '', '', '', '', '', ''
  FROM pg_catalog.pg_namespace n
  WHERE n.nspname = '__JAILS_SCHEMA__'

  UNION ALL
  SELECT 'table', n.nspname, c.relname, '', '', '', '', '', '', '', ''
  FROM pg_catalog.pg_class c
  JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
  WHERE n.nspname = '__JAILS_SCHEMA__' AND c.relkind IN ('r', 'p')
    AND NOT EXISTS (
      SELECT 1 FROM pg_catalog.pg_depend d
      WHERE d.classid = 'pg_catalog.pg_class'::regclass
        AND d.objid = c.oid AND d.deptype = 'e'
    )

  UNION ALL
  SELECT 'column', n.nspname, a.attname, c.relname,
         CASE WHEN tn.nspname = 'pg_catalog' THEN t.typname ELSE tn.nspname || '.' || t.typname END,
         (NOT a.attnotnull)::text, a.attnum::text,
         COALESCE(pg_catalog.pg_get_expr(ad.adbin, ad.adrelid), ''),
         NULLIF(a.attgenerated, ''), NULLIF(a.attidentity, ''),
         COALESCE(pg_catalog.col_description(c.oid, a.attnum), '')
  FROM pg_catalog.pg_attribute a
  JOIN pg_catalog.pg_class c ON c.oid = a.attrelid
  JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
  JOIN pg_catalog.pg_type t ON t.oid = a.atttypid
  JOIN pg_catalog.pg_namespace tn ON tn.oid = t.typnamespace
  LEFT JOIN pg_catalog.pg_attrdef ad ON ad.adrelid = a.attrelid AND ad.adnum = a.attnum
  WHERE n.nspname = '__JAILS_SCHEMA__' AND c.relkind IN ('r', 'p', 'v', 'm')
    AND a.attnum > 0 AND NOT a.attisdropped
    AND NOT EXISTS (
      SELECT 1 FROM pg_catalog.pg_depend d
      WHERE d.classid = 'pg_catalog.pg_class'::regclass
        AND d.objid = c.oid AND d.deptype = 'e'
    )

  UNION ALL
  SELECT CASE con.contype WHEN 'p' THEN 'primary_key' WHEN 'f' THEN 'foreign_key'
                          WHEN 'u' THEN 'unique' ELSE 'check' END,
         n.nspname, con.conname, c.relname,
         CASE WHEN con.contype = 'p' THEN COALESCE((
           SELECT string_agg(a.attname, ',' ORDER BY key.ordinality)
           FROM unnest(con.conkey) WITH ORDINALITY key(attnum, ordinality)
           JOIN pg_catalog.pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = key.attnum
         ), '') ELSE pg_catalog.pg_get_constraintdef(con.oid, false) END,
         COALESCE(rn.nspname, ''), COALESCE(rc.relname, ''), '', '', '', ''
  FROM pg_catalog.pg_constraint con
  JOIN pg_catalog.pg_class c ON c.oid = con.conrelid
  JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
  LEFT JOIN pg_catalog.pg_class rc ON rc.oid = con.confrelid
  LEFT JOIN pg_catalog.pg_namespace rn ON rn.oid = rc.relnamespace
  WHERE n.nspname = '__JAILS_SCHEMA__' AND con.contype IN ('p', 'f', 'u', 'c')
    AND NOT EXISTS (
      SELECT 1 FROM pg_catalog.pg_depend d
      WHERE d.classid = 'pg_catalog.pg_constraint'::regclass
        AND d.objid = con.oid AND d.deptype = 'e'
    )

  UNION ALL
  SELECT 'index', n.nspname, ic.relname, tc.relname,
         pg_catalog.pg_get_indexdef(i.indexrelid), '', '', '', '', '', ''
  FROM pg_catalog.pg_index i
  JOIN pg_catalog.pg_class ic ON ic.oid = i.indexrelid
  JOIN pg_catalog.pg_class tc ON tc.oid = i.indrelid
  JOIN pg_catalog.pg_namespace n ON n.oid = tc.relnamespace
  WHERE n.nspname = '__JAILS_SCHEMA__' AND NOT i.indisprimary
    AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_constraint con WHERE con.conindid = i.indexrelid)
    AND NOT EXISTS (
      SELECT 1 FROM pg_catalog.pg_depend d
      WHERE d.classid = 'pg_catalog.pg_class'::regclass
        AND d.objid = ic.oid AND d.deptype = 'e'
    )

  UNION ALL
  SELECT 'enum', n.nspname, t.typname, '',
         string_agg(encode(convert_to(e.enumlabel, 'UTF8'), 'hex'), ',' ORDER BY e.enumsortorder),
         '', '', '', '', '', ''
  FROM pg_catalog.pg_type t
  JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
  JOIN pg_catalog.pg_enum e ON e.enumtypid = t.oid
  WHERE n.nspname = '__JAILS_SCHEMA__'
    AND NOT EXISTS (
      SELECT 1 FROM pg_catalog.pg_depend d
      WHERE d.classid = 'pg_catalog.pg_type'::regclass
        AND d.objid = t.oid AND d.deptype = 'e'
    )
  GROUP BY n.nspname, t.typname

  UNION ALL
  SELECT 'domain', n.nspname, t.typname, '',
         pg_catalog.format_type(t.typbasetype, t.typtypmod)
           || CASE WHEN t.typnotnull THEN ' NOT NULL' ELSE '' END
           || CASE WHEN t.typdefault IS NULL THEN '' ELSE ' DEFAULT ' || t.typdefault END
           || COALESCE((
             SELECT ' ' || string_agg(pg_catalog.pg_get_constraintdef(con.oid, false), ' ' ORDER BY con.conname)
             FROM pg_catalog.pg_constraint con
             WHERE con.contypid = t.oid
           ), ''),
         '', '', '', '', '', ''
  FROM pg_catalog.pg_type t
  JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
  WHERE n.nspname = '__JAILS_SCHEMA__' AND t.typtype = 'd'
    AND NOT EXISTS (
      SELECT 1 FROM pg_catalog.pg_depend d
      WHERE d.classid = 'pg_catalog.pg_type'::regclass
        AND d.objid = t.oid AND d.deptype = 'e'
    )

  UNION ALL
  SELECT 'view', n.nspname, c.relname, '', pg_catalog.pg_get_viewdef(c.oid, false),
         '', '', '', '', '', ''
  FROM pg_catalog.pg_class c
  JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
  WHERE n.nspname = '__JAILS_SCHEMA__' AND c.relkind IN ('v', 'm')
    AND NOT EXISTS (
      SELECT 1 FROM pg_catalog.pg_depend d
      WHERE d.classid = 'pg_catalog.pg_class'::regclass
        AND d.objid = c.oid AND d.deptype = 'e'
    )

  UNION ALL
  SELECT 'routine', n.nspname,
         p.proname || '_' || substr(md5(pg_catalog.pg_get_function_identity_arguments(p.oid)), 1, 8),
         '', pg_catalog.pg_get_functiondef(p.oid), '', '', '', '', '', ''
  FROM pg_catalog.pg_proc p
  JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
  WHERE n.nspname = '__JAILS_SCHEMA__'
    AND NOT EXISTS (
      SELECT 1 FROM pg_catalog.pg_depend d
      WHERE d.classid = 'pg_catalog.pg_proc'::regclass
        AND d.objid = p.oid AND d.deptype = 'e'
    )

  UNION ALL
  SELECT 'policy', n.nspname, pol.polname, c.relname,
         'PERMISSIVE=' || pol.polpermissive::text || ';COMMAND=' || pol.polcmd
           || ';ROLES=' || pg_catalog.array_to_string(pol.polroles, ',')
           || ';USING=' || COALESCE(pg_catalog.pg_get_expr(pol.polqual, pol.polrelid), '')
           || ';CHECK=' || COALESCE(pg_catalog.pg_get_expr(pol.polwithcheck, pol.polrelid), ''),
         '', '', '', '', '', ''
  FROM pg_catalog.pg_policy pol
  JOIN pg_catalog.pg_class c ON c.oid = pol.polrelid
  JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
  WHERE n.nspname = '__JAILS_SCHEMA__'

  UNION ALL
  SELECT 'opaque', n.nspname, c.relname, '', 'unsupported relkind=' || c.relkind,
         '', '', '', '', '', ''
  FROM pg_catalog.pg_class c
  JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
  WHERE n.nspname = '__JAILS_SCHEMA__' AND c.relkind NOT IN ('r', 'p', 'v', 'm', 'i', 'I', 'S')
    AND NOT EXISTS (
      SELECT 1 FROM pg_catalog.pg_depend d
      WHERE d.classid = 'pg_catalog.pg_class'::regclass
        AND d.objid = c.oid AND d.deptype = 'e'
    )

  UNION ALL
  SELECT 'opaque', n.nspname, tg.tgname, c.relname,
         pg_catalog.pg_get_triggerdef(tg.oid, false), '', '', '', '', '', ''
  FROM pg_catalog.pg_trigger tg
  JOIN pg_catalog.pg_class c ON c.oid = tg.tgrelid
  JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
  WHERE n.nspname = '__JAILS_SCHEMA__' AND NOT tg.tgisinternal
)
SELECT kind,
       encode(convert_to(COALESCE(schema_name, ''), 'UTF8'), 'hex'),
       encode(convert_to(COALESCE(object_name, ''), 'UTF8'), 'hex'),
       encode(convert_to(COALESCE(parent_name, ''), 'UTF8'), 'hex'),
       encode(convert_to(COALESCE(d1, ''), 'UTF8'), 'hex'),
       encode(convert_to(COALESCE(d2, ''), 'UTF8'), 'hex'),
       encode(convert_to(COALESCE(d3, ''), 'UTF8'), 'hex'),
       encode(convert_to(COALESCE(d4, ''), 'UTF8'), 'hex'),
       encode(convert_to(COALESCE(d5, ''), 'UTF8'), 'hex'),
       encode(convert_to(COALESCE(d6, ''), 'UTF8'), 'hex'),
       encode(convert_to(COALESCE(d7, ''), 'UTF8'), 'hex')
FROM observed
ORDER BY kind, schema_name, parent_name, object_name, d1, d2, d3, d4, d5, d6, d7;
ROLLBACK;
"#;

fn wait_until_ready(conn: &PostgresConnect, debug: bool) -> Result<()> {
    let mut last = String::new();
    for attempt in 0..120 {
        match psql(conn, "SELECT 1;\n", debug && attempt == 0) {
            Ok(_) => return Ok(()),
            Err(error) => last = error.to_string(),
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!(
        "postgres at {}:{} did not accept connections within 30 seconds -- last error: {last}.\n       fix: inspect the declared service and retry when it is ready.",
        conn.host, conn.port
    )
    .into())
}
