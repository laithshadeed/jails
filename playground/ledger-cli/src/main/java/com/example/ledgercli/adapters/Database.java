package com.example.ledgercli.adapters;

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
public record Database(Path file) {

    /**
     * A database that lives only for as long as the connection does. Each
     * {@link #open()} returns a *fresh, empty* in-memory database, which is
     * what makes it convenient for isolated tests.
     */
    public static Database inMemory() {
        return new Database(Path.of(":memory:"));
    }

    public Connection open() throws SQLException {
        return DriverManager.getConnection("jdbc:sqlite:" + file);
    }
}
