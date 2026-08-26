//! Bounded catalog-to-query resolution and Java type mapping.

use super::{identifier_name, parser_dialect, qualified_name, schema_id};
use jails_protocol::database::{
    CatalogSnapshot, ColumnContract, DeclaredParameter, EvidenceLevel, EvidenceRecord,
    EvidenceSubject, ParameterContract, QualifiedSqlName, QueryContractV1, QueryId, QuerySource,
    SchemaObject, SchemaObjectKind, SqlDialect, SqlTypeName,
};
use jails_protocol::identity::{JavaType, Name, ObjectId, SqlName};
use jails_support::Result;
use jails_support::codec::domain_hash;
use sqlparser::ast::{Expr, JoinOperator, SelectItem, Statement, TableFactor};
use sqlparser::parser::Parser;
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
struct Relation {
    lookup: String,
    table: QualifiedSqlName,
    nullable: bool,
}

/// Compile the deliberately bounded SELECT subset into a deterministic Java
/// contract. Anything not proven by migrations requires live evidence.
pub fn compile_query(source: &QuerySource, catalog: &CatalogSnapshot) -> Result<QueryContractV1> {
    if catalog.dialect != SqlDialect::PostgreSql {
        return Err(
            "offline semantic contracts currently require PostgreSQL.\n       fix: use parse-only or a live check for this dialect."
                .into(),
        );
    }
    if !catalog.opaque.is_empty() {
        return Err(format!(
            "offline catalog contains {} opaque migration statement(s), so this query needs live verification.\n       fix: run `jails sql check --live` or simplify the unsupported migrations.",
            catalog.opaque.len()
        )
        .into());
    }

    let statements = Parser::parse_sql(parser_dialect(catalog.dialect), &source.sql).map_err(
        |error| {
            format!(
                "query SQL no longer parses: {error}.\n       fix: correct the reader-owned SQL and check again."
            )
        },
    )?;
    let [Statement::Query(query)] = statements.as_slice() else {
        return Err(
            "offline contract compiler currently proves one SELECT statement.\n       fix: use a live check for this statement shape."
                .into(),
        );
    };
    if query.with.is_some() || !query.pipe_operators.is_empty() {
        return Err(
            "offline contract compiler does not prove CTE or pipe semantics.\n       fix: use a live check for this query."
                .into(),
        );
    }
    let Some(select) = query.body.as_select() else {
        return Err(
            "offline contract compiler does not prove set-operation semantics.\n       fix: use a live check for this query."
                .into(),
        );
    };
    let relations = collect_relations(select, catalog)?;
    let columns = compile_columns(&select.projection, &relations, catalog, &source.id)?;
    let parameters = source
        .declared_parameters
        .iter()
        .map(|declared| compile_parameter(declared, &source.id))
        .collect::<Result<Vec<_>>>()?;
    let toolchain_digest = digest(
        "JAILS-SQL-TOOLCHAIN-1",
        b"sqlparser=0.62.0;catalog=jails-bounded-1;java-types=jails-postgres-1",
    );
    let details_digest = digest(
        "JAILS-SQL-OFFLINE-DETAILS-1",
        format!(
            "{}:{}:{}",
            source.query_digest(),
            catalog.digest,
            columns.len()
        )
        .as_bytes(),
    );
    Ok(QueryContractV1 {
        id: source.id.clone(),
        dialect: catalog.dialect,
        query_digest: source.query_digest(),
        catalog_digest: catalog.digest,
        cardinality: source.cardinality,
        parameters,
        columns,
        evidence: EvidenceRecord {
            subject: EvidenceSubject::Query(source.id.clone()),
            level: EvidenceLevel::VerifiedOffline,
            input_digest: source.query_digest(),
            catalog_digest: Some(catalog.digest),
            toolchain_digest,
            details_digest,
        },
    })
}

