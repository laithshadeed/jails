package {{pkg}};

import java.nio.charset.StandardCharsets;
import java.util.Base64;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.context.annotation.Import;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * Both directions, because only one of them is usually checked.
 *
 * <p>A test that an authenticated request succeeds passes just as happily
 * against a chain with {@code permitAll()} on everything. The assertion that
 * an anonymous request is *rejected* is the one that notices when the rules
 * are loosened -- which is exactly the change nobody means to make
 * permanently.
 *
 * <p>The credentials are test-only properties and the request carries a real
 * {@code Authorization} header, rather than using
 * {@code @WithMockUser}. Two reasons: it exercises the actual
 * authentication filter instead of installing a {@code SecurityContext}
 * behind it, and {@code @WithMockUser} does not survive a
 * {@code STATELESS} chain anyway -- with no {@code SecurityContext}
 * repository, the context set by the test is never read back.
 */
@WebMvcTest(
        controllers = SecurityProbeController.class,
        properties = {
            "app.security.dev.username=probe",
            "app.security.dev.password=probe"
        })
@Import(SecurityConfig.class)
class SecurityConfigTest {
    private static final String BASIC =
            "Basic "
                    + Base64.getEncoder()
                            .encodeToString("probe:probe".getBytes(StandardCharsets.UTF_8));

    @Autowired
    private MockMvcTester mvc;

    @Test
    void healthIsReachableWithoutCredentials() {
        // A load balancer cannot authenticate. The focused controller makes
        // this prove permitAll(), rather than treating a 404 as success.
        assertThat(mvc.get().uri("/management/health")).hasStatusOk();
    }

    @Test
    void anythingElseRequiresCredentials() {
        assertThat(mvc.get().uri("/anything")).hasStatus(401);
    }

    @Test
    void anAuthenticatedRequestGetsThrough() {
        assertThat(mvc.get().uri("/anything").header("Authorization", BASIC)).hasStatusOk();
    }
}

@RestController
class SecurityProbeController {

    @GetMapping({"/management/health", "/anything"})
    String ok() {
        return "ok";
    }
}
