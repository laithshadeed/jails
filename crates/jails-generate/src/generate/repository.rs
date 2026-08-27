//! `generate repo`: the port the application depends on, and the JDBC
//! adapter that implements it.
//!
//! Which adapter carries the bean is decided in `repository_wiring`, and it
//! is not a style choice: `JdbcClient` lives in spring-jdbc, so without the
//! starter the type does not exist and the adapter would not compile.

use super::*;

mod key;
pub(crate) use key::*;
use key::{RepositoryKey, boxed_key, repository_key};

// ---- repo: a port the application depends on, and the JDBC adapter that
// implements it. The one pattern java.md names by name. ----

pub(super) fn repository_port(
    pkg: &str,
    name: &str,
    extra: &str,
    key: &KeyType,
    assignment: crate::sql::Assignment,
) -> String {
    let var = lower_first(name);
    let key_import = &key.import;
    let key_java = &key.java;
    let save_note = match assignment {
        crate::sql::Assignment::DatabaseGenerated => {
            "The database assigns this table's key, so the returned value \n     * carries it and the argument does not."
        }
        _ => {
            "The application assigns this table's key, so the two are equal \n     * today; returning the stored row is what keeps a caller correct if \n     * that ever stops being true."
        }
    };
    format!(
        r#"package {pkg};

{extra}import java.util.List;
import java.util.Optional;
{key_import}
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

    Optional<{name}> findById({key_java} id);

    List<{name}> findAll();

    /**
     * Inserts a row and returns it as stored. Define conflict behavior
     * explicitly in the SQL adapter.
     *
     * <p>The return value is not the argument. {save_note}
     */
    {name} save({name} {var});

    /** @return true when a row was actually removed. */
    boolean deleteById({key_java} id);
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
    //
    // Through `has_jdbc`, which knows that `spring-boot-starter-data-jdbc`
    // declares the narrower one. Asking for the narrow name alone made a
    // Spring Data JDBC project get the in-memory adapter as its bean while a
    // generated query read the real table -- writes to a HashMap, reads from
    // an empty database, and nothing to say so.
    if project.has_jdbc() {
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
    // Derived here and handed to whichever adapter is chosen, so the two
    // shapes cannot disagree with each other or with the port. plan.md P3.3.
    let key = key_type(columns);
    match repository_wiring(project) {
        RepositoryWiring::JdbcClientBean => {
            jdbc_client_repository(pkg, name, extra, columns, owner, &key)
        }
        RepositoryWiring::PlainJdbc => jdbc_repository(pkg, name, extra, columns, owner, &key),
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
    key_type: &KeyType,
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
    // The cast exists for the *opaque* port only. With a typed key the
    // parameter is already the column's own type -- a `UUID` binds as `uuid`,
    // a `Long` as `bigint` -- and casting it would be noise. When the port
    // fell back to text, Postgres will not compare a uuid column to a text
    // parameter on its own, so the cast is spelled out. plan.md P3.3.
    let key_placeholder = match key {
        Some(column) if key_type.is_opaque() && column.sql_type != "text" => {
            format!("cast(:id as {})", column.sql_type)
        }
        _ => ":id".to_string(),
    };
    let key_java = &key_type.java;
    let key_import = &key_type.import;
    // Newest first where the table has a timestamp, the key only as the
    // tiebreak. `order by id` over a random UUID is a stable random order.
    // plan.md P4.4.
    let ordering = crate::sql::ordering(columns);
    let ordering_note = if ordering == id_column {
        "// Ordered explicitly: SQL does not otherwise promise row order, and
        // this table has no timestamp to order by."
    } else {
        "// Newest first, with the key as the tiebreak so two rows written in
        // the same instant do not swap between two identical requests."
    };
    let key_note = if composite_key {
        " * <p>The declared primary key is composite. The current {@code String id} port cannot\n * represent it, so the two single-key operations fail explicitly until the port is modelled.\n"
            .to_string()
    } else {
        match key {
            // The *component*, not the column. `userId` is `user_id` in the
            // table, and a Javadoc naming a component the record does not
            // declare sends the reader looking for an accessor that is not
            // there. plan.md P6.4.
            Some(column) if column.name != "id" => format!(
                " * <p>Repository lookups are keyed on the {{@code {}}} component.\n",
                column.component
            ),
            _ => String::new(),
        }
    };
    // A database-assigned key is not in the insert: the column is
    // `generated always as identity`, so naming it is the caller working
    // around the policy rather than exercising it. plan.md P4.2.
    let generated = crate::sql::generated_key(columns).map(|column| column.name.as_str());
    let inserted: Vec<&crate::sql::Column> = mapped
        .iter()
        .copied()
        .filter(|column| Some(column.name.as_str()) != generated)
        .collect();
    let insert_columns = if derived {
        inserted
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        "id".to_string()
    };
    let placeholders = if derived {
        inserted
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
        inserted
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
    // **`getGeneratedKeys`, not `returning`.** `insert ... returning` is one
    // round trip and PostgreSQL-only; H2 has no such clause in its parser at
    // all, and `Project::sql_dialect` treats H2 as a supported target. JDBC's
    // generated-key retrieval works on both, and rebuilding the record around
    // the key costs nothing because every other component is already in hand.
    let save_body = match (derived, generated) {
        (true, Some(key)) => {
            let arguments = columns
                .iter()
                .map(|column| {
                    if column.name == key {
                        format!(
                            "                keys.getKeyAs({}.class)",
                            boxed_key(&column.java_type)
                        )
                    } else {
                        format!("                {var}.{}()", column.component)
                    }
                })
                .collect::<Vec<_>>()
                .join(",\n");
            format!(
                "        Objects.requireNonNull({var}, \"{var} is required\");\n\
                 \x20       var keys = new GeneratedKeyHolder();\n\
                 \x20       db.sql(\"\"\"\n\
                 \x20                       insert into {table} ({insert_columns})\n\
                 \x20                       values ({placeholders})\n\
                 \x20                       \"\"\")\n\
                 {bind_body}\n\
                 \x20               .update(keys, \"{key}\");\n\
                 \x20       return new {name}(\n\
                 {arguments});"
            )
        }
        _ => format!(
            "        Objects.requireNonNull({var}, \"{var} is required\");\n\
             \x20       db.sql(\"\"\"\n\
             \x20                       insert into {table} ({insert_columns})\n\
             \x20                       values ({placeholders})\n\
             \x20                       \"\"\")\n\
             {bind_body}\n\
             \x20               .update();\n\
             \x20       return {var};"
        ),
    };
    let key_holder_import = if derived && generated.is_some() {
        "import org.springframework.jdbc.support.GeneratedKeyHolder;\n"
    } else {
        ""
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

{extra}{sql_imports}{key_import}{key_holder_import}import java.sql.ResultSet;
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
    public Optional<{name}> findById({key_java} id) {{
{find_by_id_body}
    }}

    @Override
    public List<{name}> findAll() {{
        {ordering_note}
        return db.sql("""
                        select %s
                        from {table}
                        order by {ordering}
                        """.formatted(COLUMNS))
                .query(Jdbc{name}Repository::map)
                .list();
    }}

    @Override
    public {name} save({name} {var}) {{
{save_body}
    }}

    @Override
    public boolean deleteById({key_java} id) {{
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
    key_type: &KeyType,
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
    // The cast exists for the *opaque* port only. With a typed key the
    // parameter is already the column's own type -- a `UUID` binds as `uuid`,
    // a `Long` as `bigint` -- and casting it would be noise. When the port
    // fell back to text, Postgres will not compare a uuid column to a text
    // parameter on its own, so the cast is spelled out. plan.md P3.3.
    let key_placeholder = match key {
        Some(column) if key_type.is_opaque() && column.sql_type != "text" => {
            format!("cast(? as {})", column.sql_type)
        }
        _ => "?".to_string(),
    };
    let key_java = &key_type.java;
    let key_import = &key_type.import;
    // Newest first where the table has a timestamp, the key only as the
    // tiebreak. `order by id` over a random UUID is a stable random order.
    // plan.md P4.4.
    let ordering = crate::sql::ordering(columns);
    let ordering_note = if ordering == id_column {
        "// Ordered explicitly: SQL does not otherwise promise row order, and
        // this table has no timestamp to order by."
    } else {
        "// Newest first, with the key as the tiebreak so two rows written in
        // the same instant do not swap between two identical requests."
    };
    let key_note = if composite_key {
        " * <p>The declared primary key is composite. The current {@code String id} port cannot\n              * represent it, so the two single-key operations fail explicitly until the port is modelled.\n"
            .to_string()
    } else {
        match key {
            // The *component*, not the column. `userId` is `user_id` in the
            // table, and a Javadoc naming a component the record does not
            // declare sends the reader looking for an accessor that is not
            // there. plan.md P6.4.
            Some(column) if column.name != "id" => format!(
                " * <p>Repository lookups are keyed on the {{@code {}}} component.\n",
                column.component
            ),
            _ => String::new(),
        }
    };
    // Same rule as the `JdbcClient` adapter: the identity column is not in
    // the insert. plan.md P4.2.
    let generated = crate::sql::generated_key(columns).map(|column| column.name.as_str());
    let inserted: Vec<&crate::sql::Column> = mapped
        .iter()
        .copied()
        .filter(|column| Some(column.name.as_str()) != generated)
        .collect();
    let insert_columns = if derived {
        inserted
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        "id".to_string()
    };
    let placeholders = if derived {
        vec!["?"; inserted.len()].join(", ")
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
        inserted
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
    // `Statement.RETURN_GENERATED_KEYS` is the portable half of the same
    // decision the `JdbcClient` adapter makes: no `returning` clause, because
    // H2 has none.
    let (prepare_arguments, save_result) = match (derived, generated) {
        (true, Some(key)) => {
            let arguments = columns
                .iter()
                .map(|column| {
                    if column.name == key {
                        format!(
                            "                        keys.getObject(1, {}.class)",
                            boxed_key(&column.java_type)
                        )
                    } else {
                        format!("                        {var}.{}()", column.component)
                    }
                })
                .collect::<Vec<_>>()
                .join(",\n");
            (
                ", Statement.RETURN_GENERATED_KEYS",
                format!(
                    "            try (var keys = insert.getGeneratedKeys()) {{\n\
                     \x20               if (!keys.next()) {{\n\
                     \x20                   throw new IllegalStateException(\"{table} assigned no key\");\n\
                     \x20               }}\n\
                     \x20               return new {name}(\n\
                     {arguments});\n\
                     \x20           }}"
                ),
            )
        }
        _ => ("", format!("            return {var};")),
    };
    let statement_import = if prepare_arguments.is_empty() {
        ""
    } else {
        "import java.sql.Statement;\n"
    };
    // `setString` binds text; a typed key binds as itself. `setObject` is
    // the one setter that takes both a `UUID` and a `Long`, and pgjdbc maps
    // each to the column type the statement expects. plan.md P3.3.
    let key_setter = if key_type.is_opaque() {
        "setString"
    } else {
        "setObject"
    };
    let find_by_id_body = if composite_key {
        "        throw new UnsupportedOperationException(\"findById requires a composite-key repository port\");"
            .to_string()
    } else {
        format!(
            r#"        Objects.requireNonNull(id, "id is required");
        try (var query = connection.prepareStatement(FIND_BY_ID)) {{
            query.{key_setter}(1, id);
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
            delete.{key_setter}(1, id);
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

{extra}{sql_imports}{key_import}import java.sql.Connection;
{statement_import}
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
            order by {ordering}
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
    public Optional<{name}> findById({key_java} id) {{
{find_by_id_body}
    }}

    @Override
    public List<{name}> findAll() {{
        {ordering_note}
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
    public {name} save({name} {var}) {{
        Objects.requireNonNull({var}, "{var} is required");
        try (var insert = connection.prepareStatement(INSERT{prepare_arguments})) {{
            bind(insert, {var});
            insert.executeUpdate();
{save_result}
        }} catch (SQLException error) {{
            throw new IllegalStateException("could not save to {table}", error);
        }}
    }}

    @Override
    public boolean deleteById({key_java} id) {{
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
pub(super) fn jdbc_repository_test_for(
    project: &crate::model::Project,
    subject: &Subject<'_>,
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
        subject,
    )
}

/// The record a repository is for, and everything derived from it together.
///
/// Six values that are computed together and consumed together: the three
/// packages the test has to import across, the record's name, its components
/// and the exact column projection the adapter uses. They were six positional
/// parameters, and the two `&str` triples in the middle were an ordering
/// nothing but a compile error over `&str` versus `&str` would catch -- which
/// is to say nothing.
pub(super) struct Subject<'a> {
    pub(super) pkg: &'a str,
    pub(super) domain: &'a str,
    pub(super) repository: &'a str,
    pub(super) name: &'a str,
    pub(super) fields: &'a [Field],
    pub(super) columns: &'a [crate::sql::Column],
}

fn jdbc_repository_test_with_wiring(
    wiring: RepositoryWiring,
    testcontainers_pkg: Option<&str>,
    project: &crate::model::Project,
    subject: &Subject<'_>,
) -> String {
    let Subject {
        pkg,
        domain,
        repository,
        name,
        fields,
        columns,
    } = *subject;
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
    // The port is keyed on the component's own type now, so the round trip
    // hands it that value rather than a rendering of it. An opaque key is
    // still text -- an owned type reaches the port as its name. plan.md P3.3.
    let key_type = key_type(columns);
    let key = if key_field.owned {
        format!("{var}.{}().name()", key_field.name)
    } else if key_type.is_opaque() {
        format!("String.valueOf({var}.{}())", key_field.name)
    } else {
        format!("{var}.{}()", key_field.name)
    };
    let key_java = &key_type.java;
    // The *stored* row, not the argument. A database-assigned key means the
    // two differ by exactly the component this test then looks up, and the
    // sequence does not roll back with the transaction, so a literal key
    // passes once and fails on every later run. plan.md P4.2.
    let body = format!(
        r#"        var {var} = repository.save(new {name}(
                {samples}));

        {key_java} key = {key};
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
                dialect: jails_spec::spec::kind::Dialect::Postgres,
                name: field.column.clone(),
                component: field.name.clone(),
                sql_type: "text".to_string(),
                not_null: field.optionality != Optionality::Nullable,
                read: Some("read".to_string()),
                write: Some("write".to_string()),
                java_type: field.java_type.clone(),
                constraints: field.constraints,
                closed_set: Vec::new(),
                non_blank: false,
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
            &Subject {
                pkg: "com.example.demo.adapters",
                domain: "com.example.demo.domain",
                repository: "com.example.demo.app",
                name: "Transaction",
                fields: &fields,
                columns: &columns,
            },
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
        assert!(
            source.contains("repository.save(new Transaction("),
            "the round trip asserts on the stored row, not the argument: {source}"
        );
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
            &Subject {
                pkg: "com.example.demo.adapters",
                domain: "com.example.demo.domain",
                repository: "com.example.demo.app",
                name: "Transaction",
                fields: &fields,
                columns: &columns,
            },
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
            &key_type(&columns),
        );
        let test = jdbc_repository_test_with_wiring(
            RepositoryWiring::JdbcClientBean,
            Some("com.example.demo"),
            &project,
            &Subject {
                pkg: "com.example.demo.adapters",
                domain: "com.example.demo.domain",
                repository: "com.example.demo.app",
                name: "Transaction",
                fields: &fields,
                columns: &columns,
            },
        );

        assert!(adapter.contains("where reference = :id"), "{adapter}");
        assert!(test.contains("transaction.reference()"), "{test}");
        assert!(!test.contains("@Disabled"), "{test}");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The Javadoc names a component the record actually declares.
    ///
    /// It used to print the *column* name, so a key called `customerId` was
    /// announced as `customer_id` -- an accessor the reader then goes looking
    /// for and does not find. `modern.md` §11.2's point exactly: generated
    /// prose is asserted and never checked. plan.md P6.4.
    #[test]
    fn the_key_javadoc_names_the_component_not_the_column() {
        let (root, _project) =
            crate::spring::scratch_project("repository-test-key-doc", "<project></project>");
        let fields = parse_fields(&[
            "customerId:string@pk".to_string(),
            "tenant:string".to_string(),
        ])
        .unwrap();
        let columns = mapped_columns(&fields);

        let adapter = jdbc_client_repository(
            "com.example.demo.adapters",
            "Transaction",
            "",
            &columns,
            "com.example.demo.domain",
            &key_type(&columns),
        );
        let plain = jdbc_repository(
            "com.example.demo.adapters",
            "Transaction",
            "",
            &columns,
            "com.example.demo.domain",
            &key_type(&columns),
        );

        for source in [&adapter, &plain] {
            assert!(
                source.contains("keyed on the {@code customerId} component"),
                "{source}"
            );
            assert!(!source.contains("{@code customer_id}"), "{source}");
        }
        // The SQL is still the column, which is the half that was right.
        assert!(adapter.contains("where customer_id = :id"), "{adapter}");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A port that cannot take this type's key says so by failing.
    ///
    /// A composite key is the reachable case: the port takes one value, so
    /// there is nothing to key `items` on.
    ///
    /// The branch used to ship three methods quietly doing the wrong thing
    /// under a comment explaining why: `findById` answering `Optional.empty()`
    /// forever, `deleteById` removing a typed key from a `Map<String, ...>` so
    /// it was always `false`, and `save` keying on a counter that collides
    /// after any removal. `modern.md` §8.1 is what that reads like from
    /// outside -- and the JDBC adapter beside it already failed explicitly on
    /// the same input, which is the disagreement. plan.md P7.1.
    #[test]
    fn a_fake_that_cannot_be_keyed_fails_instead_of_answering_wrongly() {
        let fields = parse_fields(&[
            "tenant:string@pk".to_string(),
            "reference:string@pk".to_string(),
        ])
        .unwrap();
        let columns = mapped_columns(&fields);
        let java = crate::spring::in_memory_repository_java(
            "com.example.demo.adapters",
            "Ticket",
            "",
            &StoredKey::of(&fields, &columns, "Ticket"),
            true,
        );

        assert_eq!(
            java.matches("UnsupportedOperationException").count(),
            2,
            "{java}"
        );
        assert!(!java.contains("return Optional.empty();"), "{java}");
        assert!(!java.contains("items.remove(id)"), "{java}");
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
            &key_type(&columns),
        );
        let test = jdbc_repository_test_with_wiring(
            RepositoryWiring::JdbcClientBean,
            Some("com.example.demo"),
            &project,
            &Subject {
                pkg: "com.example.demo.adapters",
                domain: "com.example.demo.domain",
                repository: "com.example.demo.app",
                name: "Transaction",
                fields: &fields,
                columns: &columns,
            },
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
            &Subject {
                pkg: "com.example.demo.adapters",
                domain: "com.example.demo.domain",
                repository: "com.example.demo.app",
                name: "Transaction",
                fields: &fields,
                columns: &columns,
            },
        );

        assert!(source.contains("@Disabled"), "{source}");
        assert!(source.contains("JDBC mapping for owner"), "{source}");
        assert!(source.contains("UnsupportedOperationException"), "{source}");
        assert!(!source.contains("@SpringBootTest"), "{source}");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The invariant that keeps a scaffold able to *start*: exactly one
    /// adapter is a bean. Two makes Spring refuse to choose; zero leaves the
    /// service with no repository at all.
    #[test]
    fn exactly_one_repository_adapter_carries_the_bean_annotation() {
        let columns = crate::sql::columns(
            &parse_fields(&["id:string!".to_string(), "title:string".to_string()]).unwrap(),
            &crate::model::Project::inspect(Path::new("/tmp/does-not-matter")).unwrap(),
            "com.example.app.domain",
            "note",
        );
        let jdbc_bean = jdbc_client_repository(
            "com.example.app.adapters",
            "Note",
            "",
            &columns,
            "com.example.app.domain",
            &key_type(&columns),
        );
        let fields = parse_fields(&["id:string!".to_string(), "title:string".to_string()]).unwrap();
        let in_memory_fake = crate::spring::in_memory_repository_java(
            "com.example.app.adapters",
            "Note",
            "",
            &StoredKey::of(&fields, &columns, "Note"),
            false,
        );
        // The annotation on the declaration, not the word in the Javadoc.
        assert!(
            jdbc_bean.contains("@Component\npublic final class"),
            "{jdbc_bean}"
        );
        assert!(
            !in_memory_fake.contains("@Component\npublic class"),
            "the JDBC adapter is the bean here, so this one must not be: {in_memory_fake}"
        );
        assert!(
            !in_memory_fake.contains("import org.springframework.stereotype.Component;"),
            "an unused import would fail a strict build: {in_memory_fake}"
        );

        // ...and the other way round, before `add db` has run.
        let in_memory_bean = crate::spring::in_memory_repository_java(
            "com.example.app.adapters",
            "Note",
            "",
            &StoredKey::of(&fields, &columns, "Note"),
            true,
        );
        assert!(
            in_memory_bean.contains("@Component\npublic class"),
            "{in_memory_bean}"
        );
    }

    /// `spring.md` calls a positional `?` list in a multi-column insert a
    /// silent-swap bug waiting for a schema change, and the generator used to
    /// emit exactly that.
    #[test]
    fn the_spring_adapter_binds_by_name_and_shares_one_column_list() {
        let columns = crate::sql::columns(
            &parse_fields(&[
                "id:uuid".to_string(),
                "amount:long".to_string(),
                "currency:string".to_string(),
            ])
            .unwrap(),
            &crate::model::Project::inspect(Path::new("/tmp/does-not-matter")).unwrap(),
            "com.example.app.domain",
            "reward",
        );
        let src = jdbc_client_repository(
            "com.example.app.adapters",
            "Reward",
            "",
            &columns,
            "com.example.app.domain",
            &key_type(&columns),
        );
        assert!(src.contains("JdbcClient"), "{src}");
        assert!(!src.contains("PreparedStatement"), "{src}");
        // Named, not positional.
        assert!(src.contains(".param(\"amount\""), "{src}");
        assert!(src.contains(":amount"), "{src}");
        assert!(!src.contains("setObject("), "{src}");
        // One column list, interpolated into the reads.
        assert!(src.contains("private static final String COLUMNS"), "{src}");
        assert!(src.contains(".formatted(COLUMNS)"), "{src}");
    }

    /// The whole point of a port: application code must be able to depend on
    /// it without dragging JDBC along -- including in the prose, since a
    /// reader grepping for java.sql should find only the adapter.
    #[test]
    fn repository_port_is_free_of_jdbc() {
        let src = repository_port(
            "com.example.demo.app",
            "Transaction",
            "import com.example.demo.domain.Transaction;\n",
            &key_type(&[]),
            crate::sql::Assignment::ServerGenerated,
        );

        assert!(
            src.contains("public interface TransactionRepository"),
            "{src}"
        );
        assert!(
            src.contains("Optional<Transaction> findById(String id)"),
            "{src}"
        );
        assert!(src.contains("List<Transaction> findAll()"), "{src}");
        assert!(!src.contains("java.sql"), "not even in a comment: {src}");
    }

    #[test]
    fn jdbc_adapter_uses_plain_jdbc_and_no_orm() {
        let src = jdbc_repository(
            "com.example.demo.adapters",
            "Transaction",
            "",
            &[],
            "com.example.demo.domain",
            &key_type(&[]),
        );

        assert!(src.contains("implements TransactionRepository"), "{src}");
        assert!(src.contains("connection.prepareStatement"), "{src}");
        assert!(src.contains("try (var query"), "try-with-resources: {src}");
        // No field spec here, so there is no timestamp to order by and the
        // key is all there is. plan.md P4.4.
        assert!(
            src.contains("order by id"),
            "unordered findAll would flake a test: {src}"
        );
        assert!(
            src.contains("\"\"\""),
            "SQL should be visible in text blocks: {src}"
        );
        {
            let forbidden = "org.springframework";
            assert!(!src.contains(forbidden), "{forbidden} should not appear");
        }
    }

    /// jails cannot know the columns, so map/bind are TODOs -- and a test that
    /// asserts on a TODO is noise until they are written.
    #[test]
    fn jdbc_adapter_test_is_disabled_until_the_mapping_is_written() {
        let test = jdbc_repository_test("com.example.demo.adapters", "Transaction");

        assert!(test.contains("@Disabled"), "{test}");
        assert!(test.contains("class JdbcTransactionRepositoryIT"), "{test}");
        assert!(test.contains("roundTripsThroughTheRealDatabase"), "{test}");
    }
}
