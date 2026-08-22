package {{pkg}};

import java.util.List;
import org.junit.jupiter.api.Test;
import org.springframework.mock.web.MockHttpServletRequest;
import org.springframework.web.cors.UrlBasedCorsConfigurationSource;

import static org.assertj.core.api.Assertions.assertThat;

class CorsConfigTest {

    @Test
    void permits_the_declared_origin_and_every_mutating_api_method() {
        var source = (UrlBasedCorsConfigurationSource)
                new CorsConfig().corsConfigurationSource(List.of("https://ui.example"));
        var request = new MockHttpServletRequest("OPTIONS", "/resources");
        var policy = source.getCorsConfiguration(request);

        assertThat(policy).isNotNull();
        assertThat(policy.getAllowedOrigins()).containsExactly("https://ui.example");
        assertThat(policy.getAllowedMethods())
                .contains("GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS");
        assertThat(policy.getAllowCredentials()).isTrue();
    }
}
