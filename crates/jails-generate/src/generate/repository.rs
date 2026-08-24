//! `generate repo`: the port the application depends on, and the JDBC
//! adapter that implements it.
//!
//! Which adapter carries the bean is decided in `repository_wiring`, and it
//! is not a style choice: `JdbcClient` lives in spring-jdbc, so without the
//! starter the type does not exist and the adapter would not compile.

use super::*;

// ---- repo: a port the application depends on, and the JDBC adapter that
// implements it. The one pattern java.md names by name. ----

pub(super) fn repository_port(pkg: &str, name: &str, extra: &str) -> String {
    let var = lower_first(name);
    format!(
        r#"package {pkg};

{extra}import java.util.List;
import java.util.Optional;

/**
 * Storage for {{@link {name}}}, as the application sees it.
 *
 * <p>A port: no JDBC types, no driver, no dialect. Application code depends on
 * this interface, an adapter implements it, and a test can supply an in-memory
 * one without a database anywhere in sight.
 *
 * <p>{{@code findById}} returns {{@link Optional}} rather than null, and
 * {{@code findAll}} an empty list rather than null, so no caller has to guard.
 */
public interface {name}Repository {{

    Optional<{name}> findById(String id);

    List<{name}> findAll();

    /** Inserts a row. Define conflict behavior explicitly in the SQL adapter. */
    void save({name} {var});

    /** @return true when a row was actually removed. */
    boolean deleteById(String id);
}}
"#
    )
}

/// The JDBC adapter. When the caller knows the record's components (every
/// path except a bare `generate repo` on a type jails has never seen), the
/// SQL, the bind and the row mapper are all derived from them and there is
/// nothing left to fill in. `columns` empty falls back to the old shape --
/// `select *`, and a `map`/`bind` pair that throws -- because inventing
/// columns for a type whose fields are unknown would be worse than saying so.
/// Which repository adapter shape this project gets, and which adapter is the
/// bean.
///
/// `JdbcClient` lives in `spring-jdbc`, so a plain Maven project genuinely
/// cannot have it -- there the caller-owned `Connection` adapter is not a
/// second-best choice, it is the only one. Where Spring *is* present the
/// named-parameter, injectable version wins on both counts that matter: it
/// cannot swap two same-typed columns, and it is an ordinary bean.
///
/// The second half is the part that is easy to get wrong. A `JdbcClient`
/// adapter carrying `@Component` and an in-memory adapter carrying
/// `@Component` make **two** beans qualify for one injection point, and
/// Spring refuses to choose -- a scaffold that compiles and cannot start,
/// which is precisely the failure `jails beans` exists to report. So exactly
/// one of them is annotated, and which one depends on whether this project
/// has a database yet:
///
/// - **`add db` has run** (the JDBC starter is present, so auto-configuration
///   will produce a `JdbcClient`): the JDBC adapter is the bean, and the
///   in-memory one is written as a plain fake with no annotation. That is
///   also its honest role at that point -- a fake for tests, not a stand-in
///   for the database that now exists.
/// - **No database yet**: `spring-jdbc` is not even on the classpath, so
///   `JdbcClient` is not a type that exists -- the adapter is the plain
///   caller-owned `Connection` one, it is not a bean, and the in-memory
///   adapter keeps `@Component` so the application still starts. This is
///   what makes `g scaffold Note` then `jails run` work with nothing else
///   installed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum RepositoryWiring {
    /// Spring *and* `spring-boot-starter-jdbc`: `JdbcClient` adapter, and it
    /// is the bean.
    JdbcClientBean,
    /// Anything else: the caller-owned `Connection` adapter, which needs
    /// nothing but the JDK. On Spring the in-memory adapter is the bean.
    PlainJdbc,
}

pub(super) fn repository_wiring(project: &crate::model::Project) -> RepositoryWiring {
    if !matches!(project.flavor(), crate::pom::Flavor::SpringBoot) {
        return RepositoryWiring::PlainJdbc;
    }
    // The starter is what brings `JdbcClientAutoConfiguration` in. Checking
    // for it rather than for `compose.yaml` or a migration directory means
    // the answer matches what Spring will actually do at startup.
    if project.has_dependency("org.springframework.boot", "spring-boot-starter-jdbc") {
        RepositoryWiring::JdbcClientBean
    } else {
        // `JdbcClient` lives in spring-jdbc, which the starter brings in.
        // Without it the type does not exist and the adapter would not
        // compile -- so this is not a stylistic fallback, it is the only
        // adapter that can be written.
        RepositoryWiring::PlainJdbc
    }
}

