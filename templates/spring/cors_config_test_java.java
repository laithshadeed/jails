package {{pkg}};

import static org.assertj.core.api.Assertions.assertThat;

import org.junit.jupiter.api.Test;
import java.util.List;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

/**
 * A preflight through the real dispatcher, which is the only place this
 * capability can be observed to work.
 *
 * <p>The test this replaced constructed {@code new CorsConfig()} and asserted
 * on the returned object. It passed while CORS was completely broken, because
 * the only failure mode is that <em>nothing reads the bean</em>: a
 * {@code CorsConfigurationSource} is consulted by Spring Security's filter
 * chain, and without Security there is no chain. The preflight was answered by
 * the dispatcher's default {@code OPTIONS} handler with 200, an {@code Allow}
 * header and no {@code Access-Control-Allow-Origin}, so the browser blocked
 * the real request and every server-side assertion still held.
 *
 * <p>The rule this is an instance of: a test that cannot observe the failure
 * and a test that never runs are the same bug.
 */
@SpringBootTest
@AutoConfigureMockMvc
class CorsConfigTest {

    /**
     * Any path will do. A preflight is answered by the CORS filter before
     * routing, so this asserts the policy rather than the existence of a
     * handler -- and using a path that does not resolve is what keeps this
     * test from breaking every time somebody adds or moves an endpoint.
     */
    private static final String ANY_PATH = "/any-path";

    /**
     * Read, not restated. The generated value is {@code https://example.invalid}
     * -- reserved by RFC 2606, so it can never resolve and is unmistakably a
     * setting somebody has to replace. A test that hardcoded it would go red on
     * the day it was replaced, which is a capability shipping a failing build
     * for being configured. The first origin is the one asserted; the list is
     * what the application actually allows.
     */
    @Value("${app.cors.allowed-origins}")
    private List<String> origins;

    @Autowired private MockMvcTester mvc;

    @Test
    void aPreflightFromADeclaredOriginIsAnswered() {
        assertThat(mvc.options()
                        .uri(ANY_PATH)
                        .header("Origin", origins.getFirst())
                        .header("Access-Control-Request-Method", "POST"))
                .hasStatus2xxSuccessful()
                .hasHeader("Access-Control-Allow-Origin", origins.getFirst());
    }

    /**
     * The half that proves the origin list is doing something. Without it a
     * policy allowing {@code *} would pass the test above.
     */
    @Test
    void aPreflightFromAnUndeclaredOriginIsRefused() {
        assertThat(mvc.options()
                        .uri(ANY_PATH)
                        .header("Origin", "https://not-allowed.example")
                        .header("Access-Control-Request-Method", "POST"))
                .hasStatus(403);
    }
}
