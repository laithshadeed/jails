package com.example.demo;

import java.time.Duration;
import java.time.Instant;
import java.util.List;
import org.springframework.security.oauth2.jwt.JwtClaimsSet;
import org.springframework.security.oauth2.jwt.JwtEncoder;
import org.springframework.security.oauth2.jwt.JwtEncoderParameters;
import org.springframework.stereotype.Service;

/**
 * Mint a token for a subject.
 *
 * <p>{@code exp} is set here and not optional. The decoder in
 * {@code ApiTokenConfig} refuses a token without one, and the two changes
 * belong together: a lifetime enforced only at issue time is enforced by
 * whoever is issuing, which is not a security property.
 *
 * <p>Scopes are the claim {@code ScopeAuthorizer} reads if `add security` is
 * installed, so a token minted here already carries what a `@scope` field
 * needs proved.
 */
@Service
public class ApiTokens {

    /** Short enough that a leaked token is a small window, long enough to be usable. */
    private static final Duration LIFETIME = Duration.ofMinutes(15);

    private final JwtEncoder encoder;

    public ApiTokens(JwtEncoder encoder) {
        this.encoder = encoder;
    }

    /**
     * @param subject who the token is about — a user id, a service name. Never
     *     an email or anything else that changes.
     * @param scopes what the holder may do. An empty list is a token that
     *     authenticates and authorises nothing, which is a legitimate thing to
     *     want and not an error.
     */
    public String issue(String subject, List<String> scopes) {
        Instant now = Instant.now();
        JwtClaimsSet claims =
                JwtClaimsSet.builder()
                        .issuer("urn:com.example.demo")
                        .issuedAt(now)
                        .expiresAt(now.plus(LIFETIME))
                        .subject(subject)
                        .claim("scope", String.join(" ", scopes))
                        .build();
        return encoder.encode(JwtEncoderParameters.from(claims)).getTokenValue();
    }
}
