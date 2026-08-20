package {{pkg}};

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
 * <p>Nothing calls {@code start()} -- a container that is a bean is started
 * and stopped with the application context.
 */
@TestConfiguration(proxyBeanMethods = false)
public class {{TESTCONTAINERS_CONFIG}} {

    @Bean
    @ServiceConnection
    PostgreSQLContainer postgresContainer() {
        return new PostgreSQLContainer("{{POSTGRES_IMAGE}}");
    }
}
