package com.example.demo;

import static org.assertj.core.api.Assertions.assertThatCode;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.nio.charset.StandardCharsets;
import java.time.Instant;
import java.util.HexFormat;
import org.junit.jupiter.api.Test;

/** Each test is one of the three ways this is normally got wrong. */
class ProviderVerifierTest {

    private static final String SECRET = "whsec_test_secret";

    private final ProviderVerifier verifier = new ProviderVerifier(SECRET);

    private String now() {
        return Long.toString(Instant.now().getEpochSecond());
    }

    private String signatureFor(byte[] body, String timestamp) {
        return HexFormat.of().formatHex(verifier.sign(body, timestamp));
    }

    @Test
    void a_genuine_delivery_is_accepted() {
        byte[] body = "{\"id\":\"evt_1\"}".getBytes(StandardCharsets.UTF_8);
        String timestamp = now();

        assertThatCode(() -> verifier.verify(body, timestamp, signatureFor(body, timestamp)))
                .doesNotThrowAnyException();
    }

    /**
     * The reason the signature is over raw bytes. These two documents mean the
     * same thing and hash differently, so a verifier that re-serialised before
     * checking would reject the second -- intermittently, depending on the
     * sender's formatting.
     */
    @Test
    void the_same_json_written_differently_does_not_verify_against_the_first() {
        String timestamp = now();
        byte[] sent = "{\"id\":\"evt_1\",\"amount\":1}".getBytes(StandardCharsets.UTF_8);
        byte[] reformatted = "{ \"amount\": 1, \"id\": \"evt_1\" }".getBytes(StandardCharsets.UTF_8);
        String signature = signatureFor(sent, timestamp);

        assertThatThrownBy(() -> verifier.verify(reformatted, timestamp, signature))
                .isInstanceOf(ProviderVerifier.InvalidSignatureException.class);
    }

    @Test
    void a_body_changed_after_signing_is_refused() {
        String timestamp = now();
        byte[] body = "{\"amount\":1}".getBytes(StandardCharsets.UTF_8);
        String signature = signatureFor(body, timestamp);
        byte[] tampered = "{\"amount\":9}".getBytes(StandardCharsets.UTF_8);

        assertThatThrownBy(() -> verifier.verify(tampered, timestamp, signature))
                .isInstanceOf(ProviderVerifier.InvalidSignatureException.class);
    }

    /** A captured delivery replayed tomorrow. */
    @Test
    void a_stale_timestamp_is_refused_even_with_a_valid_signature() {
        String old = Long.toString(Instant.now().minusSeconds(3600).getEpochSecond());
        byte[] body = "{\"id\":\"evt_1\"}".getBytes(StandardCharsets.UTF_8);

        assertThatThrownBy(() -> verifier.verify(body, old, signatureFor(body, old)))
                .isInstanceOf(ProviderVerifier.InvalidSignatureException.class);
    }

    /**
     * The half that is usually missed: rejecting only *old* timestamps leaves
     * a far-future one accepted, which is the same replay window with its sign
     * flipped.
     */
    @Test
    void a_far_future_timestamp_is_refused_too() {
        String ahead = Long.toString(Instant.now().plusSeconds(3600).getEpochSecond());
        byte[] body = "{\"id\":\"evt_1\"}".getBytes(StandardCharsets.UTF_8);

        assertThatThrownBy(() -> verifier.verify(body, ahead, signatureFor(body, ahead)))
                .isInstanceOf(ProviderVerifier.InvalidSignatureException.class);
    }

    /**
     * The timestamp is inside the signature, so moving it invalidates the
     * delivery. Otherwise it is a header anyone in the middle can rewrite, and
     * the replay window above is not a window at all.
     */
    @Test
    void the_timestamp_is_covered_by_the_signature() {
        byte[] body = "{\"id\":\"evt_1\"}".getBytes(StandardCharsets.UTF_8);
        String timestamp = now();
        String signature = signatureFor(body, timestamp);
        String moved = Long.toString(Long.parseLong(timestamp) - 1);

        assertThatThrownBy(() -> verifier.verify(body, moved, signature))
                .isInstanceOf(ProviderVerifier.InvalidSignatureException.class);
    }

    @Test
    void a_signature_that_is_not_hex_is_refused_rather_than_crashing() {
        byte[] body = "{}".getBytes(StandardCharsets.UTF_8);

        assertThatThrownBy(() -> verifier.verify(body, now(), "not-a-signature"))
                .isInstanceOf(ProviderVerifier.InvalidSignatureException.class);
    }
}
