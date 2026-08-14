//! `jails add <capability>` -- grow an existing project one capability at a
//! time.
//!
//! Where `generate` emits a *class*, `add` emits a *slice*: the dependency,
//! the code that uses it, and a test that proves the wiring compiles and
//! runs. Every capability is idempotent (re-running reports what is already
//! there) and takes no required arguments -- the library, the version, the
//! package and the class names all have opinionated defaults.
//!
//! `Capability` is a `clap::ValueEnum` rather than a `String` on purpose:
//! that is the only way `clap_complete` can emit a static completion list for
//! `jails add <TAB>`, and the doc comment on each variant becomes its
//! completion description.

use crate::generate::{base_package, find_project_root, main_dir, test_dir, write_new_file};
use crate::pom::{self, Dependency, Flavor, TARGET_RELEASE};
use crate::Result;
use clap::ValueEnum;
use std::path::PathBuf;

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Capability {
    /// Read CSV files into records (Apache Commons CSV)
    Csv,
    /// SQLite persistence: JDBC connections and a migration runner (sqlite-jdbc)
    Sqlite,
    /// Read and write JSON (Jackson databind)
    Json,
}

impl Capability {
    fn label(self) -> &'static str {
        match self {
            Capability::Csv => "csv",
            Capability::Sqlite => "sqlite",
            Capability::Json => "json",
        }
    }
}

/// A file a capability wants to create.
struct NewFile {
    path: PathBuf,
    contents: String,
}

/// Everything a capability wants to do to the project, computed before
/// anything is written so `--dry-run` can describe it without side effects.
struct Plan {
    deps: Vec<Dependency>,
    files: Vec<NewFile>,
}

pub fn add(capability: Capability, name: Option<&str>, dry_run: bool) -> Result<()> {
    let root = find_project_root()?;
    let pom_text = pom::read(&root)?;
    let flavor = pom::flavor(&pom_text);

    // Emitting records and pattern-matching switches into a project pinned at
    // an older release produces code that cannot compile, so fail with
    // something actionable instead.
    let target: u32 = TARGET_RELEASE.parse().expect("TARGET_RELEASE is numeric");
    match pom::release_level(&pom_text) {
        Some(level) if level < target => {
            return Err(format!(
                "this project targets Java {level}, but jails generates Java {TARGET_RELEASE} code.\n       \
                 Raise <maven.compiler.release> (or <java.version>) to {TARGET_RELEASE} in pom.xml and try again."
            ));
        }
        None => {
            return Err(format!(
                "pom.xml does not set a Java release level, and jails generates Java {TARGET_RELEASE} code.\n       \
                 Add <maven.compiler.release>{TARGET_RELEASE}</maven.compiler.release> to <properties> and try again."
            ));
        }
        Some(_) => {}
    }

    let pkg = base_package(&root)?;
    let plan = match capability {
        Capability::Csv => csv_plan(&root, &pkg, flavor, name)?,
        Capability::Sqlite => sqlite_plan(&root, &pkg, flavor, name)?,
        Capability::Json => json_plan(&root, &pkg, flavor, name)?,
    };

    // Work out the pom edit up front, so a dry run can say whether the
    // dependency would be added or is already there.
    let mut updated_pom = pom_text.clone();
    let mut spliced: Vec<&Dependency> = Vec::new();
    for dep in &plan.deps {
        match pom::add_dependency(&updated_pom, dep)? {
            Some(next) => {
                updated_pom = next;
                spliced.push(dep);
            }
            None => println!("  exists  {}:{}", dep.group_id, dep.artifact_id),
        }
    }

    if dry_run {
        for dep in &spliced {
            println!("  would add dependency  {}:{}", dep.group_id, dep.artifact_id);
        }
        for file in &plan.files {
            let verb = if file.path.exists() { "would skip (exists)" } else { "would create" };
            println!("  {verb}  {}", rel(&root, &file.path));
        }
        return Ok(());
    }

    if !spliced.is_empty() {
        std::fs::write(root.join("pom.xml"), &updated_pom)
            .map_err(|e| format!("failed to write pom.xml: {e}"))?;
        for dep in &spliced {
            println!("     dep  {}:{}", dep.group_id, dep.artifact_id);
        }
    }

    let mut created = 0;
    for file in &plan.files {
        if file.path.exists() {
            println!("  exists  {}", rel(&root, &file.path));
            continue;
        }
        write_new_file(&file.path, &file.contents)?;
        println!("  create  {}", rel(&root, &file.path));
        created += 1;
    }

    if created == 0 && spliced.is_empty() {
        println!("{} is already set up -- nothing to do", capability.label());
        return Ok(());
    }

    println!(
        "added {} ({})",
        capability.label(),
        match flavor {
            Flavor::SpringBoot => "spring boot",
            Flavor::PlainMaven => "plain maven",
        }
    );
    Ok(())
}

