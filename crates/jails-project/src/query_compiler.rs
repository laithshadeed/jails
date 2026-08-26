//! Reader-owned SQL parsing and the bounded offline catalog compiler.
//!
//! `sqlparser` supplies syntax and spans. It is deliberately not treated as a
//! PostgreSQL server: only catalog facts this module explicitly recognizes are
//! admitted. Every other migration statement is retained as an opaque blocker.

use jails_protocol::database::{
    ByteSpan, Cardinality, CatalogSnapshot, DeclaredParameter, OpaqueMigrationStatement,
    QualifiedSqlName, QueryId, QueryName, QuerySource, SchemaObject, SchemaObjectId,
    SchemaObjectKind, SliceName, SqlDialect, SqlTypeName,
};
use jails_protocol::identity::{Name, ObjectId, ProjectPath, SqlName};
use jails_support::Result;
use jails_support::codec::domain_hash;
use sqlparser::ast::{
    ColumnOption, Expr, Ident, ObjectName, ObjectNamePart, Statement, TableConstraint,
};
use sqlparser::dialect::{Dialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser::parser::Parser;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryFile<'a> {
    pub slice: &'a str,
    pub path: &'a str,
    pub contents: &'a str,
}

/// Parse several query files and enforce the project-wide qualified identity.
pub fn parse_query_files(files: &[QueryFile<'_>], dialect: SqlDialect) -> Result<Vec<QuerySource>> {
    let mut seen = BTreeSet::new();
    let mut parsed = Vec::with_capacity(files.len());
    for file in files {
        let source = parse_query_file(file.slice, file.path, file.contents, dialect)?;
        if !seen.insert(source.id.clone()) {
            return Err(format!(
                "query `{}.{}` is declared more than once.\n       fix: keep one file or rename one `jails:name` directive.",
                source.id.slice.as_str(),
                source.id.name.as_str()
            )
            .into());
        }
        parsed.push(source);
    }
    Ok(parsed)
}

/// Decode directives without rewriting the SQL bytes below them.
pub fn parse_query_file(
    slice: &str,
    path: &str,
    contents: &str,
    dialect: SqlDialect,
) -> Result<QuerySource> {
    let normalized = contents.replace("\r\n", "\n").replace('\r', "\n");
    let mut name = None;
    let mut cardinality = None;
    let mut parameters = Vec::new();
    let mut sql = String::new();
    let mut offset = 0usize;
    let mut statement_start = None;

    for line in normalized.split_inclusive('\n') {
        let without_newline = line.strip_suffix('\n').unwrap_or(line);
        let trimmed = without_newline.trim_start();
        if let Some(directive) = trimmed.strip_prefix("-- jails:") {
            let directive_start = offset + without_newline.len() - trimmed.len();
            parse_directive(
                directive,
                directive_start,
                &mut name,
                &mut cardinality,
                &mut parameters,
            )?;
        } else {
            if statement_start.is_none() && !trimmed.is_empty() {
                statement_start = Some(offset);
            }
            sql.push_str(line);
        }
        offset += line.len();
    }

    let name = name.ok_or_else(|| {
        "query has no `-- jails:name Name` directive.\n       fix: add one uppercase query name."
            .to_string()
    })?;
    let cardinality = cardinality.ok_or_else(|| {
        "query has no `-- jails:cardinality ...` directive.\n       fix: declare one, optional, many, exec, or execrows."
            .to_string()
    })?;
    let start = statement_start.ok_or_else(|| {
        "query contains directives but no SQL statement.\n       fix: add exactly one terminated statement."
            .to_string()
    })?;
    if !sql.trim_end().ends_with(';') {
        return Err(
            "query statement is not terminated.\n       fix: end the reader-owned SQL statement with `;`."
                .into(),
        );
    }

    let statements = Parser::parse_sql(parser_dialect(dialect), &sql).map_err(|error| {
        format!("query SQL does not parse: {error}.\n       fix: correct the SQL at the reported location.")
    })?;
    if statements.len() != 1 {
        return Err(format!(
            "query block contains {} statements.\n       fix: keep exactly one terminated statement per query file.",
            statements.len()
        )
        .into());
    }

    validate_parameter_uses(&sql, &parameters)?;
    QuerySource::new(
        QueryId::new(SliceName::parse(slice)?, name),
        ProjectPath::parse(path)?,
        ByteSpan::new(start, normalized.trim_end().len())?,
        &sql,
        cardinality,
        parameters,
    )
}

fn parse_directive(
    directive: &str,
    offset: usize,
    name: &mut Option<QueryName>,
    cardinality: &mut Option<Cardinality>,
    parameters: &mut Vec<DeclaredParameter>,
) -> Result<()> {
    let (key, value) = directive.trim().split_once(' ').ok_or_else(|| {
        format!(
            "incomplete SQL directive `{}`.\n       fix: supply its value after one space.",
            directive.trim()
        )
    })?;
    match key {
        "name" => set_once(name, QueryName::parse(value.trim())?, "name"),
        "cardinality" => set_once(
            cardinality,
            Cardinality::parse(value.trim())?,
            "cardinality",
        ),
        "param" => {
            let mut words = value.split_whitespace();
            let parameter_name = words
                .next()
                .ok_or("missing parameter name.\n       fix: write `-- jails:param name text`.")?;
            let type_word = words.next().ok_or_else(|| {
                format!(
                    "parameter `{parameter_name}` has no SQL type.\n       fix: write `-- jails:param {parameter_name} text`."
                )
            })?;
            if words.next().is_some() {
                return Err(format!(
                    "parameter `{parameter_name}` has extra directive words.\n       fix: use `<name> <sql-type>` with an optional `?` suffix."
                )
                .into());
            }
            if parameters
                .iter()
                .any(|parameter| parameter.name.as_str() == parameter_name)
            {
                return Err(format!(
                    "parameter `{parameter_name}` is declared more than once.\n       fix: keep exactly one declaration."
                )
                .into());
            }
            let (type_name, nullable) = type_word
                .strip_suffix('?')
                .map_or((type_word, false), |value| (value, true));
            parameters.push(DeclaredParameter {
                name: Name::parse(parameter_name)?,
                sql_type: SqlTypeName::parse(type_name)?,
                nullable,
                span: ByteSpan::new(offset, offset + directive.len())?,
            });
            Ok(())
        }
        other => Err(format!(
            "unknown SQL directive `jails:{other}`.\n       fix: use name, cardinality, or param."
        )
        .into()),
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T, label: &str) -> Result<()> {
    if slot.is_some() {
        return Err(format!(
            "query declares `jails:{label}` more than once.\n       fix: keep exactly one directive."
        )
        .into());
    }
    *slot = Some(value);
    Ok(())
}

#[derive(Debug, Default)]
struct ParameterUses {
    named: BTreeSet<String>,
    positional: bool,
}

fn validate_parameter_uses(sql: &str, declared: &[DeclaredParameter]) -> Result<()> {
    let uses = parameter_uses(sql);
    if uses.positional && !uses.named.is_empty() {
        return Err(
            "query mixes positional and named parameters.\n       fix: use only `:name` parameters."
                .into(),
        );
    }
    if uses.positional {
        return Err(
            "query uses positional parameters.\n       fix: declare and use stable `:name` parameters."
                .into(),
        );
    }
    let declared: BTreeSet<String> = declared
        .iter()
        .map(|parameter| parameter.name.as_str().to_string())
        .collect();
    if uses.named != declared {
        let missing: Vec<_> = declared.difference(&uses.named).cloned().collect();
        let undeclared: Vec<_> = uses.named.difference(&declared).cloned().collect();
        return Err(format!(
            "query parameter declarations and uses differ (unused: {}; undeclared: {}).\n       fix: make every `jails:param` and `:name` use match exactly.",
            display_names(&missing),
            display_names(&undeclared)
        )
        .into());
    }
    Ok(())
}

fn display_names(names: &[String]) -> String {
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

/// Find placeholders while excluding casts, literals, identifiers and every
/// PostgreSQL comment/string form that can legally contain a colon.
fn parameter_uses(sql: &str) -> ParameterUses {
    let bytes = sql.as_bytes();
    let mut uses = ParameterUses::default();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\'' => cursor = quoted_end(bytes, cursor + 1, b'\'', true),
            b'"' => cursor = quoted_end(bytes, cursor + 1, b'"', true),
            b'-' if bytes.get(cursor + 1) == Some(&b'-') => {
                cursor = bytes[cursor + 2..]
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(bytes.len(), |next| cursor + 3 + next);
            }
            b'/' if bytes.get(cursor + 1) == Some(&b'*') => {
                cursor = block_comment_end(bytes, cursor + 2);
            }
            b'$' => {
                if bytes.get(cursor + 1).is_some_and(u8::is_ascii_digit) {
                    uses.positional = true;
                    cursor += 2;
                } else if let Some((delimiter, body)) = dollar_delimiter(bytes, cursor) {
                    cursor = find_bytes(bytes, body, &delimiter).unwrap_or(bytes.len());
                } else {
                    cursor += 1;
                }
            }
            b':' if bytes.get(cursor + 1) == Some(&b':') => cursor += 2,
            b':' if bytes
                .get(cursor + 1)
                .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_') =>
            {
                let start = cursor + 1;
                cursor = start + 1;
                while bytes
                    .get(cursor)
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                {
                    cursor += 1;
                }
                uses.named
                    .insert(String::from_utf8_lossy(&bytes[start..cursor]).into_owned());
            }
            _ => cursor += 1,
        }
    }
    uses
}

fn quoted_end(bytes: &[u8], mut cursor: usize, quote: u8, doubled: bool) -> usize {
    while cursor < bytes.len() {
        if bytes[cursor] == quote {
            if doubled && bytes.get(cursor + 1) == Some(&quote) {
                cursor += 2;
            } else {
                return cursor + 1;
            }
        } else {
            cursor += 1;
        }
    }
    bytes.len()
}

fn block_comment_end(bytes: &[u8], mut cursor: usize) -> usize {
    let mut depth = 1usize;
    while cursor + 1 < bytes.len() {
        match (bytes[cursor], bytes[cursor + 1]) {
            (b'/', b'*') => {
                depth += 1;
                cursor += 2;
            }
            (b'*', b'/') => {
                depth -= 1;
                cursor += 2;
                if depth == 0 {
                    return cursor;
                }
            }
            _ => cursor += 1,
        }
    }
    bytes.len()
}

fn dollar_delimiter(bytes: &[u8], start: usize) -> Option<(Vec<u8>, usize)> {
    let mut cursor = start + 1;
    while bytes
        .get(cursor)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        cursor += 1;
    }
    (bytes.get(cursor) == Some(&b'$')).then(|| (bytes[start..=cursor].to_vec(), cursor + 1))
}

