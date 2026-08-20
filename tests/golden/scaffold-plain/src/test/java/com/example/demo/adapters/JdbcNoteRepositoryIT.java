package com.example.demo.adapters;

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcNoteRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Disabled("todo: configure the test database and finish the repository SQL mapping")
class JdbcNoteRepositoryIT {

    @Test
    void roundTripsThroughTheRealDatabase() {
        throw new UnsupportedOperationException("todo");
    }
}
