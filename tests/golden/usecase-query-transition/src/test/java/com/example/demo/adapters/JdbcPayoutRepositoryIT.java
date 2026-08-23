package com.example.demo.adapters;

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {@link JdbcPayoutRepository}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Disabled("todo: add PostgreSQL with jails add db before enabling this round trip")
class JdbcPayoutRepositoryIT {

    @Test
    void roundTripsThroughTheRealDatabase() {
        throw new UnsupportedOperationException("todo: add PostgreSQL with jails add db before enabling this round trip");
    }
}