fn find_bytes(haystack: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    haystack[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| start + offset + needle.len())
}

pub fn compile_catalog(
    dialect: SqlDialect,
    migrations: &[(ProjectPath, String)],
) -> Result<CatalogSnapshot> {
    let mut objects = BTreeMap::new();
    let mut opaque = Vec::new();
    for (path, source) in migrations {
        let statements = Parser::parse_sql(parser_dialect(dialect), source).map_err(|error| {
            format!(
                "migration `{path:?}` does not parse: {error}.\n       fix: correct the migration before checking queries."
            )
        })?;
        for statement in statements {
            if let Statement::CreateTable(table) = &statement
                && let Ok(facts) = create_table_facts(dialect, table)
            {
                for (id, object) in facts {
                    if objects.insert(id, object).is_some() {
                        return Err(
                            "migration catalog defines one schema object twice.\n       fix: remove the duplicate DDL or use an explicit ALTER supported by a live check."
                                .into(),
                        );
                    }
                }
                continue;
            }
            let rendered = statement.to_string();
            opaque.push(OpaqueMigrationStatement {
                path: path.clone(),
                span: ByteSpan::new(0, source.len())?,
                digest: ObjectId::from_bytes(domain_hash(
                    "JAILS-OPAQUE-MIGRATION-1",
                    rendered.as_bytes(),
                )),
                reason: format!(
                    "offline catalog does not prove `{}` semantics",
                    statement_kind(&statement)
                ),
            });
        }
    }
    CatalogSnapshot::new(dialect, objects, opaque)
}

