package com.example.demo;

import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.util.Map;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.mock.env.MockEnvironment;
import org.springframework.security.authentication.TestingAuthenticationToken;
import org.springframework.security.oauth2.jwt.Jwt;

class ScopeAuthorizerTest {

    private static final UUID WORKSPACE =
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
                Map.of("workspaceId", WORKSPACE.toString()));
        var authentication = new TestingAuthenticationToken(jwt, null);

        guard.require(authentication, "workspaceId", WORKSPACE);

        assertThatThrownBy(() -> guard.require(authentication, "workspaceId", UUID.randomUUID()))
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
                        "workspaceId",
                        WORKSPACE))
                .isInstanceOf(org.springframework.web.server.ResponseStatusException.class);
    }

    @Test
    void developmentCanPinAClaimForLocalTesting() {
        var environment = new MockEnvironment()
                .withProperty("app.security.dev.scopes.workspaceId", WORKSPACE.toString());
        var guard = new ScopeAuthorizer(environment);

        guard.require(null, "workspaceId", WORKSPACE);
        assertThatThrownBy(() -> guard.require(null, "workspaceId", UUID.randomUUID()))
                .isInstanceOf(org.springframework.web.server.ResponseStatusException.class);
    }
}
