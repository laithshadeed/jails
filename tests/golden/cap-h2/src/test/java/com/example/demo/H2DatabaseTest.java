package com.example.demo;

import static org.assertj.core.api.Assertions.assertThat;

import java.sql.Connection;
import javax.sql.DataSource;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;

/**
 * That the database is really there, and really is the test one.
 *
 * <p>Both halves matter and only the second is easy to get wrong. A capability
 * that installs a driver can be asserted by opening a connection; a capability
 * that also has to <em>keep the suite off the application's own database</em>
 * cannot, because inheriting the wrong URL fails nothing until the day a test
 * runs while the server is up and H2's file lock refuses it. So this asserts
 * the URL as well: if the test overlay stops being read, this is what says so.
 */
@SpringBootTest
class H2DatabaseTest {

    @Autowired private DataSource dataSource;

    @Test
    void theDatasourceConnectsAndItIsH2() throws Exception {
        try (Connection connection = dataSource.getConnection()) {
            assertThat(connection.getMetaData().getDatabaseProductName()).isEqualTo("H2");
            assertThat(connection.isValid(1)).isTrue();
        }
    }

    /**
     * The overlay is {@code src/test/resources/config/application.properties},
     * which outranks {@code classpath:/} and is additive -- unlike
     * {@code src/test/resources/application.properties}, which shadows the main
     * file wholesale and would silently unset everything else the application
     * needs.
     *
     * <p>The URL asserted here is the one the <em>driver</em> reports, which is
     * not the one that was configured: H2 drops everything after the first
     * {@code ;}, so the {@code DB_CLOSE_DELAY} setting is absent from this
     * string while being very much in effect.
     */
    @Test
    void theTestsUseTheirOwnInMemoryDatabase() throws Exception {
        try (Connection connection = dataSource.getConnection()) {
            assertThat(connection.getMetaData().getURL()).isEqualTo("jdbc:h2:mem:test");
        }
    }
}
