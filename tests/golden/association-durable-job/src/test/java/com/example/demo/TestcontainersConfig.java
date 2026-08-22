package com.example.demo;

import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.context.annotation.Bean;
import org.testcontainers.postgresql.PostgreSQLContainer;

/**
 * A real PostgreSQL for the tests that need one.
 *
 * <p>Import it on a test class that talks to the database:
 *
 * <pre>{@code
 * @SpringBootTest
 * @Import(TestcontainersConfig.class)
 * class RewardIngestionIT { ... }
 * }</pre>
 *
 * <p>{@code @ServiceConnection} publishes the container's JDBC url, username
 * and password to auto-configuration. Connection details take precedence over
 * {@code spring.datasource.*}, so the application's own settings do not need
 * to be overridden for tests.
 *
 * <p>Nothing calls {@code start()} -- Spring Boot starts the container bean.
 * The same container is shared by every application context in this test JVM,
 * because otherwise a suite with several {@code @SpringBootTest} classes pays
 * PostgreSQL's startup cost for every distinct context. Testcontainers' Ryuk
 * sidecar still removes it when the test JVM exits, so this is process-local
 * reuse rather than the cross-run reuse described below.
 *
 * <h2>Reuse, and why it is not on</h2>
 *
 * <p>Adding {@code .withReuse(true)} keeps the container alive between runs
 * and is the largest single saving available to a suite that starts
 * PostgreSQL. It is <em>not</em> generated, because it is only safe when this
 * is the only project on the machine using this image:
 *
 * <ul>
 *   <li><b>The reuse key is a hash of the container's configuration</b>, and
 *       nothing in that configuration identifies the project. Two applications
 *       on the same PostgreSQL image therefore reuse the <em>same database</em>
 *       -- and since both number their migrations from {@code V001}, Flyway
 *       refuses to start with a checksum mismatch against the other one's
 *       history. Jails' own verification gate hit exactly this.
 *   <li>A reused container is deliberately not registered with Ryuk, so
 *       nothing reaps it; they accumulate until something removes them.
 *   <li>The database keeps its state. Every database test Jails generates is
 *       transactional and rolls back, so that part is safe -- but a test you
 *       add that assumes an empty table will pass once and fail on the second
 *       run.
 * </ul>
 *
 * <p>If this is your only such project and you want the saving: add
 * {@code .withReuse(true)} below, and run {@code jails setup}, which writes
 * the {@code testcontainers.reuse.enable=true} that the machine -- not the
 * classpath -- has to carry. {@code jails doctor} reports whether it is on
 * and counts what has been left running.
 */
@TestConfiguration(proxyBeanMethods = false)
public class TestcontainersConfig {

    private static final PostgreSQLContainer POSTGRES = new ProcessPostgres();

    @Bean
    @ServiceConnection
    PostgreSQLContainer postgresContainer() {
        return POSTGRES;
    }

    /**
     * Spring closes container beans with each application context. A static
     * instance alone therefore does not share anything. Keep this process-wide
     * instance alive and let Ryuk perform the real cleanup when the JVM exits.
     */
    private static final class ProcessPostgres extends PostgreSQLContainer {

        private ProcessPostgres() {
            super("postgres:17-alpine");
        }

        @Override
        public void stop() {
            // Deliberately process-scoped; Ryuk owns cleanup at JVM exit.
        }
    }
}
