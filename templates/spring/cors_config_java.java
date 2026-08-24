package {{pkg}};

import java.util.List;
import org.springframework.beans.factory.annotation.Qualifier;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.boot.web.servlet.FilterRegistrationBean;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.core.Ordered;
import org.springframework.web.cors.CorsConfiguration;
import org.springframework.web.cors.CorsConfigurationSource;
import org.springframework.web.cors.UrlBasedCorsConfigurationSource;
import org.springframework.web.filter.CorsFilter;

/** Explicit browser boundary: origins are configuration, methods are reviewable code. */
@Configuration(proxyBeanMethods = false)
public class CorsConfig {

    @Bean
    CorsConfigurationSource corsConfigurationSource(
            @Value("${app.cors.allowed-origins}") List<String> origins) {
        var policy = new CorsConfiguration();
        policy.setAllowedOrigins(origins);
        policy.setAllowedMethods(List.of("GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"));
        policy.setAllowedHeaders(List.of("Authorization", "Content-Type", "Idempotency-Key"));
        policy.setExposedHeaders(List.of("Location"));
        policy.setAllowCredentials(true);
        policy.setMaxAge(3600L);

        var source = new UrlBasedCorsConfigurationSource();
        source.registerCorsConfiguration("/**", policy);
        return source;
    }

    /**
     * Registers the policy above so something actually reads it.
     *
     * <p>Without this the bean is defined and never consulted. A
     * {@code CorsConfigurationSource} is read by Spring Security's filter
     * chain, and an application with no Spring Security has no chain: the
     * preflight is answered by the dispatcher's default {@code OPTIONS}
     * handler, which returns 200 and an {@code Allow} header and <em>no</em>
     * {@code Access-Control-Allow-Origin} at all. The browser blocks the real
     * request, the server logs nothing, and every server-side test passes.
     *
     * <p>Harmless when Security is present: its chain and this filter read the
     * same bean, so they cannot disagree.
     *
     * <p>The qualifier is load-bearing. Spring MVC registers its own
     * {@code mvcHandlerMappingIntrospector}, which is <em>also</em> a
     * {@code CorsConfigurationSource}, so an unqualified injection point finds
     * two candidates and the context does not start.
     */
    @Bean
    FilterRegistrationBean<CorsFilter> corsFilterRegistration(
            @Qualifier("corsConfigurationSource") CorsConfigurationSource source) {
        var registration = new FilterRegistrationBean<>(new CorsFilter(source));
        registration.setOrder(Ordered.HIGHEST_PRECEDENCE);
        return registration;
    }
}
