//! `jails add <capability>...` -- grow an existing project with one or more
//! capabilities at a time.
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

use crate::Result;
use crate::generate::{
    base_package, find_project_root, layout, main_dir, subpackage, test_dir, write_new_file,
};
use crate::pom::{self, Dependency, Flavor, MIN_RELEASE, TARGET_RELEASE};
use clap::ValueEnum;
use std::path::PathBuf;

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Capability {
    /// PostgreSQL + Flyway + Testcontainers; raw SQL only, never an ORM
    #[value(alias = "postgres")]
    Db,
    /// Read CSV files into records (Apache Commons CSV)
    Csv,
    /// SQLite persistence: JDBC connections and a migration runner (sqlite-jdbc)
    Sqlite,
    /// Read and write JSON (Jackson databind)
    Json,
    /// Deterministic test helpers: clocks, ids, fixtures, in-process CLI runs
    Testkit,
    /// A scripted test double for any interface, driven by a lambda
    Fake,
    /// An HTTP server on the JDK's own httpserver -- no framework
    Http,
    /// Automatic formatting on `mvn verify` (Spotless + palantir-java-format)
    Format,
}

impl Capability {
    fn label(self) -> &'static str {
        match self {
            Capability::Db => "db",
            Capability::Csv => "csv",
            Capability::Sqlite => "sqlite",
            Capability::Json => "json",
            Capability::Testkit => "testkit",
            Capability::Fake => "fake",
            Capability::Http => "http",
            Capability::Format => "format",
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
#[derive(Default)]
struct Plan {
    deps: Vec<Dependency>,
    /// Build plugins to splice, as (artifactId, rendered `<plugin>` block).
    /// Plugin configuration is far too varied to model as a struct, so the
    /// capability renders the XML and pom.rs only places it.
    plugins: Vec<(&'static str, String)>,
    files: Vec<NewFile>,
}

pub fn add(
    capability: Capability,
    name: Option<&str>,
    dry_run: bool,
    package: Option<&str>,
) -> Result<()> {
    let root = find_project_root()?;
    let pom_text = pom::read(&root)?;
    let flavor = pom::flavor(&pom_text);

    // Emitting records and pattern-matching switches into a project pinned at
    // an older release produces code that cannot compile, so fail with
    // something actionable instead. The bar is what the generated code needs
    // (MIN_RELEASE), not what jails happens to default new projects to.
    match pom::release_level(&pom_text) {
        Some(level) if level < MIN_RELEASE => {
            return Err(format!(
                "this project targets Java {level}, but jails generates Java {MIN_RELEASE}+ code.\n       \
                 Raise <maven.compiler.release> (or <java.version>) to at least {MIN_RELEASE} in pom.xml and try again."
            ));
        }
        None => {
            return Err(format!(
                "pom.xml does not set a Java release level, and jails generates Java {MIN_RELEASE}+ code.\n       \
                 Add <maven.compiler.release>{TARGET_RELEASE}</maven.compiler.release> to <properties> and try again."
            ));
        }
        Some(_) => {}
    }

    let base = base_package(&root)?;
    // Capabilities land in the package their layer conventionally owns --
    // adapters for the file/database readers, api for the HTTP server -- so a
    // project that has grown a few of them still reads as a laid-out project
    // rather than a heap. `--package` overrides, and `--package ''` opts out.
    let place = |default: &str| subpackage(&base, package.unwrap_or(default));
    let plan = match capability {
        Capability::Db => db_plan(&root, flavor)?,
        Capability::Csv => csv_plan(&root, &place(layout::ADAPTERS), flavor, name)?,
        Capability::Sqlite => sqlite_plan(&root, &place(layout::ADAPTERS), flavor, name)?,
        Capability::Json => json_plan(&root, &place(layout::ADAPTERS), flavor, name)?,
        Capability::Testkit => testkit_plan(&root, &place(layout::TESTKIT))?,
        Capability::Fake => fake_plan(&root, &place(layout::TESTKIT))?,
        Capability::Http => http_plan(&root, &place(layout::API), name)?,
        Capability::Format => format_plan()?,
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

    let mut spliced_plugins: Vec<&str> = Vec::new();
    for (artifact_id, body) in &plan.plugins {
        match pom::add_plugin(&updated_pom, artifact_id, body)? {
            Some(next) => {
                updated_pom = next;
                spliced_plugins.push(artifact_id);
            }
            None => println!("  exists  plugin {artifact_id}"),
        }
    }

    if dry_run {
        for dep in &spliced {
            println!(
                "  would add dependency  {}:{}",
                dep.group_id, dep.artifact_id
            );
        }
        for artifact_id in &spliced_plugins {
            println!("  would add plugin  {artifact_id}");
        }
        for file in &plan.files {
            let verb = if file.path.exists() {
                "would skip (exists)"
            } else {
                "would create"
            };
            println!("  {verb}  {}", rel(&root, &file.path));
        }
        return Ok(());
    }

    if !spliced.is_empty() || !spliced_plugins.is_empty() {
        std::fs::write(root.join("pom.xml"), &updated_pom)
            .map_err(|e| format!("failed to write pom.xml: {e}"))?;
        for dep in &spliced {
            println!("     dep  {}:{}", dep.group_id, dep.artifact_id);
        }
        for artifact_id in &spliced_plugins {
            println!("  plugin  {artifact_id}");
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

    if created == 0 && spliced.is_empty() && spliced_plugins.is_empty() {
        println!("{} is already set up -- nothing to do", capability.label());
        return Ok(());
    }

    // Installing a formatter that immediately fails `mvn verify` is a bad
    // trade: the wrapping it wants is not something a template can predict, so
    // run it once and leave the project green.
    //
    // And if it cannot run at all, undo the pom edit. A formatter bound to
    // `verify` that crashes on this toolchain turns a working project into one
    // that cannot build -- palantir-java-format does exactly that when its
    // pinned version predates the JDK on PATH, which is a bad thing for a
    // scaffolding tool to leave behind.
    if matches!(capability, Capability::Format) {
        if crate::run::fmt_quietly(&root) {
            println!("  format  applied to the existing sources");
        } else {
            std::fs::write(root.join("pom.xml"), &pom_text)
                .map_err(|e| format!("failed to restore pom.xml: {e}"))?;
            return Err(
                "the formatter could not run on this toolchain, so pom.xml was left unchanged.\n       \
                 palantir-java-format needs a JDK it was built against -- try a current LTS (Java 25),\n       \
                 or configure Spotless yourself if you need a different formatter."
                    .to_string(),
            );
        }
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
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// db -- PostgreSQL, Flyway, and real integration tests; deliberately no ORM
// ---------------------------------------------------------------------------

const SPRING_JDBC: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-jdbc",
    version: None,
    scope: None,
};
const POSTGRES_MANAGED: Dependency = Dependency {
    group_id: "org.postgresql",
    artifact_id: "postgresql",
    version: None,
    scope: Some("runtime"),
};
const POSTGRES_PINNED: Dependency = Dependency {
    group_id: "org.postgresql",
    artifact_id: "postgresql",
    version: Some("42.7.11"),
    scope: Some("runtime"),
};
const FLYWAY_CORE_MANAGED: Dependency = Dependency {
    group_id: "org.flywaydb",
    artifact_id: "flyway-core",
    version: None,
    scope: None,
};
const FLYWAY_POSTGRES_MANAGED: Dependency = Dependency {
    group_id: "org.flywaydb",
    artifact_id: "flyway-database-postgresql",
    version: None,
    scope: None,
};
const FLYWAY_CORE_PINNED: Dependency = Dependency {
    group_id: "org.flywaydb",
    artifact_id: "flyway-core",
    version: Some("12.8.1"),
    scope: None,
};
const FLYWAY_POSTGRES_PINNED: Dependency = Dependency {
    group_id: "org.flywaydb",
    artifact_id: "flyway-database-postgresql",
    version: Some("12.8.1"),
    scope: None,
};
const TESTCONTAINERS_POSTGRES: Dependency = Dependency {
    group_id: "org.testcontainers",
    artifact_id: "testcontainers-postgresql",
    version: Some("2.0.5"),
    scope: Some("test"),
};
const TESTCONTAINERS_JUNIT: Dependency = Dependency {
    group_id: "org.testcontainers",
    artifact_id: "testcontainers-junit-jupiter",
    version: Some("2.0.5"),
    scope: Some("test"),
};

fn db_plan(root: &std::path::Path, flavor: Flavor) -> Result<Plan> {
    let mut deps = match flavor {
        Flavor::SpringBoot => vec![
            SPRING_JDBC,
            POSTGRES_MANAGED,
            FLYWAY_CORE_MANAGED,
            FLYWAY_POSTGRES_MANAGED,
        ],
        Flavor::PlainMaven => vec![POSTGRES_PINNED, FLYWAY_CORE_PINNED, FLYWAY_POSTGRES_PINNED],
    };
    deps.extend([TESTCONTAINERS_POSTGRES, TESTCONTAINERS_JUNIT]);

    Ok(Plan {
        deps,
        plugins: vec![],
        files: vec![NewFile {
            path: root.join("src/main/resources/db/migration/.gitkeep"),
            contents: String::new(),
        }],
    })
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

fn csv_plan(
    root: &std::path::Path,
    pkg: &str,
    _flavor: Flavor,
    name: Option<&str>,
) -> Result<Plan> {
    let base = capitalize(name.unwrap_or("Csv"));
    let class = format!("{base}Reader");

    Ok(Plan {
        // Spring Boot's dependency management does not cover commons-csv, so
        // the version is pinned in both flavors.
        deps: vec![COMMONS_CSV],
        plugins: vec![],
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
/// nothing beyond the driver or the fiddliness of a persistence framework.
/// A Spring project can inject the record wherever it needs a connection.
fn sqlite_plan(
    root: &std::path::Path,
    pkg: &str,
    _flavor: Flavor,
    name: Option<&str>,
) -> Result<Plan> {
    let base = name.map(capitalize).unwrap_or_default();
    let database = format!("{base}Database");
    let migrations = format!("{base}Migrations");

    Ok(Plan {
        deps: vec![SQLITE_JDBC],
        plugins: vec![],
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

const JACKSON_VERSION: &str = "2.19.0";

const JACKSON: Dependency = Dependency {
    group_id: "com.fasterxml.jackson.core",
    artifact_id: "jackson-databind",
    version: Some(JACKSON_VERSION),
    scope: None,
};

/// `findAndRegisterModules()` only finds modules that are actually on the
/// classpath, and plain `jackson-databind` ships no `java.time` support. Since
/// `generate`'s field-type table maps `date`/`datetime` to `LocalDate` and
/// `LocalDateTime`, leaving this out means every generated date serialises as
/// a nested `{"year":...}` object instead of an ISO string. Spring Boot pulls
/// it in transitively, so only the plain-Maven flavor felt it.
const JACKSON_JSR310: Dependency = Dependency {
    group_id: "com.fasterxml.jackson.datatype",
    artifact_id: "jackson-datatype-jsr310",
    version: Some(JACKSON_VERSION),
    scope: None,
};

fn json_plan(
    root: &std::path::Path,
    pkg: &str,
    flavor: Flavor,
    name: Option<&str>,
) -> Result<Plan> {
    let base = name.map(capitalize).unwrap_or_default();
    let class = format!("{base}Json");

    // Spring Boot's dependency management already pins Jackson (and the web
    // starter pulls it in transitively), so declaring a version here would
    // fight the parent pom.
    let deps = match flavor {
        Flavor::SpringBoot => vec![
            Dependency {
                version: None,
                ..JACKSON
            },
            Dependency {
                version: None,
                ..JACKSON_JSR310
            },
        ],
        Flavor::PlainMaven => vec![JACKSON, JACKSON_JSR310],
    };

    Ok(Plan {
        deps,
        plugins: vec![],
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
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.SerializationFeature;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * JSON reading and writing over one shared, thread-safe {{@link ObjectMapper}}.
 *
 * <p>Records map to JSON objects without any annotations, and
 * {{@code findAndRegisterModules()}} picks up the java.time module that ships
 * alongside this class, so {{@code LocalDate}} round-trips as an ISO string.
 *
 * <p>Two ways in, for two situations. {{@link #read}} binds the whole document
 * to a type -- right for input you control, wrong for input you do not, since
 * one bad element fails the entire parse. For untrusted input use
 * {{@link #readTree}} and {{@link #convert}} to validate element by element,
 * keeping the good records and reporting the bad ones.
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

    /**
     * Reads the whole document as a tree, without binding it to any type.
     *
     * <p>Use this when the shape cannot be trusted: walk the tree, check each
     * node with {{@code isObject()}} and friends, and {{@link #convert}} the ones
     * that look right. Nothing is lost to a single malformed element.
     */
    public static JsonNode readTree(Path path) throws IOException {{
        try (var in = Files.newInputStream(path)) {{
            return MAPPER.readTree(in);
        }}
    }}

    /** Binds one already-parsed tree node to {{@code type}}. */
    public static <T> T convert(JsonNode node, Class<T> type) {{
        return MAPPER.convertValue(node, type);
    }}

    /**
     * Reads a JSON Lines file: one JSON value per line, blank lines skipped.
     *
     * <p>The format event logs and streaming exports use, because appending a
     * line is cheap where appending to an array is not. Returned as trees
     * rather than bound values for the same reason {{@link #readTree}} exists --
     * one malformed line should not cost you the whole file.
     */
    public static List<JsonNode> readJsonl(Path path) throws IOException {{
        try (var lines = Files.lines(path)) {{
            var nodes = new ArrayList<JsonNode>();
            for (var line : lines.filter(text -> !text.isBlank()).toList()) {{
                nodes.add(MAPPER.readTree(line));
            }}
            return List.copyOf(nodes);
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
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.file.Files;
import java.nio.file.Path;
import java.time.LocalDate;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class {class}Test {{

    /** Records need no annotations to round-trip. */
    record Item(String name, int qty) {{}}

    record Dated(String name, LocalDate on) {{}}

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

    /**
     * Without the java.time module on the classpath this writes
     * {{@code {{"year":2026,...}}}} instead of an ISO string, and reading it back
     * fails outright.
     */
    @Test
    void writesDatesAsIsoStringsNotObjects() throws Exception {{
        var json = {class}.toJson(new Dated("invoice", LocalDate.of(2026, 8, 1)));

        assertTrue(json.contains("\"2026-08-01\""), "expected an ISO date in " + json);
        assertEquals(new Dated("invoice", LocalDate.of(2026, 8, 1)), {class}.parse(json, Dated.class));
    }}

    @Test
    void readsOneJsonValuePerLine() throws Exception {{
        var path = tmp.resolve("events.jsonl");
        Files.writeString(path, "{{\"id\":1}}\n\n{{\"id\":2}}\n");

        var events = {class}.readJsonl(path);

        assertEquals(2, events.size(), "blank lines should be skipped");
        assertEquals(1, events.getFirst().get("id").asInt());
        assertEquals(2, events.getLast().get("id").asInt());
    }}

    @Test
    void readsAnEmptyJsonlFileAsNoEvents() throws Exception {{
        var path = tmp.resolve("empty.jsonl");
        Files.writeString(path, "");

        assertEquals(List.of(), {class}.readJsonl(path));
    }}

    @Test
    void readsATreeWithoutBindingItToAType() throws Exception {{
        var path = tmp.resolve("tree.json");
        Files.writeString(path, "{{\"items\":[{{\"name\":\"bolt\",\"qty\":7}}]}}");

        var root = {class}.readTree(path);

        assertTrue(root.isObject());
        assertEquals("bolt", root.get("items").get(0).get("name").asText());
    }}

    /**
     * The reason the tree API exists: a document with junk mixed into an array
     * still yields every well-formed element, rather than failing as a whole.
     */
    @Test
    void keepsGoodElementsWhenSiblingsAreMalformed() throws Exception {{
        var path = tmp.resolve("mixed.json");
        Files.writeString(path, "[{{\"name\":\"bolt\",\"qty\":7}},\"not-an-object\",{{\"name\":\"nut\",\"qty\":3}}]");

        var good = new ArrayList<Item>();
        var skipped = 0;
        for (var node : {class}.readTree(path)) {{
            if (node.isObject()) {{
                good.add({class}.convert(node, Item.class));
            }} else {{
                skipped++;
            }}
        }}

        assertEquals(List.of(new Item("bolt", 7), new Item("nut", 3)), good);
        assertEquals(1, skipped);
    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// testkit
// ---------------------------------------------------------------------------

/// The four things every testable CLI needs and nobody enjoys writing twice.
/// No dependency: JUnit and AssertJ are already there, and everything here is
/// plain JDK.
///
/// These helpers also apply pressure in the right direction. `Clocks` and
/// `Ids` are only usable by code that *takes* a `Clock` and a
/// `Supplier<String>` instead of calling `Instant.now()` and
/// `UUID.randomUUID()` -- so generating them nudges the design toward the one
/// that can be tested deterministically at all.
fn testkit_plan(root: &std::path::Path, testkit: &str) -> Result<Plan> {
    let dir = test_dir(root, testkit);

    Ok(Plan {
        files: vec![
            NewFile {
                path: dir.join("Clocks.java"),
                contents: clocks_java(testkit),
            },
            NewFile {
                path: dir.join("Ids.java"),
                contents: ids_java(testkit),
            },
            NewFile {
                path: dir.join("Fixtures.java"),
                contents: fixtures_java(testkit),
            },
            NewFile {
                path: dir.join("Cli.java"),
                contents: testkit_cli_java(testkit),
            },
            NewFile {
                path: dir.join("TestkitTest.java"),
                contents: testkit_test_java(testkit),
            },
            NewFile {
                path: root.join("src/test/resources/fixtures/example.json"),
                contents: EXAMPLE_FIXTURE.to_string(),
            },
        ],
        ..Plan::default()
    })
}

const EXAMPLE_FIXTURE: &str = "{\n  \"name\": \"bolt\",\n  \"qty\": 7\n}\n";

fn clocks_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.time.ZoneId;
import java.time.ZoneOffset;

/**
 * Deterministic clocks.
 *
 * <p>These only work on code that accepts a {{@link Clock}} rather than calling
 * {{@code Instant.now()}}. That is the point: taking the clock as a parameter is
 * what makes a timestamp assertable at all.
 *
 * <p>{{@code Clock.fixed}} is already in the JDK, so only the stepping clock --
 * for asserting that events are ordered and distinct -- needs writing.
 */
public final class Clocks {{

    /** An arbitrary, memorable instant. Deterministic is the only requirement. */
    public static final Instant DEFAULT_START = Instant.parse("2026-01-01T00:00:00Z");

    private Clocks() {{}}

    public static Clock fixed(Instant instant) {{
        return Clock.fixed(instant, ZoneOffset.UTC);
    }}

    public static Clock fixed() {{
        return fixed(DEFAULT_START);
    }}

    /** A clock that advances by {{@code step}} on every read. */
    public static Clock stepping(Instant start, Duration step) {{
        return new SteppingClock(start, step, ZoneOffset.UTC);
    }}

    public static Clock stepping() {{
        return stepping(DEFAULT_START, Duration.ofSeconds(1));
    }}

    private static final class SteppingClock extends Clock {{

        private final Duration step;
        private final ZoneId zone;
        private Instant current;

        private SteppingClock(Instant start, Duration step, ZoneId zone) {{
            this.current = start;
            this.step = step;
            this.zone = zone;
        }}

        @Override
        public ZoneId getZone() {{
            return zone;
        }}

        @Override
        public Clock withZone(ZoneId other) {{
            return new SteppingClock(current, step, other);
        }}

        @Override
        public synchronized Instant instant() {{
            var value = current;
            current = current.plus(step);
            return value;
        }}
    }}
}}
"#
    )
}

fn ids_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Supplier;

/**
 * Deterministic identifiers.
 *
 * <p>The counterpart to {{@link Clocks}}: code that takes a
 * {{@code Supplier<String>}} instead of calling {{@code UUID.randomUUID()}} can
 * have its output asserted in full, identifiers included.
 */
public final class Ids {{

    private Ids() {{}}

    /** Yields {{@code prefix-1}}, {{@code prefix-2}}, ... */
    public static Supplier<String> sequential(String prefix, int start) {{
        var next = new AtomicInteger(start);
        return () -> prefix + "-" + next.getAndIncrement();
    }}

    public static Supplier<String> sequential(String prefix) {{
        return sequential(prefix, 1);
    }}
}}
"#
    )
}

fn fixtures_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import java.io.IOException;
import java.io.UncheckedIOException;
import java.net.URISyntaxException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

/**
 * Loads sample files from {{@code src/test/resources/fixtures}}.
 *
 * <p>Off the classpath, not by walking relative paths from the working
 * directory: {{@code Path.of("../fixtures")}} works until something runs the
 * suite from elsewhere, and then fails in a way that looks like a test bug.
 *
 * <p>A missing fixture fails immediately, naming what it looked for. Silently
 * returning empty input turns a typo into a passing test.
 */
public final class Fixtures {{

    private static final String ROOT = "/fixtures/";

    private Fixtures() {{}}

    /** Raw bytes of a fixture, e.g. {{@code bytes("example.json")}}. */
    public static byte[] bytes(String name) {{
        try (var in = Fixtures.class.getResourceAsStream(ROOT + name)) {{
            if (in == null) {{
                throw new IllegalArgumentException("no fixture named '" + name + "' under src/test/resources" + ROOT);
            }}
            return in.readAllBytes();
        }} catch (IOException error) {{
            throw new UncheckedIOException("unreadable fixture: " + name, error);
        }}
    }}

    public static String text(String name) {{
        return new String(bytes(name), StandardCharsets.UTF_8);
    }}

    /** Non-blank lines, for line-oriented formats like CSV and JSONL. */
    public static List<String> lines(String name) {{
        return text(name).lines().filter(line -> !line.isBlank()).toList();
    }}

    /** Real filesystem path, for code under test that insists on a {{@link Path}}. */
    public static Path path(String name) {{
        var url = Fixtures.class.getResource(ROOT + name);
        if (url == null) {{
            throw new IllegalArgumentException("no fixture named '" + name + "' under src/test/resources" + ROOT);
        }}
        try {{
            return Path.of(url.toURI());
        }} catch (URISyntaxException error) {{
            throw new IllegalStateException("fixture path is not a file: " + name, error);
        }}
    }}

    /** Copies a fixture into {{@code directory}}, for tests that mutate their input. */
    public static Path copyTo(String name, Path directory) {{
        try {{
            Files.createDirectories(directory);
            var target = directory.resolve(Path.of(name).getFileName().toString());
            Files.write(target, bytes(name));
            return target;
        }} catch (IOException error) {{
            throw new UncheckedIOException("could not copy fixture " + name, error);
        }}
    }}
}}
"#
    )
}

fn testkit_cli_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.util.List;

/**
 * Runs a command in-process and captures what a user would have seen.
 *
 * <p>No {{@code System.setOut}} anywhere: the command under test takes its
 * streams as arguments, so capturing them is just passing different ones. That
 * keeps these tests safe to run in parallel, which the swap-the-global approach
 * never is.
 *
 * <p>{{@link Command}} matches the shape {{@code jails generate command}} and
 * {{@code jails generate cli}} emit, so a real command is a method reference:
 *
 * {{@snippet :
 * var run = Cli.run(GreetCommand::run, "world");
 * assertThat(run.exitCode()).isZero();
 * assertThat(run.out()).contains("hello world");
 * }}
 */
public final class Cli {{

    /** Anything that takes streams plus argv and returns an exit code. */
    @FunctionalInterface
    public interface Command {{
        int run(PrintStream out, PrintStream err, String... args);
    }}

    /** What one invocation produced. */
    public record Run(String out, String err, int exitCode) {{

        /** Stdout split into non-blank lines, for asserting line by line. */
        public List<String> outLines() {{
            return out.lines().filter(line -> !line.isBlank()).toList();
        }}

        public boolean succeeded() {{
            return exitCode == 0;
        }}
    }}

    private Cli() {{}}

    public static Run run(Command command, String... args) {{
        var out = new ByteArrayOutputStream();
        var err = new ByteArrayOutputStream();
        int exitCode;
        try (var capturedOut = new PrintStream(out, true, StandardCharsets.UTF_8);
                var capturedErr = new PrintStream(err, true, StandardCharsets.UTF_8)) {{
            exitCode = command.run(capturedOut, capturedErr, args);
        }}
        return new Run(out.toString(StandardCharsets.UTF_8), err.toString(StandardCharsets.UTF_8), exitCode);
    }}
}}
"#
    )
}

fn testkit_test_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.time.Instant;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

/** Proves the test kit itself works, so a failure elsewhere is never its fault. */
class TestkitTest {{

    @Test
    void fixedClockDoesNotMove() {{
        var clock = Clocks.fixed();

        assertThat(clock.instant()).isEqualTo(Clocks.DEFAULT_START).isEqualTo(clock.instant());
    }}

    @Test
    void steppingClockAdvancesOnEveryRead() {{
        var clock = Clocks.stepping(Instant.parse("2026-01-01T00:00:00Z"), Duration.ofMinutes(1));

        assertThat(clock.instant()).isEqualTo(Instant.parse("2026-01-01T00:00:00Z"));
        assertThat(clock.instant()).isEqualTo(Instant.parse("2026-01-01T00:01:00Z"));
    }}

    @Test
    void idsAreSequentialAndPrefixed() {{
        var ids = Ids.sequential("txn");

        assertThat(ids.get()).isEqualTo("txn-1");
        assertThat(ids.get()).isEqualTo("txn-2");
    }}

    @Test
    void fixturesLoadOffTheClasspath() {{
        assertThat(Fixtures.text("example.json")).contains("bolt");
        assertThat(Fixtures.path("example.json")).exists();
    }}

    /** A typo in a fixture name must fail, not quietly read nothing. */
    @Test
    void aMissingFixtureNamesWhatItLookedFor() {{
        assertThatThrownBy(() -> Fixtures.text("nope.json"))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("nope.json");
    }}

    @Test
    void cliCapturesBothStreamsAndTheExitCode() {{
        var run = Cli.run(
                (out, err, args) -> {{
                    out.println("out: " + String.join(",", args));
                    err.println("err");
                    return 3;
                }},
                "a",
                "b");

        assertThat(run.out()).contains("out: a,b");
        assertThat(run.err()).contains("err");
        assertThat(run.exitCode()).isEqualTo(3);
        assertThat(run.succeeded()).isFalse();
    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// fake
// ---------------------------------------------------------------------------

/// A scripted test double. Generic by construction: jails has no Java parser
/// and no business acquiring one, so rather than generating a fake *of* some
/// interface, this generates the replay engine and you attach it to any
/// interface with a lambda. One class covers every collaborator in the project.
fn fake_plan(root: &std::path::Path, testkit: &str) -> Result<Plan> {
    let dir = test_dir(root, testkit);

    Ok(Plan {
        files: vec![
            NewFile {
                path: dir.join("Fake.java"),
                contents: scripted_java(testkit),
            },
            NewFile {
                path: dir.join("FakeTest.java"),
                contents: scripted_test_java(testkit),
            },
        ],
        ..Plan::default()
    })
}

fn scripted_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * A collaborator that replays a fixed script and records how it was called.
 *
 * <p>Attach it to any interface with a lambda -- which is why this is one class
 * rather than one fake per interface, and why it needs no mocking framework:
 *
 * {{@snippet :
 * var model = Fake.of(Fake.value("ok"), Fake.failure(new IllegalStateException("timeout")));
 * ModelProvider provider = prompt -> model.next(prompt);
 *
 * assertThat(provider.generate("hello")).isEqualTo("ok");
 * assertThat(model.calls()).containsExactly(List.of("hello"));
 * }}
 *
 * <p>Once the script runs out the last step repeats, so a test that only cares
 * about the first response does not have to pad the script to match.
 */
public final class Fake<T> {{

    /** One scripted turn. Sealed, so a switch over it is checked for exhaustiveness. */
    public sealed interface Step<T> {{}}

    public record Value<T>(T value) implements Step<T> {{}}

    public record Failure<T>(RuntimeException error) implements Step<T> {{}}

    private final List<Step<T>> script;
    private final List<List<Object>> calls = new ArrayList<>();
    private int index = 0;

    private Fake(List<Step<T>> script) {{
        if (script.isEmpty()) {{
            throw new IllegalArgumentException("a fake needs at least one step");
        }}
        this.script = List.copyOf(script);
    }}

    @SafeVarargs
    public static <T> Fake<T> of(Step<T>... steps) {{
        return new Fake<>(List.of(steps));
    }}

    public static <T> Step<T> value(T value) {{
        return new Value<>(value);
    }}

    public static <T> Step<T> failure(RuntimeException error) {{
        return new Failure<>(error);
    }}

    /**
     * Records the arguments it was called with, then plays the next step.
     *
     * <p>{{@code Stream.toList()}} rather than {{@code List.of}}: a null argument
     * is a perfectly ordinary thing to want to assert a collaborator was
     * called with, and {{@code List.of}} rejects it.
     */
    public T next(Object... arguments) {{
        calls.add(Arrays.stream(arguments).toList());
        var step = script.get(Math.min(index++, script.size() - 1));
        return switch (step) {{
            case Value<T>(var value) -> value;
            case Failure<T>(var error) -> throw error;
        }};
    }}

    /** Every call so far, in order, each as its argument list. */
    public List<List<Object>> calls() {{
        return List.copyOf(calls);
    }}

    public int callCount() {{
        return calls.size();
    }}
}}
"#
    )
}

fn scripted_test_java(pkg: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;

import java.util.List;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

class FakeTest {{

    @Test
    void playsEachStepInOrder() {{
        var fake = Fake.of(Fake.value("first"), Fake.value("second"));

        assertThat(fake.next()).isEqualTo("first");
        assertThat(fake.next()).isEqualTo("second");
    }}

    @Test
    void repeatsTheLastStepOnceTheScriptRunsOut() {{
        var fake = Fake.of(Fake.value("only"));

        assertThat(fake.next()).isEqualTo("only");
        assertThat(fake.next()).isEqualTo("only");
    }}

    @Test
    void throwsWhateverTheScriptSaysToThrow() {{
        var fake = Fake.<String>of(Fake.failure(new IllegalStateException("simulated timeout")));

        assertThatThrownBy(fake::next).isInstanceOf(IllegalStateException.class).hasMessage("simulated timeout");
    }}

    @Test
    void recordsHowItWasCalled() {{
        var fake = Fake.of(Fake.value(1));

        fake.next("a", 2);
        fake.next("b");

        assertThat(fake.calls()).containsExactly(List.of("a", 2), List.of("b"));
        assertThat(fake.callCount()).isEqualTo(2);
    }}

    @Test
    void rejectsAnEmptyScript() {{
        assertThatThrownBy(Fake::of).isInstanceOf(IllegalArgumentException.class);
    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// http
// ---------------------------------------------------------------------------

/// An HTTP server with no dependency at all: `com.sun.net.httpserver` has
/// shipped in the JDK since 6 and is a supported API, and `java.net.http`
/// gives the test its client. A framework here would be the biggest dependency
/// in the project and buy nothing a route map does not.
fn http_plan(root: &std::path::Path, pkg: &str, name: Option<&str>) -> Result<Plan> {
    let base = name.map(capitalize).unwrap_or_default();
    let class = format!("{base}Server");

    Ok(Plan {
        files: vec![
            NewFile {
                path: main_dir(root, pkg).join(format!("{class}.java")),
                contents: http_server_java(pkg, &class),
            },
            NewFile {
                path: test_dir(root, pkg).join(format!("{class}Test.java")),
                contents: http_server_test_java(pkg, &class),
            },
        ],
        ..Plan::default()
    })
}

fn http_server_java(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpServer;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * A small HTTP server on the JDK's own {{@code com.sun.net.httpserver}} -- no
 * framework, no container, no dependency.
 *
 * <p>Handlers are pure functions from {{@link Request}} to {{@link Response}}, so
 * the interesting half can be unit-tested without any socket at all; this class
 * only maps them onto HTTP.
 *
 * <p>Requests are served on virtual threads, so a handler that blocks on I/O
 * costs a stack, not a platform thread.
 *
 * {{@snippet :
 * try (var server = {class}.start(0, Map.of("/health", request -> Response.ok("{{\"status\":\"up\"}}")))) {{
 *     var uri = URI.create("http://localhost:" + server.port() + "/health");
 * }}
 * }}
 */
public final class {class} implements AutoCloseable {{

    /** Everything a handler is allowed to see. */
    public record Request(String method, String path, String query, String body) {{}}

    /** Everything a handler can say. JSON by default -- override for anything else. */
    public record Response(int status, String contentType, String body) {{

        public static Response ok(String body) {{
            return new Response(200, "application/json", body);
        }}

        public static Response text(String body) {{
            return new Response(200, "text/plain; charset=utf-8", body);
        }}

        public static Response notFound() {{
            return new Response(404, "application/json", "{{\"error\":\"not found\"}}");
        }}

        public static Response badRequest(String message) {{
            return new Response(400, "application/json", "{{\"error\":\"" + escape(message) + "\"}}");
        }}

        /**
         * Escapes exactly what a JSON string body needs. Deliberately not a JSON
         * library: this class has no dependencies, and one interpolated message
         * does not justify adding one. Build real payloads with a real
         * serialiser -- {{@code jails add json}} gives you Jackson.
         */
        private static String escape(String text) {{
            var out = new StringBuilder(text.length() + 16);
            for (var c : text.toCharArray()) {{
                switch (c) {{
                    case '"' -> out.append("\\\"");
                    case '\\' -> out.append("\\\\");
                    case '\n' -> out.append("\\n");
                    case '\r' -> out.append("\\r");
                    case '\t' -> out.append("\\t");
                    // Appended from a char rather than written as one literal:
                    // Java translates a backslash-u escape before it even lexes
                    // the file, and %04x is not four hex digits, so the obvious
                    // spelling is an "illegal unicode escape" at compile time.
                    // (Which applies to comments too -- hence this wording.)
                    default -> {{
                        if (c < 0x20) {{
                            out.append('\\').append("u%04x".formatted((int) c));
                        }} else {{
                            out.append(c);
                        }}
                    }}
                }}
            }}
            return out.toString();
        }}
    }}

    @FunctionalInterface
    public interface Handler {{
        Response handle(Request request);
    }}

    private final HttpServer http;
    private final ExecutorService requests;

    private {class}(HttpServer http, ExecutorService requests) {{
        this.http = http;
        this.requests = requests;
    }}

    /**
     * Binds and starts. Pass port 0 to let the OS pick a free one and read it
     * back from {{@link #port()}} -- which is what makes tests safe to run in
     * parallel, and CI safe from whatever else is listening on 8080.
     */
    public static {class} start(int port, Map<String, Handler> routes) {{
        try {{
            var http = HttpServer.create(new InetSocketAddress(port), 0);
            routes.forEach((path, handler) -> http.createContext(path, exchange -> dispatch(exchange, handler)));
            var requests = Executors.newVirtualThreadPerTaskExecutor();
            http.setExecutor(requests);
            http.start();
            return new {class}(http, requests);
        }} catch (IOException error) {{
            throw new UncheckedIOException("could not start the server on port " + port, error);
        }}
    }}

    public int port() {{
        return http.getAddress().getPort();
    }}

    private static void dispatch(HttpExchange exchange, Handler handler) throws IOException {{
        try (exchange) {{
            var uri = exchange.getRequestURI();
            Response response;
            try (var in = exchange.getRequestBody()) {{
                var body = new String(in.readAllBytes(), StandardCharsets.UTF_8);
                var request = new Request(exchange.getRequestMethod(), uri.getPath(), uri.getQuery(), body);
                // A handler that throws must not leave the connection hanging:
                // the client would block until it timed out, with nothing said.
                response = handle(handler, request);
            }}

            var bytes = response.body().getBytes(StandardCharsets.UTF_8);
            exchange.getResponseHeaders().set("Content-Type", response.contentType());
            exchange.sendResponseHeaders(response.status(), bytes.length);
            try (var out = exchange.getResponseBody()) {{
                out.write(bytes);
            }}
        }}
    }}

    private static Response handle(Handler handler, Request request) {{
        try {{
            return handler.handle(request);
        }} catch (RuntimeException error) {{
            // The client gets nothing useful (deliberately -- an exception
            // message can carry internals), but swallowing it outright leaves
            // nobody anything to debug from. Swap in a logger when you add one.
            System.err.println("handler failed for " + request.method() + " " + request.path());
            error.printStackTrace();
            return new Response(500, "application/json", "{{\"error\":\"internal error\"}}");
        }}
    }}

    /**
     * Stops accepting connections and shuts the request executor down.
     *
     * <p>Both halves matter: {{@link HttpServer#stop}} does <em>not</em> shut down
     * an executor the caller supplied, so stopping without this leaks one per
     * server -- which a test that starts a server per case does many times over.
     */
    @Override
    public void close() {{
        http.stop(0);
        requests.close();
    }}
}}
"#
    )
}

fn http_server_test_java(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;

import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.util.Map;

import static org.assertj.core.api.Assertions.assertThat;

/** End-to-end over a real socket, on an ephemeral port so nothing collides. */
class {class}Test {{

    private static final Map<String, {class}.Handler> ROUTES = Map.of(
            "/health", request -> {class}.Response.ok("{{\"status\":\"up\"}}"),
            "/echo", request -> {class}.Response.text(request.method() + " " + request.body()),
            "/boom", request -> {{
                throw new IllegalStateException("handler blew up");
            }});

    private HttpResponse<String> call(int port, String path, String body) throws Exception {{
        var request = HttpRequest.newBuilder(URI.create("http://localhost:" + port + path))
                .method(body == null ? "GET" : "POST", body == null
                        ? HttpRequest.BodyPublishers.noBody()
                        : HttpRequest.BodyPublishers.ofString(body))
                .build();
        try (var client = HttpClient.newHttpClient()) {{
            return client.send(request, HttpResponse.BodyHandlers.ofString());
        }}
    }}

    @Test
    void servesARegisteredRoute() throws Exception {{
        try (var server = {class}.start(0, ROUTES)) {{
            var response = call(server.port(), "/health", null);

            assertThat(response.statusCode()).isEqualTo(200);
            assertThat(response.body()).contains("up");
            assertThat(response.headers().firstValue("Content-Type")).hasValue("application/json");
        }}
    }}

    @Test
    void handsTheHandlerTheMethodAndBody() throws Exception {{
        try (var server = {class}.start(0, ROUTES)) {{
            assertThat(call(server.port(), "/echo", "hello").body()).isEqualTo("POST hello");
        }}
    }}

    @Test
    void answersUnknownPathsWithFourOhFour() throws Exception {{
        try (var server = {class}.start(0, ROUTES)) {{
            assertThat(call(server.port(), "/nope", null).statusCode()).isEqualTo(404);
        }}
    }}

    /** A throwing handler must still answer, or the client just hangs. */
    @Test
    void turnsAHandlerExceptionIntoAFiveHundred() throws Exception {{
        try (var server = {class}.start(0, ROUTES)) {{
            assertThat(call(server.port(), "/boom", null).statusCode()).isEqualTo(500);
        }}
    }}

    @Test
    void picksAFreePortWhenAskedForZero() {{
        try (var server = {class}.start(0, ROUTES)) {{
            assertThat(server.port()).isPositive();
        }}
    }}
}}
"#
    )
}

// ---------------------------------------------------------------------------
// format
// ---------------------------------------------------------------------------

/// Spotless, bound to `verify` as a check and available as `jails fmt` to
/// apply. Formatting nobody has to think about is the only kind that survives.
const SPOTLESS_ARTIFACT: &str = "spotless-maven-plugin";

fn format_plan() -> Result<Plan> {
    Ok(Plan {
        plugins: vec![(SPOTLESS_ARTIFACT, SPOTLESS_PLUGIN.to_string())],
        ..Plan::default()
    })
}

/// palantir-java-format over google-java-format: it keeps a 120-column line,
/// which the generated code (records with several components, fluent AssertJ
/// chains) reads far better at than 100. Both are pinned -- a formatter that
/// drifts version rewrites files nobody touched.
const SPOTLESS_PLUGIN: &str = r#"<plugin>
    <groupId>com.diffplug.spotless</groupId>
    <artifactId>spotless-maven-plugin</artifactId>
    <version>3.9.0</version>
    <configuration>
        <java>
            <palantirJavaFormat>
                <version>2.97.0</version>
            </palantirJavaFormat>
            <removeUnusedImports/>
        </java>
    </configuration>
    <executions>
        <execution>
            <id>spotless-check</id>
            <phase>verify</phase>
            <goals>
                <goal>check</goal>
            </goals>
        </execution>
    </executions>
</plugin>"#;

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
        assert!(
            src.contains("public record Row("),
            "rows should be a record"
        );
        assert!(src.contains(".toList()"), "should use Stream.toList()");
        assert!(
            src.contains("try (var reader"),
            "should use try-with-resources"
        );
        assert!(
            !src.contains("java.io.File"),
            "should use NIO paths, not File"
        );
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
        assert!(
            migrations.contains("schema_migrations"),
            "applied scripts must be tracked"
        );
        assert!(
            migrations.contains("connection.rollback()"),
            "a failed script must not half-apply"
        );
        assert!(migrations.contains("\"\"\""), "SQL should use a text block");
        // The generated helper uses only JDBC and its own migration table.
        assert!(!migrations.contains("org.springframework"));
        assert!(!db.contains("org.springframework"));
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
        assert!(spring.deps.iter().all(|d| d.version.is_none()));
        let plain = json_plan(root, "com.example.demo", Flavor::PlainMaven, None).unwrap();
        assert!(
            plain
                .deps
                .iter()
                .all(|d| d.version == Some(JACKSON_VERSION))
        );
    }

    /// Without jackson-datatype-jsr310 on the classpath,
    /// `findAndRegisterModules()` finds no java.time support and every
    /// LocalDate -- a type `generate`'s own field table emits -- serialises as
    /// a nested object instead of an ISO string.
    #[test]
    fn json_ships_the_java_time_module_alongside_databind() {
        let root = std::path::Path::new("/tmp/does-not-matter");
        for flavor in [Flavor::SpringBoot, Flavor::PlainMaven] {
            let plan = json_plan(root, "com.example.demo", flavor, None).unwrap();
            let artifacts: Vec<&str> = plan.deps.iter().map(|d| d.artifact_id).collect();
            assert!(
                artifacts.contains(&"jackson-databind"),
                "{flavor:?} is missing databind"
            );
            assert!(
                artifacts.contains(&"jackson-datatype-jsr310"),
                "{flavor:?} is missing java.time support"
            );
        }
    }

    /// The two Jackson artifacts are released in lockstep and mixing versions
    /// across them is a documented source of NoSuchMethodError.
    #[test]
    fn json_pins_both_jackson_artifacts_to_one_version() {
        assert_eq!(JACKSON.version, JACKSON_JSR310.version);
    }

    /// `read(path, type)` loses the whole document to one bad element, so the
    /// generated class has to offer a tree route for untrusted input.
    #[test]
    fn json_offers_a_tree_api_for_input_whose_shape_is_not_trusted() {
        let src = json_java("com.example.demo", "Json");
        assert!(src.contains("public static JsonNode readTree(Path path)"));
        assert!(src.contains("public static <T> T convert(JsonNode node, Class<T> type)"));
        assert!(src.contains("import com.fasterxml.jackson.databind.JsonNode;"));

        let test = json_test_java("com.example.demo", "Json");
        assert!(test.contains("keepsGoodElementsWhenSiblingsAreMalformed"));
        assert!(test.contains("writesDatesAsIsoStringsNotObjects"));
    }

    /// JSON Lines is the format event logs use, and one malformed line must
    /// not cost the whole file -- so it returns trees, like readTree.
    #[test]
    fn json_reads_jsonl_as_a_list_of_trees() {
        let src = json_java("com.example.demo", "Json");
        assert!(
            src.contains("public static List<JsonNode> readJsonl(Path path)"),
            "{src}"
        );
        assert!(
            src.contains("isBlank"),
            "blank lines should be skipped: {src}"
        );

        let test = json_test_java("com.example.demo", "Json");
        assert!(test.contains("readJsonl"));
        assert!(test.contains("readsAnEmptyJsonlFileAsNoEvents"));
    }

    #[test]
    fn json_uses_nio_streams_rather_than_file() {
        let src = json_java("com.example.demo", "Json");
        assert!(src.contains("Files.newInputStream"));
        assert!(src.contains("Files.newOutputStream"));
        assert!(
            !src.contains("java.io.File"),
            "should not fall back to java.io.File"
        );
        assert!(
            src.contains("private static final ObjectMapper MAPPER"),
            "mapper should be shared"
        );
    }

    /// validation/09 addresses the scripted double as `Fake`; the class and
    /// its file have to agree with that.
    #[test]
    fn the_scripted_double_is_called_fake() {
        let src = scripted_java("com.example.demo.testkit");
        assert!(src.contains("public final class Fake<T>"), "{src}");
        assert!(!src.contains("Scripted"), "no trace of the old name: {src}");

        let test = scripted_test_java("com.example.demo.testkit");
        assert!(test.contains("class FakeTest"));
        assert!(!test.contains("Scripted"));
    }

    #[test]
    fn capitalize_uppercases_the_first_letter_only() {
        assert_eq!(capitalize("csv"), "Csv");
        assert_eq!(capitalize("transaction"), "Transaction");
        assert_eq!(capitalize(""), "");
    }
}
