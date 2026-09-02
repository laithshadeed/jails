package {{pkg}};

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.HexFormat;
import java.util.Optional;
import java.util.function.Supplier;
import org.springframework.stereotype.Component;

/**
 * Run an operation at most once per idempotency key, and replay its outcome.
 *
 * <p>Four outcomes, and every one of them is a case something gets wrong:
 *
 * <ul>
 *   <li><b>First call</b> — claim the key, run the operation, record the result.
 *   <li><b>Retry, same request</b> — return the recorded result. Not a 409: the
 *       caller retried precisely because it never saw the first answer, so
 *       telling it "already done" without the answer leaves it exactly where it
 *       started. This is the case a unique constraint alone cannot serve.
 *   <li><b>Same key, different request</b> — refuse. The key was reused for
 *       something else, which is a client bug; replaying the first result would
 *       silently discard the second request.
 *   <li><b>Retry while the first call is still running</b> — refuse, and say to
 *       retry. There is no answer to replay yet, and waiting for one would tie
 *       up a request thread on a lock held by another.
 * </ul>
 *
 * <p>The request hash is over the canonical bytes the caller supplies. Hashing a
 * pretty-printed body would make one request look like two, so pass the payload
 * you actually mean to compare — not a re-serialised object.
 */
@Component
public final class {{name}}Guard {

    private final {{name}}Receipts receipts;

    public {{name}}Guard({{name}}Receipts receipts) {
        this.receipts = receipts;
    }

    /**
     * @param scope what the key is unique within -- a tenant, an account, an API
     *     client. Keys from two callers must not collide, and a single global
     *     namespace is how they do.
     * @param key the caller's idempotency key, usually an {@code Idempotency-Key} header.
     * @param request the canonical request bytes, used only for comparison.
     * @param operation the work to run at most once. Its return value is stored
     *     verbatim and replayed to later retries.
     */
    public Outcome execute(String scope, String key, String request, Supplier<String> operation) {
        String hash = canonicalHash(request);
        Optional<{{name}}Receipt> existing = receipts.claim(scope, key, hash);

        if (existing.isEmpty()) {
            String body = operation.get();
            receipts.complete(scope, key, 200, body);
            return new Outcome(body, false);
        }

        {{name}}Receipt receipt = existing.get();
        if (!receipt.requestHash().equals(hash)) {
            throw new KeyReusedException(key);
        }
        if (!receipt.isComplete()) {
            throw new InProgressException(key);
        }
        return new Outcome(receipt.responseBody().orElseThrow(), true);
    }

    /**
     * @param body the response, whether it was just produced or replayed.
     * @param replayed true when this is a retry being served the first result --
     *     worth surfacing as a response header so a caller can tell.
     */
    public record Outcome(String body, boolean replayed) {}

    /** The same key, a different request. A 422 rather than a 409. */
    public static final class KeyReusedException extends RuntimeException {
        public KeyReusedException(String key) {
            super("idempotency key '" + key + "' was already used for a different request");
        }
    }

    /** The first attempt has not finished. A 409 with a Retry-After. */
    public static final class InProgressException extends RuntimeException {
        public InProgressException(String key) {
            super("idempotency key '" + key + "' is still in flight; retry shortly");
        }
    }

    /**
     * The hash this guard compares requests by.
     *
     * <p>Public because it is genuinely useful outside — logging which request a
     * key is bound to, or building the same receipt from another path — and
     * because a test that needs to place a claim by hand should use the real
     * function rather than a guess at it.
     */
    public static String canonicalHash(String request) {
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            return HexFormat.of()
                    .formatHex(digest.digest(request.getBytes(StandardCharsets.UTF_8)));
        } catch (java.security.NoSuchAlgorithmException impossible) {
            // SHA-256 is required of every Java platform implementation.
            throw new IllegalStateException(impossible);
        }
    }
}
