package com.example.demo;

import org.springframework.beans.factory.annotation.Value;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.context.annotation.Profile;
import org.springframework.security.config.Customizer;
import org.springframework.security.config.annotation.web.builders.HttpSecurity;
import org.springframework.security.config.annotation.web.configuration.EnableWebSecurity;
import org.springframework.security.config.annotation.web.configurers.AbstractHttpConfigurer;
import org.springframework.security.config.http.SessionCreationPolicy;
import org.springframework.security.core.userdetails.User;
import org.springframework.security.core.userdetails.UserDetailsService;
import org.springframework.security.crypto.bcrypt.BCryptPasswordEncoder;
import org.springframework.security.crypto.password.PasswordEncoder;
import org.springframework.security.provisioning.InMemoryUserDetailsManager;
import org.springframework.security.web.SecurityFilterChain;

/**
 * Who may reach what, spelled out.
 *
 * <p>Written rather than inherited on purpose. Spring Boot's default chain
 * secures everything and prints a generated password at startup, which is a
 * good default and an opaque one -- and the usual reaction to it is a blanket
 * {@code permitAll()} that nobody revisits. A chain you can read is a chain
 * you can review.
 *
 * <p>Shaped for an API rather than a browser application. The three choices
 * below go together and are only safe together:
 *
 * <ul>
 *   <li>{@code STATELESS} -- no session is created, so there is no session
 *       cookie.
 *   <li>CSRF disabled -- CSRF is an attack on *ambient* credentials, meaning
 *       one the browser attaches automatically, like a session cookie. With
 *       no cookie there is nothing to ride on. Re-enable it the moment this
 *       application starts issuing one: form login, {@code rememberMe} and
 *       session-based auth all need it.
 *   <li>HTTP Basic -- honest placeholder. Replace it with the real scheme
 *       ({@code oauth2ResourceServer} for JWTs) rather than building a
 *       token check by hand.
 * </ul>
 */
@Configuration(proxyBeanMethods = false)
@EnableWebSecurity
@Profile("!prod")
public class SecurityConfig {

    @Bean
    public PasswordEncoder passwordEncoder() {
        return new BCryptPasswordEncoder();
    }

    /** Explicit local credentials; this entire configuration is absent in prod. */
    @Bean
    public UserDetailsService developmentUsers(
            @Value("${app.security.dev.username:dev}") String username,
            @Value("${app.security.dev.password:dev-only-change-me}") String password,
            PasswordEncoder encoder) {
        return new InMemoryUserDetailsManager(
                User.withUsername(username)
                        .password(encoder.encode(password))
                        .roles("USER")
                        .build());
    }

    @Bean
    public SecurityFilterChain securityFilterChain(HttpSecurity http) throws Exception {
        return http.authorizeHttpRequests(
                        requests ->
                                requests
                                        // Liveness for a load balancer, which
                                        // cannot authenticate. Only `health` --
                                        // `env` and `heapdump` are not public.
                                        .requestMatchers("/management/health/**")
                                        .permitAll()
                                        // Default deny: a new endpoint is
                                        // protected until someone says
                                        // otherwise, which is the only default
                                        // that fails safe.
                                        .anyRequest()
                                        .authenticated())
                .sessionManagement(
                        session -> session.sessionCreationPolicy(SessionCreationPolicy.STATELESS))
                .cors(Customizer.withDefaults())
                .csrf(AbstractHttpConfigurer::disable)
                .httpBasic(Customizer.withDefaults())
                .build();
    }
}
