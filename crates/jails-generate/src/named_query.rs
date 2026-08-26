//! Java projection of an already verified named-query contract.
//!
//! This module never parses SQL or infers a type. Reader SQL comes from
//! `QuerySource`; every Java shape comes from `QueryContractV1`.

use crate::model::Artifact;
use jails_protocol::database::{
    Cardinality, ColumnContract, EvidenceLevel, ParameterContract, QueryContractV1, QuerySource,
};
use jails_protocol::identity::{JavaType, Package};
use jails_support::Result;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedQueryPackages {
    pub application_query: Package,
    pub jdbc_adapter: Package,
    pub fake_adapter: Package,
}

pub fn project(
    source: &QuerySource,
    contract: &QueryContractV1,
    packages: &NamedQueryPackages,
) -> Result<Vec<Artifact>> {
    validate(source, contract)?;
    let name = source.id.name.as_str();
    let port = port_java(source, contract, packages);
    let jdbc = jdbc_java(source, contract, packages)?;
    let fake = fake_java(source, contract, packages);
    let test = contract_test_java(source, contract, packages);
    Ok(vec![
        java_file(
            false,
            &packages.application_query,
            &format!("{name}.java"),
            port,
        ),
        java_file(
            false,
            &packages.jdbc_adapter,
            &format!("Jdbc{name}.java"),
            jdbc,
        ),
        java_file(
            true,
            &packages.fake_adapter,
            &format!("Fake{name}.java"),
            fake,
        ),
        java_file(
            true,
            &packages.fake_adapter,
            &format!("{name}ContractTest.java"),
            test,
        ),
        Artifact {
            kind: "SQL contract",
            path: format!(
                ".jails/sql-contracts/{}/{}.json",
                source.id.slice.as_str().to_ascii_lowercase(),
                kebab(name)
            )
            .into(),
            contents: contract_json(contract)?,
        },
    ])
}

fn validate(source: &QuerySource, contract: &QueryContractV1) -> Result<()> {
    if source.id != contract.id || source.query_digest() != contract.query_digest {
        return Err(
            "query source and verified contract identities differ.\n       fix: run SQL check again before generating Java."
                .into(),
        );
    }
    if !matches!(
        contract.evidence.level,
        EvidenceLevel::VerifiedOffline | EvidenceLevel::VerifiedLive | EvidenceLevel::Executed
    ) {
        return Err(
            "parse-only SQL evidence cannot generate Java.\n       fix: run an offline or live SQL check first."
                .into(),
        );
    }
    if contract.columns.is_empty()
        && !matches!(
            contract.cardinality,
            Cardinality::Exec | Cardinality::ExecRows
        )
    {
        return Err(
            "row-returning query contract has no result columns.\n       fix: verify the query against a catalog before generating Java."
                .into(),
        );
    }
    Ok(())
}

fn port_java(
    source: &QuerySource,
    contract: &QueryContractV1,
    packages: &NamedQueryPackages,
) -> String {
    let name = source.id.name.as_str();
    let mut imports = imports(contract);
    if contract
        .parameters
        .iter()
        .any(|parameter| !parameter.nullable && !is_primitive(&parameter.java_type))
    {
        imports.insert("java.util.Objects".to_string());
    }
    let imports = render_imports(&imports);
    let params = params_record(&contract.parameters);
    let row = row_record(&contract.columns);
    let argument = if contract.parameters.is_empty() {
        ""
    } else {
        "Params params"
    };
    format!(
        "package {};\n\n{}public interface {name} {{\n    {} execute({argument});\n{}{} }}\n",
        packages.application_query.as_str(),
        imports,
        return_type(contract),
        params,
        row
    )
    .replace("\n }\n", "\n}\n")
}

