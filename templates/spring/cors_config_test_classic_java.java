package {{pkg}};

import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.options;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.header;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import {{mockmvc_import}};
import org.springframework.test.web.servlet.MockMvc;

/**
 * A preflight through the real dispatcher, which is the only place this
 * capability can be observed to work.
 *
 * <p>The test this replaced constructed {@code new CorsConfig()} and asserted
 * on the returned object. It passed while CORS was completely broken, because
 * the only failure mode is that <em>nothing reads the bean</em>.
 *
 * <p>The rule this is an instance of: a test that cannot observe the failure
 * and a test that never runs are the same bug.
 *
 * <p>Written against plain {@code MockMvc} rather than {@code MockMvcTester},
 * because this project's Spring Framework predates the AssertJ entry point.
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

    @Autowired private MockMvc mvc;

    @Test
    void aPreflightFromADeclaredOriginIsAnswered() throws Exception {
        mvc.perform(options(ANY_PATH)
                        .header("Origin", "{{origin}}")
                        .header("Access-Control-Request-Method", "POST"))
                .andExpect(status().is2xxSuccessful())
                .andExpect(header().string("Access-Control-Allow-Origin", "{{origin}}"));
    }

    /**
     * The half that proves the origin list is doing something. Without it a
     * policy allowing {@code *} would pass the test above.
     */
    @Test
    void aPreflightFromAnUndeclaredOriginIsRefused() throws Exception {
        mvc.perform(options(ANY_PATH)
                        .header("Origin", "https://not-allowed.example")
                        .header("Access-Control-Request-Method", "POST"))
                .andExpect(status().isForbidden());
    }
}