pub(super) fn jdbc_repository_for(
    project: &crate::model::Project,
    pkg: &str,
    name: &str,
    extra: &str,
    columns: &[crate::sql::Column],
    owner: &str,
) -> String {
    match repository_wiring(project) {
        RepositoryWiring::JdbcClientBean => {
            jdbc_client_repository(pkg, name, extra, columns, owner)
        }
        RepositoryWiring::PlainJdbc => jdbc_repository(pkg, name, extra, columns, owner),
    }
}

#[derive(Clone, Copy)]
enum RepositoryKey<'a> {
    Single(Option<&'a crate::sql::Column>),
    Composite,
}

/// The key exposed through the repository port.
///
/// A declared single-column primary key wins over naming convention. With no
/// declaration, `id` and then the first mapped component retain the historical
/// fallback. A composite key is deliberately not collapsed to one component:
/// the current `String id` port cannot represent it without lying.
fn repository_key(columns: &[crate::sql::Column]) -> RepositoryKey<'_> {
    let declared = columns
        .iter()
        .filter(|column| column.constraints.primary_key)
        .collect::<Vec<_>>();
    match declared.as_slice() {
        [key] => RepositoryKey::Single(Some(*key)),
        [_, _, ..] => RepositoryKey::Composite,
        [] => RepositoryKey::Single(
            columns
                .iter()
                .filter(|column| column.mapped())
                .find(|column| column.name == "id")
                .or_else(|| columns.iter().find(|column| column.mapped())),
        ),
    }
}