fn params_record(parameters: &[ParameterContract]) -> String {
    if parameters.is_empty() {
        return String::new();
    }
    let fields = parameters
        .iter()
        .map(|parameter| {
            format!(
                "{} {}",
                component_type(&parameter.java_type, parameter.nullable),
                parameter.name.as_str()
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let guards = parameters
        .iter()
        .filter(|parameter| !parameter.nullable && !is_primitive(&parameter.java_type))
        .map(|parameter| {
            format!(
                "            Objects.requireNonNull({}, \"{}\");\n",
                parameter.name.as_str(),
                parameter.name.as_str()
            )
        })
        .collect::<String>();
    if guards.is_empty() {
        format!("\n    record Params({fields}) {{}}\n")
    } else {
        format!(
            "\n    record Params({fields}) {{\n        public Params {{\n{guards}        }}\n    }}\n"
        )
    }
}

fn row_record(columns: &[ColumnContract]) -> String {
    if columns.is_empty() {
        return String::new();
    }
    let fields = columns
        .iter()
        .map(|column| {
            format!(
                "{} {}",
                component_type(&column.java_type, column.nullable),
                column.java_name.as_str()
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("\n    record Row({fields}) {{}}\n")
}

fn jdbc_java(
    source: &QuerySource,
    contract: &QueryContractV1,
    packages: &NamedQueryPackages,
) -> Result<String> {
    let name = source.id.name.as_str();
    let mut imports = BTreeSet::from([
        format!("{}.{}", packages.application_query.as_str(), name),
        "org.springframework.jdbc.core.RowMapper".to_string(),
        "org.springframework.jdbc.core.simple.JdbcClient".to_string(),
        "org.springframework.stereotype.Repository".to_string(),
    ]);
    if contract
        .columns
        .iter()
        .any(|column| column.java_type.to_string() == "java.time.Instant")
    {
        imports.insert("java.time.OffsetDateTime".to_string());
    }
    let imports = render_imports(&imports);
    let mapper = if contract.columns.is_empty() {
        String::new()
    } else {
        let fields = contract
            .columns
            .iter()
            .map(row_read)
            .collect::<Result<Vec<_>>>()?
            .join(",\n            ");
        format!(
            "\n    private static final RowMapper<Row> ROW_MAPPER = (result, rowNumber) -> new Row(\n            {fields});\n"
        )
    };
    let binds = contract
        .parameters
        .iter()
        .map(|parameter| {
            format!(
                "\n                .param(\"{}\", params.{}())",
                parameter.name.as_str(),
                parameter.name.as_str()
            )
        })
        .collect::<String>();
    let terminal = jdbc_terminal(contract.cardinality);
    let argument = if contract.parameters.is_empty() {
        ""
    } else {
        "Params params"
    };
    let sql = &source.sql;
    Ok(format!(
        "package {};\n\n{}@Repository\npublic final class Jdbc{name} implements {name} {{\n    private static final String SQL = \"\"\"\n{sql}\"\"\";\n{mapper}\n    private final JdbcClient jdbc;\n\n    public Jdbc{name}(JdbcClient jdbc) {{\n        this.jdbc = jdbc;\n    }}\n\n    @Override\n    public {} execute({argument}) {{\n        {}jdbc.sql(SQL){binds}{terminal}\n    }}\n}}\n",
        packages.jdbc_adapter.as_str(),
        imports,
        return_type(contract),
        if contract.cardinality == Cardinality::Exec {
            ""
        } else {
            "return "
        }
    ))
}

fn jdbc_terminal(cardinality: Cardinality) -> &'static str {
    match cardinality {
        Cardinality::One => "\n                .query(ROW_MAPPER)\n                .single();",
        Cardinality::Optional => {
            "\n                .query(ROW_MAPPER)\n                .optional();"
        }
        Cardinality::Many => "\n                .query(ROW_MAPPER)\n                .list();",
        Cardinality::Exec => "\n                .update();",
        Cardinality::ExecRows => "\n                .update();",
    }
}

fn row_read(column: &ColumnContract) -> Result<String> {
    let name = column.name.as_str();
    let java = column.java_type.to_string();
    let expression = match java.as_str() {
        "java.lang.String" => format!("result.getString(\"{name}\")"),
        "java.util.UUID" => format!("result.getObject(\"{name}\", java.util.UUID.class)"),
        "java.math.BigDecimal" => format!("result.getBigDecimal(\"{name}\")"),
        "int" if column.nullable => format!("result.getObject(\"{name}\", Integer.class)"),
        "int" => format!("result.getInt(\"{name}\")"),
        "long" if column.nullable => format!("result.getObject(\"{name}\", Long.class)"),
        "long" => format!("result.getLong(\"{name}\")"),
        "float" if column.nullable => format!("result.getObject(\"{name}\", Float.class)"),
        "float" => format!("result.getFloat(\"{name}\")"),
        "double" if column.nullable => format!("result.getObject(\"{name}\", Double.class)"),
        "double" => format!("result.getDouble(\"{name}\")"),
        "boolean" if column.nullable => {
            format!("result.getObject(\"{name}\", Boolean.class)")
        }
        "boolean" => format!("result.getBoolean(\"{name}\")"),
        "java.time.LocalDate" => {
            format!("result.getObject(\"{name}\", java.time.LocalDate.class)")
        }
        "java.time.LocalDateTime" => {
            format!("result.getObject(\"{name}\", java.time.LocalDateTime.class)")
        }
        "java.time.Instant" if column.nullable => format!(
            "java.util.Optional.ofNullable(result.getObject(\"{name}\", OffsetDateTime.class)).map(OffsetDateTime::toInstant).orElse(null)"
        ),
        "java.time.Instant" => {
            format!("result.getObject(\"{name}\", OffsetDateTime.class).toInstant()")
        }
        other => {
            return Err(format!(
                "Java type `{other}` has no explicit JDBC row mapping.\n       fix: add an explicit mapping before generating Java."
            )
            .into());
        }
    };
    Ok(expression)
}

fn fake_java(
    source: &QuerySource,
    contract: &QueryContractV1,
    packages: &NamedQueryPackages,
) -> String {
    let name = source.id.name.as_str();
    let argument = if contract.parameters.is_empty() {
        ""
    } else {
        "Params params"
    };
    let behavior_type = match (contract.parameters.is_empty(), contract.cardinality) {
        (true, Cardinality::Exec) => "Runnable".to_string(),
        (true, _) => format!("java.util.function.Supplier<{}>", return_type(contract)),
        (false, Cardinality::Exec) => "java.util.function.Consumer<Params>".to_string(),
        (false, _) => format!(
            "java.util.function.Function<Params, {}>",
            return_type(contract)
        ),
    };
    let call = match (contract.parameters.is_empty(), contract.cardinality) {
        (true, Cardinality::Exec) => "behavior.run()",
        (true, _) => "behavior.get()",
        (false, Cardinality::Exec) => "behavior.accept(params)",
        (false, _) => "behavior.apply(params)",
    };
    let imports = render_imports(&imports(contract));
    format!(
        "package {};\n\nimport {}.{};\nimport java.util.Objects;\n{}public final class Fake{name} implements {name} {{\n    private final {behavior_type} behavior;\n\n    public Fake{name}({behavior_type} behavior) {{\n        this.behavior = Objects.requireNonNull(behavior, \"behavior\");\n    }}\n\n    @Override\n    public {} execute({argument}) {{\n        {}{call};\n    }}\n}}\n",
        packages.fake_adapter.as_str(),
        packages.application_query.as_str(),
        name,
        imports,
        return_type(contract),
        if contract.cardinality == Cardinality::Exec {
            ""
        } else {
            "return "
        }
    )
}

fn contract_test_java(
    source: &QuerySource,
    contract: &QueryContractV1,
    packages: &NamedQueryPackages,
) -> String {
    let name = source.id.name.as_str();
    let has_parameters = !contract.parameters.is_empty();
    let lambda_arg = if has_parameters {
        "params -> "
    } else {
        "() -> "
    };
    let (value, assertion) = match contract.cardinality {
        Cardinality::Many => ("java.util.List.of()", "assertTrue(result.isEmpty());"),
        Cardinality::Optional => (
            "java.util.Optional.empty()",
            "assertTrue(result.isEmpty());",
        ),
        Cardinality::One => ("null", "assertNull(result);"),
        Cardinality::Exec => (
            "{}",
            if has_parameters {
                "assertDoesNotThrow(() -> fake.execute(params));"
            } else {
                "assertDoesNotThrow(fake::execute);"
            },
        ),
        Cardinality::ExecRows => ("0", "assertEquals(0, result);"),
    };
    let behavior = format!("{lambda_arg}{value}");
    let parameter_values = contract
        .parameters
        .iter()
        .map(sample_value)
        .collect::<Vec<_>>()
        .join(", ");
    let setup = if contract.parameters.is_empty() {
        String::new()
    } else {
        format!("        var params = new {name}.Params({parameter_values});\n")
    };
    let execute = if contract.cardinality == Cardinality::Exec {
        String::new()
    } else if contract.parameters.is_empty() {
        "        var result = fake.execute();\n".to_string()
    } else {
        "        var result = fake.execute(params);\n".to_string()
    };
    format!(
        "package {};\n\nimport static org.junit.jupiter.api.Assertions.*;\n\nimport {}.{};\nimport org.junit.jupiter.api.Test;\n\nfinal class {name}ContractTest {{\n    @Test\n    void fake_obeys_the_port_boundary() {{\n        var fake = new Fake{name}({behavior});\n{setup}{execute}        {assertion}\n    }}\n}}\n",
        packages.fake_adapter.as_str(),
        packages.application_query.as_str(),
        name
    )
}

fn sample_value(parameter: &ParameterContract) -> String {
    if parameter.nullable {
        return "null".to_string();
    }
    match parameter.java_type.to_string().as_str() {
        "java.lang.String" => "\"value\"".to_string(),
        "java.util.UUID" => "java.util.UUID.randomUUID()".to_string(),
        "java.math.BigDecimal" => "java.math.BigDecimal.ZERO".to_string(),
        "int" => "1".to_string(),
        "long" => "1L".to_string(),
        "float" => "1.0f".to_string(),
        "double" => "1.0d".to_string(),
        "boolean" => "false".to_string(),
        "java.time.LocalDate" => "java.time.LocalDate.EPOCH".to_string(),
        "java.time.LocalDateTime" => "java.time.LocalDateTime.MIN".to_string(),
        "java.time.Instant" => "java.time.Instant.EPOCH".to_string(),
        _ => "null".to_string(),
    }
}

fn imports(contract: &QueryContractV1) -> BTreeSet<String> {
    let mut imports = BTreeSet::new();
    for java_type in contract
        .parameters
        .iter()
        .map(|field| &field.java_type)
        .chain(contract.columns.iter().map(|field| &field.java_type))
    {
        let qualified = java_type.to_string();
        if qualified.contains('.') && !qualified.starts_with("java.lang.") {
            imports.insert(qualified);
        }
    }
    match contract.cardinality {
        Cardinality::Many => {
            imports.insert("java.util.List".to_string());
        }
        Cardinality::Optional => {
            imports.insert("java.util.Optional".to_string());
        }
        _ => {}
    }
    imports
}

fn render_imports(imports: &BTreeSet<String>) -> String {
    if imports.is_empty() {
        return String::new();
    }
    format!(
        "{}\n\n",
        imports
            .iter()
            .map(|import| format!("import {import};"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn return_type(contract: &QueryContractV1) -> String {
    match contract.cardinality {
        Cardinality::One => "Row".to_string(),
        Cardinality::Optional => "Optional<Row>".to_string(),
        Cardinality::Many => "List<Row>".to_string(),
        Cardinality::Exec => "void".to_string(),
        Cardinality::ExecRows => "int".to_string(),
    }
}

fn component_type(java_type: &JavaType, nullable: bool) -> String {
    if nullable {
        return boxed(java_type);
    }
    java_type.name().as_str().to_string()
}

fn boxed(java_type: &JavaType) -> String {
    match java_type.to_string().as_str() {
        "boolean" => "Boolean".to_string(),
        "byte" => "Byte".to_string(),
        "char" => "Character".to_string(),
        "double" => "Double".to_string(),
        "float" => "Float".to_string(),
        "int" => "Integer".to_string(),
        "long" => "Long".to_string(),
        "short" => "Short".to_string(),
        _ => java_type.name().as_str().to_string(),
    }
}

fn is_primitive(java_type: &JavaType) -> bool {
    !java_type.to_string().contains('.')
}

fn java_file(test: bool, package: &Package, name: &str, contents: String) -> Artifact {
    Artifact {
        kind: "named query Java",
        path: std::path::PathBuf::from(if test {
            "src/test/java"
        } else {
            "src/main/java"
        })
        .join(package.as_str().replace('.', "/"))
        .join(name),
        contents,
    }
}

fn contract_json(contract: &QueryContractV1) -> Result<String> {
    let parameters = contract
        .parameters
        .iter()
        .map(|parameter| {
            format!(
                "    {{\"name\":\"{}\",\"sql_type\":\"{}\",\"java_type\":\"{}\",\"nullable\":{}}}",
                parameter.name.as_str(),
                parameter.sql_type.as_str(),
                parameter.java_type,
                parameter.nullable
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let columns = contract
        .columns
        .iter()
        .map(|column| {
            format!(
                "    {{\"name\":\"{}\",\"java_name\":\"{}\",\"sql_type\":\"{}\",\"java_type\":\"{}\",\"nullable\":{}}}",
                column.name.as_str(),
                column.java_name.as_str(),
                column.sql_type.as_str(),
                column.java_type,
                column.nullable
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    Ok(format!(
        "{{\n  \"schema_version\": 1,\n  \"id\": {{\"slice\":\"{}\",\"name\":\"{}\"}},\n  \"dialect\": \"{}\",\n  \"query_digest\": \"{}\",\n  \"catalog_digest\": \"{}\",\n  \"cardinality\": \"{}\",\n  \"parameters\": [\n{}\n  ],\n  \"columns\": [\n{}\n  ],\n  \"evidence\": {{\"level\":\"{}\",\"input_digest\":\"{}\",\"catalog_digest\":\"{}\",\"toolchain_digest\":\"{}\",\"details_digest\":\"{}\"}}\n}}\n",
        contract.id.slice.as_str(),
        contract.id.name.as_str(),
        contract.dialect.label(),
        contract.query_digest,
        contract.catalog_digest,
        contract.cardinality.label(),
        parameters,
        columns,
        evidence_label(contract.evidence.level),
        contract.evidence.input_digest,
        contract.evidence.catalog_digest.ok_or(
            "verified SQL evidence has no catalog digest.\n       fix: run SQL check again.",
        )?,
        contract.evidence.toolchain_digest,
        contract.evidence.details_digest
    ))
}

fn evidence_label(level: EvidenceLevel) -> &'static str {
    match level {
        EvidenceLevel::Parsed => "parsed",
        EvidenceLevel::VerifiedOffline => "verified-offline",
        EvidenceLevel::VerifiedLive => "verified-live",
        EvidenceLevel::Executed => "executed",
    }
}

fn kebab(name: &str) -> String {
    let mut output = String::new();
    for (index, character) in name.chars().enumerate() {
        if index > 0 && character.is_ascii_uppercase() {
            output.push('-');
        }
        output.push(character.to_ascii_lowercase());
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_project::query_compiler::{compile_catalog, compile_query, parse_query_file};
    use jails_protocol::database::SqlDialect;
    use jails_protocol::identity::ProjectPath;
    use std::fs;
    use std::process::Command;

    fn fixture() -> (QuerySource, QueryContractV1, NamedQueryPackages) {
        let catalog = compile_catalog(
            SqlDialect::PostgreSql,
            &[(
                ProjectPath::parse("src/main/resources/db/migration/V001__entries.sql").unwrap(),
                "CREATE TABLE entries (id uuid PRIMARY KEY, group_id uuid NOT NULL, amount numeric NOT NULL, state text NOT NULL, created_at timestamptz NOT NULL);".to_string(),
            )],
        )
        .unwrap();
        let source = parse_query_file(
            "Example",
            "src/main/resources/db/queries/FindEntries.sql",
            "-- jails:name FindEntries\n-- jails:cardinality many\n-- jails:param state text\n-- jails:param minimum numeric\n-- jails:param limit int4\nSELECT id, group_id, amount, state, created_at\nFROM entries\nWHERE state = :state AND amount >= :minimum\nORDER BY created_at, id\nLIMIT :limit;\n",
            SqlDialect::PostgreSql,
        )
        .unwrap();
        let contract = compile_query(&source, &catalog).unwrap();
        let packages = NamedQueryPackages {
            application_query: Package::parse("org.example.sample.application.query").unwrap(),
            jdbc_adapter: Package::parse("org.example.sample.adapter.jdbc").unwrap(),
            fake_adapter: Package::parse("org.example.sample.adapter.query").unwrap(),
        };
        (source, contract, packages)
    }

    #[test]
    fn verified_contract_projects_port_jdbc_fake_test_and_json() {
        let (source, contract, packages) = fixture();
        let files = project(&source, &contract, &packages).unwrap();
        assert_eq!(files.len(), 5);
        let port = &files[0].contents;
        assert!(port.contains("public interface FindEntries"));
        assert!(port.contains("record Params(String state, BigDecimal minimum, int limit)"));
        assert!(port.contains("record Row(UUID id, UUID groupId"));
        let jdbc = &files[1].contents;
        assert!(jdbc.contains("private static final String SQL = \"\"\""));
        assert!(jdbc.contains(&source.sql));
        assert!(jdbc.contains("private static final RowMapper<Row> ROW_MAPPER"));
        assert!(jdbc.contains(".param(\"minimum\", params.minimum())"));
        assert!(
            files[2]
                .contents
                .contains("java.util.function.Function<Params, List<Row>>")
        );
        assert!(files[3].contents.contains("fake_obeys_the_port_boundary"));
        assert!(files[4].contents.contains("\"schema_version\": 1"));
        assert!(
            files[4]
                .contents
                .contains(&contract.query_digest.to_string())
        );
    }

    #[test]
    fn source_and_contract_drift_refuses_generation() {
        let (mut source, contract, packages) = fixture();
        source.sql.push_str("-- changed\n");
        let error = project(&source, &contract, &packages).unwrap_err();
        assert!(error.to_string().contains("identities differ"));
        assert!(error.to_string().contains("fix:"));
    }

    #[test]
    fn generated_port_and_fake_compile_with_javac() {
        let (source, contract, packages) = fixture();
        let files = project(&source, &contract, &packages).unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let classes = scratch.path().join("classes");
        fs::create_dir_all(&classes).unwrap();
        let mut sources = Vec::new();
        for file in [&files[0], &files[2]] {
            let path = scratch.path().join(&file.path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, &file.contents).unwrap();
            sources.push(path);
        }
        let output = Command::new("javac")
            .arg("-d")
            .arg(classes)
            .args(&sources)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
