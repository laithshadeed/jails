package {{pkg}};

import java.nio.file.Path;
import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.SQLException;

/**
 * A SQLite database file. Connections come from {@code java.sql} -- the only
 * thing the driver dependency adds is the {@code jdbc:sqlite:} URL scheme.
 *
 * <p>Callers own the {@link Connection} and should use try-with-resources.
 */
public record {{class}}(Path file) {

    /**
     * A database that lives only for as long as the connection does. Each
     * {@link #open()} returns a *fresh, empty* in-memory database, which is
     * what makes it convenient for isolated tests.
     */
    public static {{class}} inMemory() {
        return new {{class}}(Path.of(":memory:"));
    }

    public Connection open() throws SQLException {
        return DriverManager.getConnection("jdbc:sqlite:" + file);
    }
}