fn create_table_facts(
    dialect: SqlDialect,
    table: &sqlparser::ast::CreateTable,
) -> Result<Vec<(SchemaObjectId, SchemaObject)>> {
    if table.temporary
        || table.external
        || table.query.is_some()
        || table.like.is_some()
        || table.clone.is_some()
        || table.inherits.is_some()
        || table.partition_of.is_some()
    {
        return Err(
            "unsupported CREATE TABLE shape.\n       fix: use a live check or simplify the migration to a plain table declaration."
                .into(),
        );
    }
    let (namespace, table_name) = qualified_name(&table.name)?;
    let qualified_table = QualifiedSqlName {
        namespace: Some(namespace.clone()),
        name: table_name.clone(),
    };
    let table_id = schema_id(
        dialect,
        &namespace,
        SchemaObjectKind::Table,
        table_name.clone(),
        None,
    );
    let mut facts = vec![(table_id, SchemaObject::Table)];
    let mut primary_key = Vec::new();
    for (ordinal, column) in table.columns.iter().enumerate() {
        let name = unquoted_name(&column.name.value, column.name.quote_style)?;
        let sql_type = catalog_type(&column.data_type.to_string())?;
        let mut nullable = true;
        for option in &column.options {
            match &option.option {
                ColumnOption::Null | ColumnOption::Default(_) => {}
                ColumnOption::NotNull => nullable = false,
                ColumnOption::PrimaryKey(_) => {
                    nullable = false;
                    primary_key.push(name.clone());
                }
                _ => {
                    return Err(
                        "unsupported column option.\n       fix: use a live check for this migration."
                            .into(),
                    );
                }
            }
        }
        facts.push((
            schema_id(
                dialect,
                &namespace,
                SchemaObjectKind::Column,
                name,
                Some(qualified_table.clone()),
            ),
            SchemaObject::Column {
                sql_type,
                nullable,
                ordinal: u32::try_from(ordinal).map_err(|_| {
                    "too many table columns.\n       fix: split this schema into smaller tables."
                })?,
            },
        ));
    }
    for constraint in &table.constraints {
        match constraint {
            TableConstraint::PrimaryKey(key) => {
                if !primary_key.is_empty() || key.columns.is_empty() {
                    return Err(
                        "ambiguous primary key.\n       fix: declare one table-level or column-level primary key."
                            .into(),
                    );
                }
                for column in &key.columns {
                    match &column.column.expr {
                        Expr::Identifier(identifier) => primary_key
                            .push(unquoted_name(&identifier.value, identifier.quote_style)?),
                        _ => {
                            return Err(
                                "expression primary key.\n       fix: use named columns or run a live check."
                                    .into(),
                            );
                        }
                    }
                }
            }
            _ => {
                return Err(
                    "unsupported table constraint.\n       fix: use a live check for this migration."
                        .into(),
                );
            }
        }
    }
    if !primary_key.is_empty() {
        facts.push((
            schema_id(
                dialect,
                &namespace,
                SchemaObjectKind::PrimaryKey,
                SqlName::parse(&format!("{}_pkey", table_name.as_str()))?,
                Some(qualified_table),
            ),
            SchemaObject::PrimaryKey {
                columns: primary_key,
            },
        ));
    }
    Ok(facts)
}

