package com.example.demo;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.time.Duration;
import java.time.Instant;
import java.util.HexFormat;
import javax.crypto.Mac;
import javax.crypto.spec.SecretKeySpec;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.stereotype.Component;

/**
 * Decide whether an inbound webhook really came from who it says.
 *
 * <p>Three details, and each of them is a way this is normally got wrong.
 *
 * <ol>
 *   <li><b>The signature is over the raw request bytes.</b> Not over a
 *       re-serialised object: two JSON documents can mean the same thing and
 *       hash differently -- key order, whitespace, a number written {@code 1.0}
 *       instead of {@code 1}. A verifier that deserialises first and
 *       re-serialises to check will reject perfectly good deliveries, in a way
 *       that looks intermittent because it depends on the sender's formatting.
 *       This class takes {@code byte[]}, and the controller takes
 *       {@code @RequestBody byte[]} for the same reason.
 *   <li><b>The comparison is {@link MessageDigest#isEqual}.</b> {@code equals}
 *       and {@code Arrays.equals} return as soon as two bytes differ, so how
 *       long a rejection takes tells an attacker how much of the signature was
 *       right -- and a signature can be recovered one byte at a time from that.
 *       {@code isEqual} is documented as time-constant, and the JDK implements
 *       it as such.
 *   <li><b>The timestamp is checked, and it is checked in <i>both</i>
 *       directions.</b> Without it a valid signed request captured once can be
 *       replayed forever. Five minutes is Stripe's tolerance and a good
 *       default. Rejecting only old timestamps leaves a far-future one
 *       accepted, which is the same replay window under a different sign.
 * </ol>
 *
 * <p>The signed payload is {@code <timestamp>.<body>}: the timestamp must be
 * inside the signature or it is a header anybody can rewrite, and the delimiter
 * must be a character that cannot occur in the timestamp, or two different
 * (timestamp, body) pairs can produce the same signed bytes.
 */
@Component
public class ProviderVerifier {

    /** Stripe's tolerance, and a sensible one: long enough for a retry, short enough to matter. */
    private static final Duration TOLERANCE = Duration.ofMinutes(5);

    private static final String ALGORITHM = "HmacSHA256";

    private final byte[] secret;

    public ProviderVerifier(@Value("${app.provider.secret}") String secret) {
        this.secret = secret.getBytes(StandardCharsets.UTF_8);
    }

    /**
     * @param body the bytes as they arrived, before any parsing.
     * @param timestamp the sender's timestamp header, in epoch seconds.
     * @param signature the sender's signature header, hex-encoded.
     * @throws InvalidSignatureException when the delivery cannot be trusted.
     *     One exception for every reason on purpose: a caller that can tell
     *     "bad signature" from "stale timestamp" is a caller that can probe.
     */
    public void verify(byte[] body, String timestamp, String signature) {
        Instant sentAt;
        try {
            sentAt = Instant.ofEpochSecond(Long.parseLong(timestamp.trim()));
        } catch (NumberFormatException | ArithmeticException malformed) {
            throw new InvalidSignatureException();
        }
        Duration drift = Duration.between(sentAt, Instant.now()).abs();
        if (drift.compareTo(TOLERANCE) > 0) {
            throw new InvalidSignatureException();
        }

        byte[] expected = sign(body, timestamp);
        byte[] presented;
        try {
            presented = HexFormat.of().parseHex(signature.trim());
        } catch (IllegalArgumentException notHex) {
            throw new InvalidSignatureException();
        }
        if (!MessageDigest.isEqual(expected, presented)) {
            throw new InvalidSignatureException();
        }
    }

    /**
     * The signature this service would produce for a delivery.
     *
     * <p>Public because sending a webhook and receiving one are the same
     * computation, and a test that reimplements it is testing itself.
     */
    public byte[] sign(byte[] body, String timestamp) {
        try {
            Mac mac = Mac.getInstance(ALGORITHM);
            mac.init(new SecretKeySpec(secret, ALGORITHM));
            mac.update(timestamp.getBytes(StandardCharsets.UTF_8));
            mac.update((byte) '.');
            mac.update(body);
            return mac.doFinal();
        } catch (java.security.GeneralSecurityException impossible) {
            // HmacSHA256 is required of every Java platform implementation,
            // and the key is never empty by construction.
            throw new IllegalStateException(impossible);
        }
    }

    /** One reason, deliberately. See {@link #verify}. */
    public static final class InvalidSignatureException extends RuntimeException {
        public InvalidSignatureException() {
            super("webhook signature is not valid for this delivery");
        }
    }
}
