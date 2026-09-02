package {{pkg}};

import java.time.Instant;
import java.util.Optional;

/**
 * A recorded outcome, so a retry returns what the first attempt returned.
 *
 * <p>A unique column on the key gives you one row per key. It does not give you
 * the <em>retained result</em>, and that is the whole difference: without the
 * stored response a retried call finds the row, fails the insert, and answers
 * 409 Conflict. The caller retried because it never saw the first answer, so a
 * 409 tells it the operation happened and still withholds the outcome — which
 * is exactly the state it was trying to escape.
 *
 * <p>{@code requestHash} is what makes the key safe to trust. The same key
 * carrying a <em>different</em> request is a client bug, not a retry, and
 * answering it with the first request's result would silently discard the
 * second. It is a canonical hash of the request body, so formatting and key
 * order do not make one request look like two.
 *
 * <p>{@code responseBody} is empty while the first attempt is still running.
 * That is the concurrent case: two identical requests arrive together, one wins
 * the insert, and the other must be told to retry rather than handed a null
 * body.
 */
public record {{name}}Receipt(
        String scope,
        String key,
        String requestHash,
        int status,
        Optional<String> responseBody,
        Instant createdAt) {

    public {{name}}Receipt {
        scope = java.util.Objects.requireNonNull(scope, "scope");
        key = java.util.Objects.requireNonNull(key, "key");
        requestHash = java.util.Objects.requireNonNull(requestHash, "requestHash");
        responseBody = java.util.Objects.requireNonNullElse(responseBody, Optional.empty());
        createdAt = java.util.Objects.requireNonNull(createdAt, "createdAt");
    }

    /** True once the first attempt finished and its outcome is replayable. */
    public boolean isComplete() {
        return responseBody.isPresent();
    }
}
