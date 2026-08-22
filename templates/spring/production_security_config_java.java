package {{pkg}};

import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.context.annotation.Profile;
import org.springframework.security.config.Customizer;
import org.springframework.security.config.annotation.web.builders.HttpSecurity;
import org.springframework.security.config.annotation.web.configurers.AbstractHttpConfigurer;
import org.springframework.security.config.http.SessionCreationPolicy;
import org.springframework.security.web.SecurityFilterChain;

/**
 * Production authentication is a JWT resource server, never the local user.
 *
 * <p>Set {@code spring.security.oauth2.resourceserver.jwt.issuer-uri}. Spring
 * validates issuer, signature, expiry and not-before through the provider's
 * discovery/JWK metadata. With no issuer/JWK configuration the production
 * application fails startup instead of falling back to a generated password.
 */
@Configuration(proxyBeanMethods = false)
@Profile("prod")
public class ProductionSecurityConfig {

    @Bean
    public SecurityFilterChain productionSecurityFilterChain(HttpSecurity http)
            throws Exception {
        return http.authorizeHttpRequests(
                        requests -> requests
                                .requestMatchers("/management/health/**")
                                .permitAll()
                                .anyRequest()
                                .authenticated())
                .sessionManagement(
                        session -> session.sessionCreationPolicy(SessionCreationPolicy.STATELESS))
                .cors(Customizer.withDefaults())
                .csrf(AbstractHttpConfigurer::disable)
                .oauth2ResourceServer(resourceServer -> resourceServer.jwt(Customizer.withDefaults()))
                .build();
    }
}