fn rel(root: &std::path::Path, path: &std::path::Path) -> String {
    path.strip_prefix(root).unwrap_or(path).display().to_string()
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// csv
// ---------------------------------------------------------------------------

/// Commons CSV renamed `Builder.build()` to `Builder.get()` in 1.13, so the
/// pinned version and the generated call have to move together.
const COMMONS_CSV: Dependency = Dependency {
    group_id: "org.apache.commons",
    artifact_id: "commons-csv",
    version: Some("1.14.1"),
    scope: None,
};

fn csv_plan(root: &std::path::Path, pkg: &str, _flavor: Flavor, name: Option<&str>) -> Result<Plan> {
    let base = capitalize(name.unwrap_or("Csv"));
    let class = format!("{base}Reader");

    Ok(Plan {
        // Spring Boot's dependency management does not cover commons-csv, so
        // the version is pinned in both flavors.
        deps: vec![COMMONS_CSV],
        files: vec![
            NewFile {
                path: main_dir(root, pkg).join(format!("{class}.java")),
                contents: csv_reader_java(pkg, &class),
            },
            NewFile {
                path: test_dir(root, pkg).join(format!("{class}Test.java")),
                contents: csv_reader_test_java(pkg, &class),
            },
        ],
    })
}

fn csv_reader_java(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;
import org.apache.commons.csv.CSVFormat;

/**
 * Reads a CSV file with a header row into {{@link Row}} values.
 *
 * <p>Parsing is delegated to Commons CSV so quoted fields, embedded commas
 * and embedded newlines are handled correctly.
 */
public final class {class} {{

    private {class}() {{}}

    /** One CSV record: column name to value. */
    public record Row(Map<String, String> values) {{

        public Row {{
            values = Map.copyOf(values);
        }}

        /** Value of {{@code column}}, or a clear failure if it is not in the header. */
        public String get(String column) {{
            var value = values.get(column);
            if (value == null) {{
                throw new IllegalArgumentException("no column named '" + column + "' in " + values.keySet());
            }}
            return value;
        }}

        public int getInt(String column) {{
            return Integer.parseInt(get(column));
        }}
    }}

    /** Reads every row of {{@code path}}, treating the first line as the header. */
    public static List<Row> read(Path path) throws IOException {{
        var format = CSVFormat.DEFAULT.builder()
                .setHeader()
                .setSkipHeaderRecord(true)
                .setTrim(true)
                .get();
        try (var reader = Files.newBufferedReader(path);
                var parser = format.parse(reader)) {{
            return parser.stream().map(record -> new Row(record.toMap())).toList();
        }} catch (UncheckedIOException e) {{
            throw e.getCause();
        }}
    }}
}}
"#
    )
}