fn collect_relations(
    select: &sqlparser::ast::Select,
    catalog: &CatalogSnapshot,
) -> Result<Vec<Relation>> {
    let mut relations = Vec::new();
    for from in &select.from {
        push_relation(&mut relations, &from.relation, false, catalog)?;
        for join in &from.joins {
            let nullable = matches!(
                &join.join_operator,
                JoinOperator::Left(_) | JoinOperator::LeftOuter(_) | JoinOperator::FullOuter(_)
            );
            if matches!(
                &join.join_operator,
                JoinOperator::Right(_) | JoinOperator::RightOuter(_) | JoinOperator::FullOuter(_)
            ) {
                for relation in &mut relations {
                    relation.nullable = true;
                }
            }
            push_relation(&mut relations, &join.relation, nullable, catalog)?;
        }
    }
    if relations.is_empty() {
        return Err(
            "offline SELECT contract has no table relation.\n       fix: use a live check for expression-only queries."
                .into(),
        );
    }
    Ok(relations)
}

fn push_relation(
    relations: &mut Vec<Relation>,
    factor: &TableFactor,
    nullable: bool,
    catalog: &CatalogSnapshot,
) -> Result<()> {
    let TableFactor::Table {
        name,
        alias,
        args,
        with_hints,
        version,
        with_ordinality,
        partitions,
        json_path,
        sample,
        index_hints,
    } = factor
    else {
        return Err(
            "offline catalog does not prove derived or function table relations.\n       fix: use a live check for this query."
                .into(),
        );
    };
    if args.is_some()
        || !with_hints.is_empty()
        || version.is_some()
        || *with_ordinality
        || !partitions.is_empty()
        || json_path.is_some()
        || sample.is_some()
        || !index_hints.is_empty()
    {
        return Err(
            "offline catalog does not prove decorated table relations.\n       fix: use a live check for this query."
                .into(),
        );
    }
    let (namespace, table_name) = qualified_name(name)?;
    let table = QualifiedSqlName {
        namespace: Some(namespace.clone()),
        name: table_name.clone(),
    };
    let id = schema_id(
        catalog.dialect,
        &namespace,
        SchemaObjectKind::Table,
        table_name.clone(),
        None,
    );
    if !matches!(catalog.objects.get(&id), Some(SchemaObject::Table)) {
        return Err(format!(
            "table `{}` is absent from the offline catalog.\n       fix: add its migration or run a live check.",
            table_name.as_str()
        )
        .into());
    }
    let lookup = alias.as_ref().map_or_else(
        || table_name.as_str().to_string(),
        |value| value.name.value.to_ascii_lowercase(),
    );
    if relations.iter().any(|relation| relation.lookup == lookup) {
        return Err(format!(
            "table relation `{lookup}` is ambiguous.\n       fix: assign unique SQL aliases."
        )
        .into());
    }
    relations.push(Relation {
        lookup,
        table,
        nullable,
    });
    Ok(())
}

fn compile_columns(
    projection: &[SelectItem],
    relations: &[Relation],
    catalog: &CatalogSnapshot,
    query: &QueryId,
) -> Result<Vec<ColumnContract>> {
    let mut columns = Vec::with_capacity(projection.len());
    let mut java_names = BTreeSet::new();
    for item in projection {
        let (expression, alias) = match item {
            SelectItem::UnnamedExpr(expression) => (expression, None),
            SelectItem::ExprWithAlias { expr, alias } => (expr, Some(alias)),
            _ => {
                return Err(
                    "offline contract requires explicit scalar projection columns.\n       fix: replace wildcards/multi-aliases or use a live check."
                        .into(),
                );
            }
        };
        let (relation, column_name) = resolve_column(expression, relations)?;
        let output_name = match alias {
            Some(identifier) => identifier_name(identifier)?,
            None => column_name.clone(),
        };
        let namespace = relation
            .table
            .namespace
            .as_ref()
            .expect("catalog relation has namespace");
        let id = schema_id(
            catalog.dialect,
            namespace,
            SchemaObjectKind::Column,
            column_name.clone(),
            Some(relation.table.clone()),
        );
        let Some(SchemaObject::Column {
            sql_type, nullable, ..
        }) = catalog.objects.get(&id)
        else {
            return Err(format!(
                "column `{}` is absent from the offline catalog.\n       fix: correct the query/migrations or run a live check.",
                column_name.as_str()
            )
            .into());
        };
        let java_name = Name::parse(&lower_camel(output_name.as_str()))?;
        if !java_names.insert(java_name.as_str().to_string()) {
            return Err(format!(
                "query produces duplicate Java field `{}`.\n       fix: assign distinct SQL aliases.",
                java_name.as_str()
            )
            .into());
        }
        columns.push(ColumnContract {
            name: output_name,
            sql_type: sql_type.clone(),
            java_name,
            java_type: java_type(sql_type)?,
            nullable: *nullable || relation.nullable,
            evidence: mapping_evidence(query, column_name.as_str(), sql_type.as_str()),
        });
    }
    Ok(columns)
}

