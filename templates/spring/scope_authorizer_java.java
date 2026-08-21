package {{pkg}};

import java.util.Arrays;
import java.util.Objects;
import org.springframework.core.env.Environment;
import org.springframework.security.core.Authentication;
import org.springframework.security.oauth2.jwt.Jwt;
import org.springframework.stereotype.Component;
import org.springframework.web.server.ResponseStatusException;

import static org.springframework.http.HttpStatus.NOT_FOUND;

/**
 * Enforces an explicit request scope without knowing any application domain.
 *
 * <p>A field marked {@code @scope} is checked against the same-named JWT
 * claim in production. A mismatch is deliberately reported as not-found so
 * one tenant cannot use an authorization response to enumerate another
 * tenant's resources.
 *
 * <p>Outside {@code prod}, set {@code app.security.dev.scopes.<claim>} to the
 * value being exercised. The development-only {@code *} default keeps a new
 * project convenient; it is never consulted by the production profile.
 */
@Component
public final class ScopeAuthorizer {

    private final Environment environment;

    public ScopeAuthorizer(Environment environment) {
        this.environment = Objects.requireNonNull(environment, "environment is required");
    }

    public void require(Authentication authentication, String claim, Object requested) {
        Objects.requireNonNull(claim, "claim is required");
        Objects.requireNonNull(requested, "requested scope is required");
        var expected = expected(authentication, claim);
        if (!"*".equals(expected) && !String.valueOf(requested).equals(expected)) {
            throw new ResponseStatusException(NOT_FOUND, "resource not found");
        }
    }

    private String expected(Authentication authentication, String claim) {
        if (isProduction()) {
            if (authentication == null || !(authentication.getPrincipal() instanceof Jwt jwt)) {
                throw new ResponseStatusException(NOT_FOUND, "resource not found");
            }
            var value = jwt.getClaimAsString(claim);
            if (value == null || value.isBlank()) {
                throw new ResponseStatusException(NOT_FOUND, "resource not found");
            }
            return value;
        }
        return environment.getProperty("app.security.dev.scopes." + claim, "*");
    }

    private boolean isProduction() {
        return Arrays.asList(environment.getActiveProfiles()).contains("prod");
    }
}