fn parser_dialect(dialect: SqlDialect) -> &'static dyn Dialect {
    static POSTGRES: PostgreSqlDialect = PostgreSqlDialect {};
    static MYSQL: MySqlDialect = MySqlDialect {};
    static SQLITE: SQLiteDialect = SQLiteDialect {};
    match dialect {
        SqlDialect::PostgreSql => &POSTGRES,
        SqlDialect::MySql => &MYSQL,
        SqlDialect::Sqlite => &SQLITE,
    }
}

fn qualified_name(name: &ObjectName) -> Result<(SqlName, SqlName)> {
    let parts = &name.0;
    match parts.as_slice() {
        [table] => Ok((SqlName::parse("public")?, object_name_part(table)?)),
        [schema, table] => Ok((object_name_part(schema)?, object_name_part(table)?)),
        _ => Err(
            "offline catalog supports one schema qualifier.\n       fix: use a live check for this identity."
                .into(),
        ),
    }
}

fn object_name_part(part: &ObjectNamePart) -> Result<SqlName> {
    match part {
        ObjectNamePart::Identifier(identifier) => identifier_name(identifier),
        ObjectNamePart::Function(_) => Err(
            "dynamic SQL identities require live verification.\n       fix: run a live check."
                .into(),
        ),
    }
}

fn identifier_name(identifier: &Ident) -> Result<SqlName> {
    unquoted_name(&identifier.value, identifier.quote_style)
}

fn unquoted_name(value: &str, quote: Option<char>) -> Result<SqlName> {
    if quote.is_some() {
        return Err(
            "quoted SQL identities require live verification.\n       fix: run a live check or use an unquoted lowercase identity."
                .into(),
        );
    }
    SqlName::parse(&value.to_ascii_lowercase())
}