/// The Spring flavour of the same adapter, over `JdbcClient` with **named**
/// parameters.
///
/// Two things make this the right default wherever Spring is present, and
/// both are failure modes rather than preferences:
///
/// - **Named parameters.** A positional `?` list is a silent-swap bug waiting
///   for a schema change: reorder two same-typed columns in the insert and
///   nothing fails to compile, nothing throws, and the data is wrong. A
///   seven-column insert has forty-two ways to be subtly wrong and one to be
///   right.
/// - **It is a bean.** The plain-JDBC adapter takes a `Connection` the caller
///   owns, which is why it cannot carry `@Component` and why `scaffold` has
///   to ship an in-memory adapter alongside it just so the context starts.
///   `JdbcClient` is injected, so the adapter is an ordinary bean and that
///   whole dance disappears.
///
/// The row mapper is shared with the plain-JDBC template: `JdbcClient` hands
/// a `ResultSet` to a `RowMapper` just the same, so `sql::Column::read` needs
/// no second form.
pub(super) fn jdbc_client_repository(
    pkg: &str,
    name: &str,
    extra: &str,
    columns: &[crate::sql::Column],
    owner: &str,
) -> String {
    let var = lower_first(name);
    let table = crate::sql::table_name(name);
    let mapped: Vec<&crate::sql::Column> = columns.iter().filter(|c| c.mapped()).collect();
    let derived = !mapped.is_empty();

    // One column list feeds the select, the insert, the bind and the mapper.
    // That is the entire reason this is generated rather than written.
    let select_list = if derived {
        mapped
            .iter()
            .map(|c| format!("            {},", c.name))
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end_matches(',')
            .to_string()
    } else {
        "            *".to_string()
    };
    let key_choice = repository_key(columns);
    let composite_key = matches!(key_choice, RepositoryKey::Composite);
    let key = match key_choice {
        RepositoryKey::Single(key) => key.filter(|column| column.mapped()),
        RepositoryKey::Composite => mapped.first().copied(),
    };
    let id_column = key
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "id".to_string());
    // The port takes a String id, so a non-text key column needs the cast
    // spelled out -- Postgres will not compare a uuid column to a text
    // parameter on its own.
    let key_placeholder = match key {
        Some(column) if column.sql_type != "text" => {
            format!("cast(:id as {})", column.sql_type)
        }
        _ => ":id".to_string(),
    };
    let key_note = if composite_key {
        " * <p>The declared primary key is composite. The current {@code String id} port cannot\n * represent it, so the two single-key operations fail explicitly until the port is modelled.\n"
            .to_string()
    } else {
        match key {
            Some(column) if column.name != "id" => format!(
                " * <p>Repository lookups are keyed on the {{@code {}}} component.\n",
                column.name
            ),
            _ => String::new(),
        }
    };
    let insert_columns = if derived {
        mapped
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        "id".to_string()
    };
    let placeholders = if derived {
        mapped
            .iter()
            .map(|c| format!(":{}", c.name))
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        ":id".to_string()
    };
    let mut sql_imports = crate::sql::imports(columns)
        .into_iter()
        .map(|i| format!("import {i};\n"))
        .collect::<String>();
    for column in &mapped {
        if builtin_by_java_name(&column.java_type).is_none() {
            sql_imports.push_str(&import_of(pkg, owner, &column.java_type));
        }
    }

    let map_body = if derived {
        let args = mapped
            .iter()
            .map(|c| format!("                {}", c.read.as_deref().unwrap_or("null")))
            .collect::<Vec<_>>()
            .join(",\n");
        format!("        return new {name}(\n{args});")
    } else {
        format!(
            "        throw new UnsupportedOperationException(\"TODO: map a {table} row to {name}\");"
        )
    };
    // `.param(name, value)` rather than a positional list: the binding and
    // the statement name the same thing, so they cannot drift apart.
    let bind_body = if derived {
        mapped
            .iter()
            .map(|c| {
                format!(
                    "                .param(\"{}\", {})",
                    c.name,
                    c.write.as_deref().unwrap_or("null")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        format!("                .param(\"id\", {var}.toString())")
    };
    let find_by_id_body = if composite_key {
        "        throw new UnsupportedOperationException(\"findById requires a composite-key repository port\");"
            .to_string()
    } else {
        format!(
            r#"        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        select %s
                        from {table}
                        where {id_column} = {key_placeholder}
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(Jdbc{name}Repository::map)
                .optional();"#
        )
    };
    let delete_by_id_body = if composite_key {
        "        throw new UnsupportedOperationException(\"deleteById requires a composite-key repository port\");"
            .to_string()
    } else {
        format!(
            r#"        Objects.requireNonNull(id, "id is required");
        return db.sql("""
                        delete from {table}
                        where {id_column} = {key_placeholder}
                        """)
                .param("id", id)
                .update()
                > 0;"#
        )
    };

    let unmapped: Vec<&str> = columns
        .iter()
        .filter(|c| !c.mapped())
        .map(|c| c.name.as_str())
        .collect();
    let doc_note = if !derived {
        " * <p>The statements are yours to finish: this adapter was generated without a\n * field spec, so jails knows the columns of exactly nothing.\n".to_string()
    } else if unmapped.is_empty() {
        " * <p>The SQL, the bind and the row mapper are all derived from the same field\n * spec, so they cannot disagree about a column name or a type.\n".to_string()
    } else {
        format!(
            " * <p>The SQL, the bind and the row mapper are derived from the field spec.\n\
             * Not persisted, because jails has no mapping for the type: {}.\n\
             * Add those columns by hand, or model them as their own table.\n",
            unmapped.join(", ")
        )
    };

    format!(
        r#"package {pkg};

{extra}{sql_imports}import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;

/**
 * {{@link {name}Repository}} over {{@link JdbcClient}}. No ORM: the queries are
 * visible, and the only abstraction is a named parameter.
 *
 * <p>Parameters are named rather than positional on purpose. A {{@code ?}} list
 * is a silent-swap bug waiting for a schema change -- reorder two columns of
 * the same type and nothing fails to compile and nothing throws.
 *
{doc_note}{key_note} */
@Component
public final class Jdbc{name}Repository implements {name}Repository {{

    /**
     * One column list, shared by the select, the insert and the row mapper.
     * A hand-maintained pair drifts -- {{@code amount}} in the insert against
     * {{@code amount_minor}} in the select compiles fine and fails at runtime.
     */
    private static final String COLUMNS =
            """
{select_list}
            """;

    private final JdbcClient db;

    public Jdbc{name}Repository(JdbcClient db) {{
        this.db = Objects.requireNonNull(db, "db is required");
    }}

    @Override
    public Optional<{name}> findById(String id) {{
{find_by_id_body}
    }}

    @Override
    public List<{name}> findAll() {{
        // Ordered explicitly: SQL does not otherwise promise row order.
        return db.sql("""
                        select %s
                        from {table}
                        order by {id_column}
                        """.formatted(COLUMNS))
                .query(Jdbc{name}Repository::map)
                .list();
    }}

    @Override
    public void save({name} {var}) {{
        Objects.requireNonNull({var}, "{var} is required");
        db.sql("""
                        insert into {table} ({insert_columns})
                        values ({placeholders})
                        """)
{bind_body}
                .update();
    }}

    @Override
    public boolean deleteById(String id) {{
{delete_by_id_body}
    }}

    /** Builds a {name} from the current row. */
    private static {name} map(ResultSet rows, int rowNumber) throws SQLException {{
{map_body}
    }}
}}
"#
    )
}

pub(super) fn jdbc_repository(
    pkg: &str,
    name: &str,
    extra: &str,
    columns: &[crate::sql::Column],
    // Where the record's own types live, so a project enum the mapper calls
    // `valueOf` on can be imported. Without this the adapter compiles only
    // when it happens to sit in the same package as the enum.
    owner: &str,
) -> String {
    let var = lower_first(name);
    let table = crate::sql::table_name(name);
    let mapped: Vec<&crate::sql::Column> = columns.iter().filter(|c| c.mapped()).collect();
    let derived = !mapped.is_empty();

    // Every SQL fragment below is built from the same column list, which is
    // the whole point: a hand-written adapter drifts (an `amount` in the
    // insert against an `amount_minor` in the select compiles and fails at
    // runtime), and one list cannot disagree with itself.
    let select_list = if derived {
        mapped
            .iter()
            .map(|c| format!("                {},", c.name))
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end_matches(',')
            .to_string()
    } else {
        "                *".to_string()
    };
    // Use the same declared-key policy as the Spring adapter and its generated
    // integration test. Three local conventions can otherwise make the test
    // exercise a different column from the adapter it claims to prove.
    let key_choice = repository_key(columns);
    let composite_key = matches!(key_choice, RepositoryKey::Composite);
    let key = match key_choice {
        RepositoryKey::Single(key) => key.filter(|column| column.mapped()),
        RepositoryKey::Composite => mapped.first().copied(),
    };
    let id_column = key
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "id".to_string());
    // The port takes a String id, so a non-text key column needs the cast
    // spelled out -- Postgres will not compare a uuid column to a text
    // parameter on its own.
    let key_placeholder = match key {
        Some(column) if column.sql_type != "text" => {
            format!("cast(? as {})", column.sql_type)
        }
        _ => "?".to_string(),
    };
    let key_note = if composite_key {
        " * <p>The declared primary key is composite. The current {@code String id} port cannot\n              * represent it, so the two single-key operations fail explicitly until the port is modelled.\n"
            .to_string()
    } else {
        match key {
            Some(column) if column.name != "id" => format!(
                " * <p>Repository lookups are keyed on the {{@code {}}} component.\n",
                column.name
            ),
            _ => String::new(),
        }
    };
    let insert_columns = if derived {
        mapped
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        "id".to_string()
    };
    let placeholders = if derived {
        vec!["?"; mapped.len()].join(", ")
    } else {
        "?".to_string()
    };
    let mut sql_imports = crate::sql::imports(columns)
        .into_iter()
        .map(|i| format!("import {i};\n"))
        .collect::<String>();
    // A mapped column whose type is not in jails' table is a project enum
    // (nothing else maps), and the mapper names it directly.
    for column in &mapped {
        if builtin_by_java_name(&column.java_type).is_none() {
            sql_imports.push_str(&import_of(pkg, owner, &column.java_type));
        }
    }

    let map_body = if derived {
        let args = mapped
            .iter()
            .map(|c| format!("                {}", c.read.as_deref().unwrap_or("null")))
            .collect::<Vec<_>>()
            .join(",\n");
        format!("        return new {name}(\n{args});")
    } else {
        format!(
            "        throw new UnsupportedOperationException(\"TODO: map a {table} row to {name}\");"
        )
    };
    let bind_body = if derived {
        mapped
            .iter()
            .enumerate()
            .map(|(index, c)| {
                format!(
                    "        insert.setObject({}, {});",
                    index + 1,
                    c.write.as_deref().unwrap_or("null")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        format!(
            "        throw new UnsupportedOperationException(\"TODO: bind {name} to the insert\");"
        )
    };
    let find_by_id_body = if composite_key {
        "        throw new UnsupportedOperationException(\"findById requires a composite-key repository port\");"
            .to_string()
    } else {
        format!(
            r#"        Objects.requireNonNull(id, "id is required");
        try (var query = connection.prepareStatement(FIND_BY_ID)) {{
            query.setString(1, id);
            try (var rows = query.executeQuery()) {{
                return rows.next() ? Optional.of(map(rows)) : Optional.empty();
            }}
        }} catch (SQLException error) {{
            throw new IllegalStateException("could not read {table} " + id, error);
        }}"#
        )
    };
    let delete_by_id_body = if composite_key {
        "        throw new UnsupportedOperationException(\"deleteById requires a composite-key repository port\");"
            .to_string()
    } else {
        format!(
            r#"        Objects.requireNonNull(id, "id is required");
        try (var delete = connection.prepareStatement(DELETE_BY_ID)) {{
            delete.setString(1, id);
            return delete.executeUpdate() > 0;
        }} catch (SQLException error) {{
            throw new IllegalStateException("could not delete from {table} " + id, error);
        }}"#
        )
    };

    // Anything jails could not map is named rather than quietly dropped --
    // the adapter still compiles, but it does not pretend to persist a
    // column it has no mapping for.
    let unmapped: Vec<&str> = columns
        .iter()
        .filter(|c| !c.mapped())
        .map(|c| c.name.as_str())
        .collect();
    let doc_note = if !derived {
        " * <p>{@link #map} and {@link #bind} are yours to finish: this adapter was \n\
             * generated without a field spec, so jails knows the columns of exactly nothing.\n"
            .to_string()
    } else if unmapped.is_empty() {
        " * <p>The SQL, the bind and the row mapper are all derived from the same field
 * spec, so they cannot disagree about a column name or a type.
"
        .to_string()
    } else {
        format!(
            " * <p>The SQL, the bind and the row mapper are derived from the field spec.\n\
             * Not persisted, because jails has no mapping for the type: {}.\n\
             * Add those columns by hand, or model them as their own table.\n",
            unmapped.join(", ")
        )
    };

    format!(
        r#"package {pkg};

{extra}{sql_imports}import java.sql.Connection;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import java.util.Optional;

/**
 * {{@link {name}Repository}} over plain JDBC. No ORM: the queries are visible,
 * and a {{@code PreparedStatement}} is the whole abstraction.
 *
 * <p>The caller owns the {{@link Connection}} -- this class neither opens nor
 * closes it, so one transaction can span several repositories.
 *
{doc_note}{key_note} */
public final class Jdbc{name}Repository implements {name}Repository {{

    private static final String FIND_BY_ID =
            """
            select
{select_list}
            from {table}
            where {id_column} = {key_placeholder}
            """;
    private static final String FIND_ALL =
            """
            select
{select_list}
            from {table}
            order by {id_column}
            """;
    private static final String INSERT =
            """
            insert into {table} ({insert_columns})
            values ({placeholders})
            """;
    private static final String DELETE_BY_ID =
            """
            delete from {table}
            where {id_column} = {key_placeholder}
            """;

    private final Connection connection;

    public Jdbc{name}Repository(Connection connection) {{
        this.connection = Objects.requireNonNull(connection, "connection is required");
    }}

    @Override
    public Optional<{name}> findById(String id) {{
{find_by_id_body}
    }}

    @Override
    public List<{name}> findAll() {{
        // Ordered explicitly: SQL does not otherwise promise row order.
        try (var query = connection.prepareStatement(FIND_ALL);
                var rows = query.executeQuery()) {{
            var all = new ArrayList<{name}>();
            while (rows.next()) {{
                all.add(map(rows));
            }}
            return List.copyOf(all);
        }} catch (SQLException error) {{
            throw new IllegalStateException("could not read {table}", error);
        }}
    }}

    @Override
    public void save({name} {var}) {{
        Objects.requireNonNull({var}, "{var} is required");
        try (var insert = connection.prepareStatement(INSERT)) {{
            bind(insert, {var});
            insert.executeUpdate();
        }} catch (SQLException error) {{
            throw new IllegalStateException("could not save to {table}", error);
        }}
    }}

    @Override
    public boolean deleteById(String id) {{
{delete_by_id_body}
    }}

    /** Builds a {name} from the current row. */
    private {name} map(ResultSet rows) throws SQLException {{
{map_body}
    }}

    /** Sets every column the insert above declares, in that order. */
    private void bind(java.sql.PreparedStatement insert, {name} {var}) throws SQLException {{
{bind_body}
    }}
}}
"#
    )
}

/// The honest fallback used when there is no field model to build and verify.
///
/// Kept as the two-argument entry point because the bare-repository unit test
/// deliberately exercises this case: an integration test must not pretend to
/// prove a mapper whose columns or record constructor jails cannot know.
pub(super) fn jdbc_repository_test(pkg: &str, name: &str) -> String {
    disabled_jdbc_repository_test(
        pkg,
        name,
        "todo: supply repository fields so jails can build a complete round-trip sample",
    )
}

/// A transactional PostgreSQL round trip when both sides are fully derived.
///
/// `fields` builds the record sample and `columns` is the exact projection the
/// adapter uses. Requiring every column to be mapped prevents a generated test
/// from blessing an adapter which silently omitted part of the record. The
/// Spring/JDBC gate keeps plain projects compilable: they do not have the
/// annotations or an injectable repository port this test requires.
#[allow(clippy::too_many_arguments)]
pub(super) fn jdbc_repository_test_for(
    project: &crate::model::Project,
    pkg: &str,
    domain: &str,
    repository: &str,
    name: &str,
    fields: &[Field],
    columns: &[crate::sql::Column],
) -> String {
    // The projection, not the directory. In an `app apply` the whole manifest
    // is one transition, so `add db`'s `TestcontainersConfig` is not on disk
    // when the scaffold that needs it plans -- and reading disk here emitted
    // every JDBC round trip `@Disabled` in exactly the projects that had asked
    // for a database. The package is read off the file rather than assumed to
    // be the base one, so a project that placed it elsewhere still imports it.
    let config_pkg = project
        .projected_test_sources()
        .iter()
        .find(|(path, _)| {
            path.file_stem().and_then(|stem| stem.to_str()) == Some("TestcontainersConfig")
        })
        .and_then(|(_, source)| jails_java::java::package_of(source));
    jdbc_repository_test_with_wiring(
        repository_wiring(project),
        config_pkg.as_deref(),
        project,
        pkg,
        domain,
        repository,
        name,
        fields,
        columns,
    )
}

#[allow(clippy::too_many_arguments)]
fn jdbc_repository_test_with_wiring(
    wiring: RepositoryWiring,
    testcontainers_pkg: Option<&str>,
    project: &crate::model::Project,
    pkg: &str,
    domain: &str,
    repository: &str,
    name: &str,
    fields: &[Field],
    columns: &[crate::sql::Column],
) -> String {
    if fields.is_empty() {
        return jdbc_repository_test(pkg, name);
    }
    if columns.len() != fields.len() {
        return disabled_jdbc_repository_test(
            pkg,
            name,
            "todo: make the repository columns match every record field before enabling this round trip",
        );
    }

    let unmapped = fields
        .iter()
        .zip(columns)
        .filter(|(_, column)| !column.mapped())
        .map(|(field, _)| field.name.as_str())
        .collect::<Vec<_>>();
    if !unmapped.is_empty() {
        return disabled_jdbc_repository_test(
            pkg,
            name,
            &format!(
                "todo: complete the JDBC mapping for {} before enabling this round trip",
                unmapped.join(", ")
            ),
        );
    }

    if wiring != RepositoryWiring::JdbcClientBean {
        return disabled_jdbc_repository_test(
            pkg,
            name,
            "todo: add PostgreSQL with jails add db before enabling this round trip",
        );
    }
    let Some(config_pkg) = testcontainers_pkg else {
        return disabled_jdbc_repository_test(
            pkg,
            name,
            "todo: generate TestcontainersConfig with jails add db before enabling this round trip",
        );
    };

    let key_column = match repository_key(columns) {
        RepositoryKey::Composite => {
            return disabled_jdbc_repository_test(
                pkg,
                name,
                "todo: model the composite repository key in the port before enabling this round trip",
            );
        }
        RepositoryKey::Single(Some(key)) => key,
        RepositoryKey::Single(None) => return jdbc_repository_test(pkg, name),
    };

    let sampled = fields
        .iter()
        .map(|field| sample_in_package(field, project, domain))
        .collect::<Vec<_>>();
    let unfabricable = fields
        .iter()
        .zip(&sampled)
        .filter(|(_, sample)| sample.is_none())
        .map(|(field, _)| field.name.as_str())
        .collect::<Vec<_>>();
    if !unfabricable.is_empty() {
        return disabled_jdbc_repository_test(
            pkg,
            name,
            &format!(
                "todo: supply a sample for {} -- jails cannot know how to build one",
                unfabricable.join(", ")
            ),
        );
    }

    let key_index = columns
        .iter()
        .position(|column| column.name == key_column.name)
        .expect("the selected repository key came from this column slice");
    let key_field = &fields[key_index];
    if key_field.optionality == Optionality::Nullable {
        return disabled_jdbc_repository_test(
            pkg,
            name,
            &format!(
                "todo: choose a required repository key -- {} is optional",
                key_field.name
            ),
        );
    }

    let mut imports = vec![
        "org.junit.jupiter.api.Test".to_string(),
        "org.springframework.beans.factory.annotation.Autowired".to_string(),
        "org.springframework.boot.test.context.SpringBootTest".to_string(),
        "org.springframework.transaction.annotation.Transactional".to_string(),
        "static org.assertj.core.api.Assertions.assertThat".to_string(),
    ];
    push_type_import(&mut imports, pkg, domain, name);
    push_type_import(&mut imports, pkg, repository, &format!("{name}Repository"));
    for field in fields {
        imports.extend(field.imports.iter().map(|import| (*import).to_string()));
        if field.owned {
            push_type_import(&mut imports, pkg, domain, &field.java_type);
        }
    }
    for (_, needed) in sampled.iter().flatten() {
        imports.extend(needed.iter().map(|import| (*import).to_string()));
    }
    if fields
        .iter()
        .any(|field| field.optionality == Optionality::Nullable)
    {
        imports.push("java.util.Optional".to_string());
    }

    imports.push("org.springframework.context.annotation.Import".to_string());
    push_type_import(&mut imports, pkg, config_pkg, "TestcontainersConfig");
    let mut annotations = String::from("@Import(TestcontainersConfig.class)\n");
    annotations.push_str("@SpringBootTest\n@Transactional\n");

    imports.sort();
    imports.dedup();
    let imports = imports
        .iter()
        .map(|import| format!("import {import};"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    let samples = sampled
        .iter()
        .map(|sample| sample.as_ref().unwrap().0.as_str())
        .collect::<Vec<_>>()
        .join(",\n                ");
    let var = lower_first(name);
    let key = if key_field.owned {
        format!("{var}.{}().name()", key_field.name)
    } else {
        format!("String.valueOf({var}.{}())", key_field.name)
    };
    let body = format!(
        r#"        var {var} = new {name}(
                {samples});
        repository.save({var});

        String key = {key};
        assertThat(repository.findById(key)).contains({var});
        assertThat(repository.findAll()).contains({var});

        assertThat(repository.deleteById(key)).isTrue();
        assertThat(repository.findById(key)).isEmpty();"#
    );
    let repository_field = format!("    @Autowired private {name}Repository repository;\n");

    crate::template::render(
        crate::template_here!("generate/jdbc_repository_test.java"),
        &[
            ("pkg", pkg),
            ("name", name),
            ("imports", &imports),
            ("annotations", &annotations),
            ("repository_field", &repository_field),
            ("body", &body),
        ],
    )
}

fn disabled_jdbc_repository_test(pkg: &str, name: &str, reason: &str) -> String {
    let imports = "import org.junit.jupiter.api.Disabled;\nimport org.junit.jupiter.api.Test;\n";
    let annotations = format!("@Disabled(\"{reason}\")\n");
    let body = format!("        throw new UnsupportedOperationException(\"{reason}\");");
    crate::template::render(
        crate::template_here!("generate/jdbc_repository_test.java"),
        &[
            ("pkg", pkg),
            ("name", name),
            ("imports", imports),
            ("annotations", &annotations),
            ("repository_field", ""),
            ("body", &body),
        ],
    )
}

fn push_type_import(imports: &mut Vec<String>, user: &str, owner: &str, class: &str) {
    if user != owner {
        imports.push(format!("{owner}.{class}"));
    }
}

#[cfg(test)]
mod repository_test_generation_tests {
    use super::*;

    fn mapped_columns(fields: &[Field]) -> Vec<crate::sql::Column> {
        fields
            .iter()
            .map(|field| crate::sql::Column {
                name: field.name.clone(),
                sql_type: "text".to_string(),
                not_null: field.optionality != Optionality::Nullable,
                read: Some("read".to_string()),
                write: Some("write".to_string()),
                java_type: field.java_type.clone(),
                constraints: field.constraints,
            })
            .collect()
    }

    #[test]
    fn complete_repository_test_exercises_the_transactional_port_contract() {
        let (root, project) =
            crate::spring::scratch_project("complete-repository-test", "<project></project>");
        let fields = parse_fields(&[
            "id:uuid".to_string(),
            "createdAt:instant".to_string(),
            "nickname:string?".to_string(),
        ])
        .unwrap();
        let columns = mapped_columns(&fields);
        let source = jdbc_repository_test_with_wiring(
            RepositoryWiring::JdbcClientBean,
            Some("com.example.demo"),
            &project,
            "com.example.demo.adapters",
            "com.example.demo.domain",
            "com.example.demo.app",
            "Transaction",
            &fields,
            &columns,
        );

        assert!(
            source.contains("@Import(TestcontainersConfig.class)"),
            "{source}"
        );
        assert!(source.contains("@SpringBootTest"), "{source}");
        assert!(source.contains("@Transactional"), "{source}");
        assert!(
            source.contains("@Autowired private TransactionRepository repository"),
            "{source}"
        );
        assert!(source.contains("UUID.fromString"), "{source}");
        assert!(source.contains("Instant.parse"), "{source}");
        assert!(source.contains("Optional.empty()"), "{source}");
        assert!(source.contains("repository.save(transaction)"), "{source}");
        assert!(source.contains("repository.findById(key)"), "{source}");
        assert!(source.contains("repository.findAll()"), "{source}");
        assert!(source.contains("repository.deleteById(key)"), "{source}");
        assert!(!source.contains("@Disabled"), "{source}");
        assert!(
            !source.contains("UnsupportedOperationException"),
            "{source}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn active_repository_test_requires_the_generated_container_config() {
        let (root, project) =
            crate::spring::scratch_project("repository-test-no-config", "<project></project>");
        let fields = parse_fields(&["id:uuid".to_string()]).unwrap();
        let columns = mapped_columns(&fields);

        let source = jdbc_repository_test_with_wiring(
            RepositoryWiring::JdbcClientBean,
            None,
            &project,
            "com.example.demo.adapters",
            "com.example.demo.domain",
            "com.example.demo.app",
            "Transaction",
            &fields,
            &columns,
        );

        assert!(source.contains("@Disabled"), "{source}");
        assert!(source.contains("generate TestcontainersConfig"), "{source}");
        assert!(!source.contains("@SpringBootTest"), "{source}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_declared_single_column_key_drives_the_adapter_and_round_trip() {
        let (root, project) =
            crate::spring::scratch_project("repository-test-declared-key", "<project></project>");
        let fields = parse_fields(&[
            "tenant:string".to_string(),
            "reference:string@pk".to_string(),
        ])
        .unwrap();
        let columns = mapped_columns(&fields);

        let adapter = jdbc_client_repository(
            "com.example.demo.adapters",
            "Transaction",
            "",
            &columns,
            "com.example.demo.domain",
        );
        let test = jdbc_repository_test_with_wiring(
            RepositoryWiring::JdbcClientBean,
            Some("com.example.demo"),
            &project,
            "com.example.demo.adapters",
            "com.example.demo.domain",
            "com.example.demo.app",
            "Transaction",
            &fields,
            &columns,
        );

        assert!(adapter.contains("where reference = :id"), "{adapter}");
        assert!(test.contains("transaction.reference()"), "{test}");
        assert!(!test.contains("@Disabled"), "{test}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_composite_key_is_refused_until_the_port_can_represent_it() {
        let (root, project) =
            crate::spring::scratch_project("repository-test-composite-key", "<project></project>");
        let fields = parse_fields(&[
            "tenant:string@pk".to_string(),
            "reference:string@pk".to_string(),
        ])
        .unwrap();
        let columns = mapped_columns(&fields);

        let adapter = jdbc_client_repository(
            "com.example.demo.adapters",
            "Transaction",
            "",
            &columns,
            "com.example.demo.domain",
        );
        let test = jdbc_repository_test_with_wiring(
            RepositoryWiring::JdbcClientBean,
            Some("com.example.demo"),
            &project,
            "com.example.demo.adapters",
            "com.example.demo.domain",
            "com.example.demo.app",
            "Transaction",
            &fields,
            &columns,
        );

        assert!(adapter.contains("findById requires a composite-key repository port"));
        assert!(adapter.contains("deleteById requires a composite-key repository port"));
        assert!(test.contains("@Disabled"), "{test}");
        assert!(
            test.contains("model the composite repository key"),
            "{test}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incomplete_repository_mapping_stays_honestly_disabled() {
        let (root, project) =
            crate::spring::scratch_project("incomplete-repository-test", "<project></project>");
        let fields =
            parse_fields(&["id:uuid".to_string(), "owner:UnknownOwner".to_string()]).unwrap();
        let mut columns = mapped_columns(&fields);
        columns[1].read = None;
        let source = jdbc_repository_test_with_wiring(
            RepositoryWiring::JdbcClientBean,
            None,
            &project,
            "com.example.demo.adapters",
            "com.example.demo.domain",
            "com.example.demo.app",
            "Transaction",
            &fields,
            &columns,
        );

        assert!(source.contains("@Disabled"), "{source}");
        assert!(source.contains("JDBC mapping for owner"), "{source}");
        assert!(source.contains("UnsupportedOperationException"), "{source}");
        assert!(!source.contains("@SpringBootTest"), "{source}");
        std::fs::remove_dir_all(root).unwrap();
    }
}