fn resolve_column<'a>(
    expression: &Expr,
    relations: &'a [Relation],
) -> Result<(&'a Relation, SqlName)> {
    match expression {
        Expr::Identifier(identifier) => {
            let column = identifier_name(identifier)?;
            if relations.len() == 1 {
                return Ok((&relations[0], column));
            }
            Err(format!(
                "unqualified column `{}` is ambiguous across joined tables.\n       fix: qualify it with a table alias.",
                column.as_str()
            )
            .into())
        }
        Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
            let lookup = parts[0].value.to_ascii_lowercase();
            let column = identifier_name(&parts[1])?;
            let relation = relations
                .iter()
                .find(|relation| relation.lookup == lookup)
                .ok_or_else(|| {
                    format!(
                        "unknown table alias `{lookup}`.\n       fix: use an alias declared in this query."
                    )
                })?;
            Ok((relation, column))
        }
        _ => Err(
            "offline contract does not infer projected expression types.\n       fix: project a catalog column or use a live check."
                .into(),
        ),
    }
}

fn compile_parameter(parameter: &DeclaredParameter, query: &QueryId) -> Result<ParameterContract> {
    Ok(ParameterContract {
        name: parameter.name.clone(),
        sql_type: parameter.sql_type.clone(),
        java_type: java_type(&parameter.sql_type)?,
        nullable: parameter.nullable,
        evidence: mapping_evidence(query, parameter.name.as_str(), parameter.sql_type.as_str()),
    })
}

fn java_type(sql_type: &SqlTypeName) -> Result<JavaType> {
    let java = match sql_type.as_str() {
        "text" | "varchar" | "bpchar" => "java.lang.String",
        "uuid" => "java.util.UUID",
        "numeric" => "java.math.BigDecimal",
        "int2" | "int4" => "int",
        "int8" => "long",
        "float4" => "float",
        "float8" => "double",
        "bool" => "boolean",
        "date" => "java.time.LocalDate",
        "timestamp" => "java.time.LocalDateTime",
        "timestamptz" => "java.time.Instant",
        other => {
            return Err(format!(
                "SQL type `{other}` has no Java mapping.\n       fix: add an explicit type mapping before generating Java."
            )
            .into());
        }
    };
    JavaType::parse(java)
}

fn lower_camel(sql: &str) -> String {
    let mut words = sql.split('_');
    let mut output = words.next().unwrap_or_default().to_string();
    for word in words {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            output.push(first.to_ascii_uppercase());
            output.extend(chars);
        }
    }
    output
}

fn mapping_evidence(query: &QueryId, name: &str, sql_type: &str) -> ObjectId {
    digest(
        "JAILS-SQL-MAPPING-1",
        format!(
            "{}.{}:{name}:{sql_type}",
            query.slice.as_str(),
            query.name.as_str()
        )
        .as_bytes(),
    )
}

fn digest(domain: &str, bytes: &[u8]) -> ObjectId {
    ObjectId::from_bytes(domain_hash(domain, bytes))
}
