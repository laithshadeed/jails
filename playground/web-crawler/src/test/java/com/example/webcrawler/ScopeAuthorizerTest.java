package com.example.webcrawler;

import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.util.Map;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.mock.env.MockEnvironment;
import org.springframework.security.authentication.TestingAuthenticationToken;
import org.springframework.security.oauth2.jwt.Jwt;

class ScopeAuthorizerTest {

    private static final UUID TENANT =
            UUID.fromString("00000000-0000-0000-0000-000000000001");

    @Test
    void productionRequiresTheSameNamedJwtClaim() {
        var environment = new MockEnvironment().withProperty("spring.profiles.active", "prod");
        environment.setActiveProfiles("prod");
        var guard = new ScopeAuthorizer(environment);
        var jwt = new Jwt(
                "token",
                null,
                null,
                Map.of("alg", "none"),
                Map.of("tenantId", TENANT.toString()));
        var authentication = new TestingAuthenticationToken(jwt, null);

        guard.require(authentication, "tenantId", TENANT);

        assertThatThrownBy(() -> guard.require(authentication, "tenantId", UUID.randomUUID()))
                .isInstanceOf(org.springframework.web.server.ResponseStatusException.class)
                .hasMessageContaining("404");
    }

    @Test
    void productionRejectsNonJwtAndMissingClaims() {
        var environment = new MockEnvironment();
        environment.setActiveProfiles("prod");
        var guard = new ScopeAuthorizer(environment);

        assertThatThrownBy(() -> guard.require(
                        new TestingAuthenticationToken("local-user", null),
                        "tenantId",
                        TENANT))
                .isInstanceOf(org.springframework.web.server.ResponseStatusException.class);
    }

    @Test
    void developmentCanPinAClaimForLocalTesting() {
        var environment = new MockEnvironment()
                .withProperty("app.security.dev.scopes.tenantId", TENANT.toString());
        var guard = new ScopeAuthorizer(environment);

        guard.require(null, "tenantId", TENANT);
        assertThatThrownBy(() -> guard.require(null, "tenantId", UUID.randomUUID()))
                .isInstanceOf(org.springframework.web.server.ResponseStatusException.class);
    }
}