fn csv_reader_test_java(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class {class}Test {{

    @TempDir
    Path tmp;

    private Path csv(String contents) throws Exception {{
        var path = tmp.resolve("rows.csv");
        Files.writeString(path, contents);
        return path;
    }}

    @Test
    void readsRowsKeyedByHeader() throws Exception {{
        var rows = {class}.read(csv("name,qty\nbolt,7\n"));

        assertEquals(1, rows.size());
        assertEquals("bolt", rows.getFirst().get("name"));
        assertEquals(7, rows.getFirst().getInt("qty"));
    }}

    @Test
    void keepsCommasInsideQuotedFields() throws Exception {{
        var rows = {class}.read(csv("name,qty\n\"widget, large\",3\n"));

        assertEquals("widget, large", rows.getFirst().get("name"));
    }}

    @Test
    void readsAnEmptyFileAsNoRows() throws Exception {{
        assertEquals(List.of(), {class}.read(csv("name,qty\n")));
    }}

    @Test
    void namesTheColumnWhenItIsMissing() throws Exception {{
        var rows = {class}.read(csv("name,qty\nbolt,7\n"));

        var error = assertThrows(IllegalArgumentException.class, () -> rows.getFirst().get("price"));
        assertEquals(true, error.getMessage().contains("price"));
    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// sqlite
// ---------------------------------------------------------------------------

const SQLITE_JDBC: Dependency = Dependency {
    group_id: "org.xerial",
    artifact_id: "sqlite-jdbc",
    version: Some("3.49.1.0"),
    scope: None,
};

/// Deliberately the same code in both flavors. `java.sql` is part of the
/// standard library, so a plain JDBC connection plus a migration runner needs
/// nothing beyond the driver -- no ORM, no Flyway, and none of the fiddliness
/// of getting SQLite to work under JPA/Hibernate dialects. A Spring project
/// can inject the record wherever it needs a connection.
fn sqlite_plan(root: &std::path::Path, pkg: &str, _flavor: Flavor, name: Option<&str>) -> Result<Plan> {
    let base = name.map(capitalize).unwrap_or_default();
    let database = format!("{base}Database");
    let migrations = format!("{base}Migrations");

    Ok(Plan {
        deps: vec![SQLITE_JDBC],
        files: vec![
            NewFile {
                path: main_dir(root, pkg).join(format!("{database}.java")),
                contents: database_java(pkg, &database),
            },
            NewFile {
                path: main_dir(root, pkg).join(format!("{migrations}.java")),
                contents: migrations_java(pkg, &migrations),
            },
            NewFile {
                path: root.join("src/main/resources/db/migration/001_init.sql"),
                contents: FIRST_MIGRATION.to_string(),
            },
            NewFile {
                path: test_dir(root, pkg).join(format!("{database}Test.java")),
                contents: database_test_java(pkg, &database, &migrations),
            },
        ],
    })
}

const FIRST_MIGRATION: &str = "-- Applied once, in filename order, by Migrations.applyAll.
create table if not exists item (
    id integer primary key autoincrement,
    name text not null,
    qty integer not null default 0
);
";

fn database_java(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import java.nio.file.Path;
import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.SQLException;

/**
 * A SQLite database file. Connections come from {{@code java.sql}} -- the only
 * thing the driver dependency adds is the {{@code jdbc:sqlite:}} URL scheme.
 *
 * <p>Callers own the {{@link Connection}} and should use try-with-resources.
 */
public record {class}(Path file) {{

    /**
     * A database that lives only for as long as the connection does. Each
     * {{@link #open()}} returns a *fresh, empty* in-memory database, which is
     * what makes it convenient for isolated tests.
     */
    public static {class} inMemory() {{
        return new {class}(Path.of(":memory:"));
    }}

    public Connection open() throws SQLException {{
        return DriverManager.getConnection("jdbc:sqlite:" + file);
    }}
}}
"#
    )
}

fn migrations_java(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.sql.Connection;
import java.sql.SQLException;
import java.util.ArrayList;
import java.util.List;

/**
 * Applies {{@code .sql}} files in filename order, exactly once each.
 *
 * <p>Applied scripts are recorded in a {{@code schema_migrations}} table, so
 * running this on every startup is safe: only new files do any work.
 */
public final class {class} {{

    private static final String CREATE_TRACKING_TABLE =
            """
            create table if not exists schema_migrations (
                name text primary key,
                applied_at text not null default (datetime('now'))
            )
            """;

    private {class}() {{}}

    /**
     * Applies every not-yet-applied script in {{@code dir}}, returning the names
     * of the ones applied. A missing directory means no migrations, not an
     * error.
     */
    public static List<String> applyAll(Connection connection, Path dir) throws IOException, SQLException {{
        try (var statement = connection.createStatement()) {{
            statement.execute(CREATE_TRACKING_TABLE);
        }}

        var applied = new ArrayList<String>();
        for (var script : scripts(dir)) {{
            var name = script.getFileName().toString();
            if (!alreadyApplied(connection, name)) {{
                apply(connection, name, Files.readString(script));
                applied.add(name);
            }}
        }}
        return List.copyOf(applied);
    }}

    private static List<Path> scripts(Path dir) throws IOException {{
        if (!Files.isDirectory(dir)) {{
            return List.of();
        }}
        try (var files = Files.list(dir)) {{
            return files.filter(path -> path.getFileName().toString().endsWith(".sql")).sorted().toList();
        }}
    }}

    private static boolean alreadyApplied(Connection connection, String name) throws SQLException {{
        try (var query = connection.prepareStatement("select 1 from schema_migrations where name = ?")) {{
            query.setString(1, name);
            try (var rows = query.executeQuery()) {{
                return rows.next();
            }}
        }}
    }}

    /** Each script runs in one transaction, together with recording its name. */
    private static void apply(Connection connection, String name, String sql) throws SQLException {{
        var autoCommit = connection.getAutoCommit();
        connection.setAutoCommit(false);
        try {{
            try (var statement = connection.createStatement()) {{
                // Simple splitter: fine for schema DDL, but it would break on a
                // semicolon inside a string literal or a trigger body.
                for (var command : sql.split(";")) {{
                    if (!command.isBlank()) {{
                        statement.execute(command);
                    }}
                }}
            }}
            try (var insert = connection.prepareStatement("insert into schema_migrations(name) values (?)")) {{
                insert.setString(1, name);
                insert.executeUpdate();
            }}
            connection.commit();
        }} catch (SQLException e) {{
            connection.rollback();
            throw e;
        }} finally {{
            connection.setAutoCommit(autoCommit);
        }}
    }}
}}
"#
    )
}

fn database_test_java(pkg: &str, database: &str, migrations: &str) -> String {
    format!(
        r#"package {pkg};

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class {database}Test {{

    @TempDir
    Path tmp;

    private Path migrationDir() throws Exception {{
        var dir = tmp.resolve("migration");
        Files.createDirectories(dir);
        Files.writeString(dir.resolve("001_init.sql"), "create table item (id integer primary key, name text not null);");
        return dir;
    }}

    @Test
    void appliesEachMigrationExactlyOnce() throws Exception {{
        var database = new {database}(tmp.resolve("test.db"));
        var dir = migrationDir();

        try (var connection = database.open()) {{
            assertEquals(List.of("001_init.sql"), {migrations}.applyAll(connection, dir));
            assertEquals(List.of(), {migrations}.applyAll(connection, dir), "second run should be a no-op");
        }}
    }}

    @Test
    void storesAndReadsRows() throws Exception {{
        var database = new {database}(tmp.resolve("test.db"));
        var dir = migrationDir();

        try (var connection = database.open()) {{
            {migrations}.applyAll(connection, dir);

            try (var insert = connection.prepareStatement("insert into item(name) values (?)")) {{
                insert.setString(1, "bolt");
                insert.executeUpdate();
            }}
            try (var query = connection.prepareStatement("select name from item");
                    var rows = query.executeQuery()) {{
                assertTrue(rows.next());
                assertEquals("bolt", rows.getString("name"));
            }}
        }}
    }}

    @Test
    void treatsAMissingMigrationDirectoryAsNoMigrations() throws Exception {{
        try (var connection = {database}.inMemory().open()) {{
            assertEquals(List.of(), {migrations}.applyAll(connection, tmp.resolve("nope")));
        }}
    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// json
// ---------------------------------------------------------------------------

const JACKSON: Dependency = Dependency {
    group_id: "com.fasterxml.jackson.core",
    artifact_id: "jackson-databind",
    version: Some("2.19.0"),
    scope: None,
};

fn json_plan(root: &std::path::Path, pkg: &str, flavor: Flavor, name: Option<&str>) -> Result<Plan> {
    let base = name.map(capitalize).unwrap_or_default();
    let class = format!("{base}Json");

    // Spring Boot's dependency management already pins Jackson (and the web
    // starter pulls it in transitively), so declaring a version here would
    // fight the parent pom.
    let dep = match flavor {
        Flavor::SpringBoot => Dependency { version: None, ..JACKSON },
        Flavor::PlainMaven => JACKSON,
    };

    Ok(Plan {
        deps: vec![dep],
        files: vec![
            NewFile {
                path: main_dir(root, pkg).join(format!("{class}.java")),
                contents: json_java(pkg, &class),
            },
            NewFile {
                path: test_dir(root, pkg).join(format!("{class}Test.java")),
                contents: json_test_java(pkg, &class),
            },
        ],
    })
}

fn json_java(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import com.fasterxml.jackson.core.JsonProcessingException;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.SerializationFeature;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

/**
 * JSON reading and writing over one shared, thread-safe {{@link ObjectMapper}}.
 *
 * <p>Records map to JSON objects without any annotations, and
 * {{@code findAndRegisterModules()}} picks up whatever Jackson modules are on
 * the classpath (java.time support, for one).
 */
public final class {class} {{

    private static final ObjectMapper MAPPER = new ObjectMapper()
            .findAndRegisterModules()
            .disable(SerializationFeature.WRITE_DATES_AS_TIMESTAMPS);

    private {class}() {{}}

    public static <T> T read(Path path, Class<T> type) throws IOException {{
        try (var in = Files.newInputStream(path)) {{
            return MAPPER.readValue(in, type);
        }}
    }}

    /** Reads a top-level JSON array into a list of {{@code element}}. */
    public static <T> List<T> readList(Path path, Class<T> element) throws IOException {{
        var listType = MAPPER.getTypeFactory().constructCollectionType(List.class, element);
        try (var in = Files.newInputStream(path)) {{
            return MAPPER.readValue(in, listType);
        }}
    }}

    /** Writes {{@code value}} as indented JSON, replacing any existing file. */
    public static void write(Path path, Object value) throws IOException {{
        try (var out = Files.newOutputStream(path)) {{
            MAPPER.writerWithDefaultPrettyPrinter().writeValue(out, value);
        }}
    }}

    public static String toJson(Object value) throws JsonProcessingException {{
        return MAPPER.writeValueAsString(value);
    }}

    public static <T> T parse(String json, Class<T> type) throws JsonProcessingException {{
        return MAPPER.readValue(json, type);
    }}
}}
"#
    )
}

fn json_test_java(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class {class}Test {{

    /** Records need no annotations to round-trip. */
    record Item(String name, int qty) {{}}

    @TempDir
    Path tmp;

    @Test
    void roundTripsARecordThroughAFile() throws Exception {{
        var path = tmp.resolve("item.json");
        {class}.write(path, new Item("bolt", 7));

        assertEquals(new Item("bolt", 7), {class}.read(path, Item.class));
    }}

    @Test
    void readsAJsonArrayAsAList() throws Exception {{
        var path = tmp.resolve("items.json");
        Files.writeString(path, "[{{\"name\":\"bolt\",\"qty\":7}},{{\"name\":\"nut\",\"qty\":3}}]");

        assertEquals(List.of(new Item("bolt", 7), new Item("nut", 3)), {class}.readList(path, Item.class));
    }}

    @Test
    void roundTripsThroughAString() throws Exception {{
        assertEquals(new Item("bolt", 7), {class}.parse({class}.toJson(new Item("bolt", 7)), Item.class));
    }}
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_reader_uses_the_builder_api_matching_the_pinned_version() {
        // 1.13 renamed build() to get(); emitting the wrong one is a compile
        // error that only shows up in the real-toolchain tests.
        let src = csv_reader_java("com.example.demo", "CsvReader");
        assert_eq!(COMMONS_CSV.version, Some("1.14.1"));
        assert!(src.contains(".get();"));
        assert!(!src.contains(".build();"));
    }

    #[test]
    fn csv_reader_is_generated_into_the_projects_package() {
        let src = csv_reader_java("com.example.demo", "CsvReader");
        assert!(src.starts_with("package com.example.demo;\n"));
        assert!(src.contains("public final class CsvReader"));
    }

    #[test]
    fn csv_reader_uses_modern_java_idioms() {
        let src = csv_reader_java("com.example.demo", "CsvReader");
        assert!(src.contains("public record Row("), "rows should be a record");
        assert!(src.contains(".toList()"), "should use Stream.toList()");
        assert!(src.contains("try (var reader"), "should use try-with-resources");
        assert!(!src.contains("java.io.File"), "should use NIO paths, not File");
    }

    #[test]
    fn csv_name_override_renames_the_class_and_its_test() {
        let src = csv_reader_java("com.example.demo", "TransactionReader");
        assert!(src.contains("public final class TransactionReader"));
        let test = csv_reader_test_java("com.example.demo", "TransactionReader");
        assert!(test.contains("class TransactionReaderTest"));
        assert!(test.contains("TransactionReader.read("));
    }

    #[test]
    fn sqlite_uses_stdlib_jdbc_and_no_orm() {
        let db = database_java("com.example.demo", "Database");
        assert!(db.contains("public record Database(Path file)"));
        assert!(db.contains("java.sql.DriverManager"));
        assert!(db.contains("jdbc:sqlite:"));

        let migrations = migrations_java("com.example.demo", "Migrations");
        assert!(migrations.contains("schema_migrations"), "applied scripts must be tracked");
        assert!(migrations.contains("connection.rollback()"), "a failed script must not half-apply");
        assert!(migrations.contains("\"\"\""), "SQL should use a text block");
        // No ORM, no migration framework -- the driver is the only dependency.
        for forbidden in ["hibernate", "flyway", "liquibase", "jakarta.persistence"] {
            assert!(!migrations.contains(forbidden), "{forbidden} should not appear");
            assert!(!db.contains(forbidden), "{forbidden} should not appear");
        }
    }

    #[test]
    fn sqlite_name_override_renames_both_classes_consistently() {
        let db = database_java("com.example.demo", "LedgerDatabase");
        assert!(db.contains("public record LedgerDatabase(Path file)"));
        assert!(db.contains("public static LedgerDatabase inMemory()"));

        let test = database_test_java("com.example.demo", "LedgerDatabase", "LedgerMigrations");
        assert!(test.contains("class LedgerDatabaseTest"));
        assert!(test.contains("LedgerMigrations.applyAll("));
    }

    #[test]
    fn json_pins_a_version_only_when_no_parent_manages_it() {
        // Spring Boot's parent already pins Jackson; declaring our own version
        // would override the curated one.
        assert_eq!(JACKSON.version, Some("2.19.0"));
        let root = std::path::Path::new("/tmp/does-not-matter");
        let spring = json_plan(root, "com.example.demo", Flavor::SpringBoot, None).unwrap();
        assert_eq!(spring.deps[0].version, None);
        let plain = json_plan(root, "com.example.demo", Flavor::PlainMaven, None).unwrap();
        assert_eq!(plain.deps[0].version, Some("2.19.0"));
    }

    #[test]
    fn json_uses_nio_streams_rather_than_file() {
        let src = json_java("com.example.demo", "Json");
        assert!(src.contains("Files.newInputStream"));
        assert!(src.contains("Files.newOutputStream"));
        assert!(!src.contains("java.io.File"), "should not fall back to java.io.File");
        assert!(src.contains("private static final ObjectMapper MAPPER"), "mapper should be shared");
    }

    #[test]
    fn capitalize_uppercases_the_first_letter_only() {
        assert_eq!(capitalize("csv"), "Csv");
        assert_eq!(capitalize("transaction"), "Transaction");
        assert_eq!(capitalize(""), "");
    }
}
