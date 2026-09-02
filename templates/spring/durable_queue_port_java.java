package {{pkg}};

import java.time.Instant;
import java.util.Optional;
import java.util.UUID;

/**
 * The durable work queue a caller enqueues into.
 *
 * <p>Managed ABI: the store implements it, the worker drains it, and anything
 * that wants work done later names it. The payload is the command's own
 * {@code Input}, so enqueuing cannot describe work the command could not
 * execute.
 *
 * <p><strong>The id is the caller's, and reusing one is a conflict rather than
 * an overwrite.</strong> That is what makes {@code enqueue} safe to retry: the
 * same id with the same payload is the same request, and the same id with a
 * different payload is a mistake worth reporting rather than silently
 * accepting.
 */
public interface {{name}}Queue {

    void enqueue(UUID id, {{usecase}}Command.Input work);

    Optional<Status> status(UUID id);

    enum State { PENDING, RUNNING, SUCCEEDED, FAILED }

    record Status(UUID id, State state, int attempts, Instant nextAttemptAt,
                  Optional<String> lastError, Optional<Instant> completedAt) {}

    final class IdempotencyConflictException extends RuntimeException {
        public IdempotencyConflictException(UUID id) {
            super("work id " + id + " was already used with a different payload");
        }
    }
}
