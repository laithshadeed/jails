package com.example.demo;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.time.Instant;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.springframework.security.oauth2.jwt.JwtClaimsSet;
import org.springframework.security.oauth2.jwt.JwtDecoder;
import org.springframework.security.oauth2.jwt.JwtEncoder;
import org.springframework.security.oauth2.jwt.JwtEncoderParameters;
import org.springframework.security.oauth2.jwt.JwtException;

/**
 * The round trip, and the two rejections that are the point of the class.
 *
 * <p>No Spring context: the encoder and decoder are the beans
 * {@code ApiTokenConfig} declares, built directly, so what is under test
 * is the configuration rather than the wiring.
 */
class ApiTokensTest {

    /** 32 bytes, the minimum HS256 accepts. */
    private static final String SECRET = "0123456789abcdef0123456789abcdef";

    private final ApiTokenConfig config = new ApiTokenConfig(SECRET);
    private final JwtEncoder encoder = config.jwtEncoder();
    private final JwtDecoder decoder = config.jwtDecoder();
    private final ApiTokens tokens = new ApiTokens(encoder);

    @Test
    void a_minted_token_verifies_and_carries_its_subject_and_scopes() {
        String token = tokens.issue("user-42", List.of("orders:read", "orders:write"));

        var decoded = decoder.decode(token);

        assertThat(decoded.getSubject()).isEqualTo("user-42");
        assertThat(decoded.getClaimAsString("scope")).isEqualTo("orders:read orders:write");
        assertThat(decoded.getExpiresAt()).isNotNull();
    }

    /**
     * The one this class exists for. {@code JwtTimestampValidator} ships with
     * {@code allowEmptyExpiryClaim = true}, so **every out-of-the-box decoder
     * accepts a token that never expires** and nothing warns. Delete the
     * `setAllowEmptyExpiryClaim(false)` line in the config and only this test
     * notices.
     */
    @Test
    void a_token_with_no_expiry_is_refused() {
        String forever =
                encoder.encode(
                                JwtEncoderParameters.from(
                                        JwtClaimsSet.builder()
                                                .issuer("urn:com.example.demo")
                                                .subject("user-42")
                                                .build()))
                        .getTokenValue();

        assertThatThrownBy(() -> decoder.decode(forever))
                .isInstanceOf(JwtException.class)
                .hasMessageContaining("exp is required");
    }

    @Test
    void an_expired_token_is_refused() {
        Instant past = Instant.now().minusSeconds(3600);
        String stale =
                encoder.encode(
                                JwtEncoderParameters.from(
                                        JwtClaimsSet.builder()
                                                .issuer("urn:com.example.demo")
                                                .subject("user-42")
                                                .issuedAt(past)
                                                .expiresAt(past.plusSeconds(60))
                                                .build()))
                        .getTokenValue();

        assertThatThrownBy(() -> decoder.decode(stale)).isInstanceOf(JwtException.class);
    }

    /** A token signed with another secret is not this service's token. */
    @Test
    void a_token_signed_with_a_different_secret_is_refused() {
        var other = new ApiTokenConfig("fedcba9876543210fedcba9876543210");
        String foreign = new ApiTokens(other.jwtEncoder()).issue("user-42", List.of());

        assertThatThrownBy(() -> decoder.decode(foreign)).isInstanceOf(JwtException.class);
    }
}
