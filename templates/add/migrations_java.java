package {{pkg}};

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.sql.Connection;
import java.sql.SQLException;
import java.util.ArrayList;
import java.util.List;

/**
 * Applies {@code .sql} files in filename order, exactly once each.
 *
 * <p>Applied scripts are recorded in a {@code schema_migrations} table, so
 * running this on every startup is safe: only new files do any work.
 */
public final class {{class}} {

    private static final String CREATE_TRACKING_TABLE =
            """
            create table if not exists schema_migrations (
                name text primary key,
                applied_at text not null default (datetime('now'))
            )
            """;

    private {{class}}() {}

    /**
     * Applies every not-yet-applied script in {@code dir}, returning the names
     * of the ones applied. A missing directory means no migrations, not an
     * error.
     */
    public static List<String> applyAll(Connection connection, Path dir) throws IOException, SQLException {
        try (var statement = connection.createStatement()) {
            statement.execute(CREATE_TRACKING_TABLE);
        }

        var applied = new ArrayList<String>();
        for (var script : scripts(dir)) {
            var name = script.getFileName().toString();
            if (!alreadyApplied(connection, name)) {
                apply(connection, name, Files.readString(script));
                applied.add(name);
            }
        }
        return List.copyOf(applied);
    }

    private static List<Path> scripts(Path dir) throws IOException {
        if (!Files.isDirectory(dir)) {
            return List.of();
        }
        try (var files = Files.list(dir)) {
            return files.filter(path -> path.getFileName().toString().endsWith(".sql")).sorted().toList();
        }
    }

    private static boolean alreadyApplied(Connection connection, String name) throws SQLException {
        try (var query = connection.prepareStatement("select 1 from schema_migrations where name = ?")) {
            query.setString(1, name);
            try (var rows = query.executeQuery()) {
                return rows.next();
            }
        }
    }

    /** Each script runs in one transaction, together with recording its name. */
    private static void apply(Connection connection, String name, String sql) throws SQLException {
        var autoCommit = connection.getAutoCommit();
        connection.setAutoCommit(false);
        try {
            try (var statement = connection.createStatement()) {
                // Simple splitter: fine for schema DDL, but it would break on a
                // semicolon inside a string literal or a trigger body.
                for (var command : sql.split(";")) {
                    if (!command.isBlank()) {
                        statement.execute(command);
                    }
                }
            }
            try (var insert = connection.prepareStatement("insert into schema_migrations(name) values (?)")) {
                insert.setString(1, name);
                insert.executeUpdate();
            }
            connection.commit();
        } catch (SQLException e) {
            connection.rollback();
            throw e;
        } finally {
            connection.setAutoCommit(autoCommit);
        }
    }
}