fn catalog_type(value: &str) -> Result<SqlTypeName> {
    let lowercase = value.to_ascii_lowercase();
    let normalized = match lowercase.as_str() {
        "smallint" => "int2",
        "integer" | "int" => "int4",
        "bigint" => "int8",
        "boolean" => "bool",
        "real" => "float4",
        "double precision" => "float8",
        "timestamp with time zone" => "timestamptz",
        "timestamp without time zone" => "timestamp",
        "character varying" | "varchar" => "text",
        other => other,
    };
    SqlTypeName::parse(normalized)
}

fn schema_id(
    dialect: SqlDialect,
    namespace: &SqlName,
    kind: SchemaObjectKind,
    name: SqlName,
    parent: Option<QualifiedSqlName>,
) -> SchemaObjectId {
    SchemaObjectId {
        dialect,
        namespace: namespace.clone(),
        kind,
        name,
        parent,
    }
}

fn statement_kind(statement: &Statement) -> &'static str {
    match statement {
        Statement::CreateTable(_) => "CREATE TABLE",
        Statement::AlterTable(_) => "ALTER TABLE",
        Statement::CreateIndex(_) => "CREATE INDEX",
        Statement::CreateView(_) => "CREATE VIEW",
        Statement::Query(_) => "query",
        _ => "migration statement",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(sql: &str) -> Result<QuerySource> {
        parse_query_file(
            "Accounts",
            "src/main/resources/db/queries/FindItems.sql",
            sql,
            SqlDialect::PostgreSql,
        )
    }

    #[test]
    fn directives_are_removed_but_reader_sql_bytes_are_preserved() {
        let parsed = query(
            "-- jails:name FindItems\r\n-- jails:cardinality many\r\n-- jails:param status text\r\nSELECT  id\r\nFROM items\r\nWHERE status = :status;\r\n",
        )
        .unwrap();
        assert_eq!(
            parsed.sql,
            "SELECT  id\nFROM items\nWHERE status = :status;\n"
        );
        assert_eq!(parsed.declared_parameters.len(), 1);
    }

    #[test]
    fn casts_strings_comments_identifiers_and_dollar_quotes_are_not_parameters() {
        let parsed = query(
            r#"-- jails:name FindItems
-- jails:cardinality many
-- jails:param wanted text
SELECT ':literal', "colon:name", $$:body$$, $tag$:tagged$tag$, value::text
FROM items /* :blocked /* :nested */ */
WHERE value = :wanted -- :comment
;
"#,
        )
        .unwrap();
        assert_eq!(parsed.declared_parameters[0].name.as_str(), "wanted");
    }

    #[test]
    fn declarations_and_uses_must_match_exactly() {
        let error = query(
            "-- jails:name FindItems\n-- jails:cardinality many\n-- jails:param wanted text\nSELECT * FROM items WHERE value = :other;\n",
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unused: wanted; undeclared: other")
        );
        assert!(error.to_string().contains("fix:"));
    }

    #[test]
    fn one_terminated_statement_is_required() {
        let error =
            query("-- jails:name FindItems\n-- jails:cardinality many\nSELECT 1; SELECT 2;\n")
                .unwrap_err();
        assert!(error.to_string().contains("2 statements"));
        assert!(error.to_string().contains("fix:"));
    }

    #[test]
    fn duplicate_qualified_query_names_are_refused() {
        let sql = "-- jails:name FindItems\n-- jails:cardinality many\nSELECT 1;\n";
        let error = parse_query_files(
            &[
                QueryFile {
                    slice: "Accounts",
                    path: "queries/one.sql",
                    contents: sql,
                },
                QueryFile {
                    slice: "Accounts",
                    path: "queries/two.sql",
                    contents: sql,
                },
            ],
            SqlDialect::PostgreSql,
        )
        .unwrap_err();
        assert!(error.to_string().contains("declared more than once"));
    }

    #[test]
    fn catalog_admits_plain_tables_and_marks_other_ddl_opaque() {
        let path = ProjectPath::parse("src/main/resources/db/migration/V001__items.sql").unwrap();
        let catalog = compile_catalog(
            SqlDialect::PostgreSql,
            &[(
                path,
                "CREATE TABLE items (id uuid PRIMARY KEY, label text NOT NULL); ALTER TABLE items ADD COLUMN note text;".to_string(),
            )],
        )
        .unwrap();
        assert_eq!(catalog.objects.len(), 4);
        assert_eq!(catalog.opaque.len(), 1);
        assert!(catalog.opaque[0].reason.contains("ALTER TABLE"));
    }
}
