package {{pkg}};

import java.util.List;
import java.util.concurrent.CompletionException;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

/**
 * Leased outbox relay; success means every configured sink acknowledged the
 * event.
 *
 * <p>One tick drains the backlog rather than moving one row. A relay that
 * claims a single row per tick has a throughput ceiling of one event per tick
 * whatever the sinks can do, and the only symptom is a queue that never
 * empties -- so the batch size is a property and the run keeps claiming until
 * a short batch says the runnable set is exhausted. It terminates because
 * every claimed row either succeeds or has its next attempt pushed into the
 * future.
 *
 * <p>{@code fixedDelay} means two runs never overlap, so a long drain delays
 * the next tick rather than racing it.
 */
@Component
public final class {{usecase}}OutboxWorker {

    private static final Logger log = LoggerFactory.getLogger({{usecase}}OutboxWorker.class);
    private final Jdbc{{usecase}}Outbox outbox;
    private final List<{{usecase}}OutboxSink> sinks;
    private final int batchSize;

    public {{usecase}}OutboxWorker(
            Jdbc{{usecase}}Outbox outbox,
            List<{{usecase}}OutboxSink> sinks,
            @Value("${outbox.{{property}}.batch-size:100}") int batchSize) {
        this.outbox = outbox;
        this.sinks = List.copyOf(sinks);
        if (sinks.isEmpty()) throw new IllegalStateException("outbox needs at least one sink");
        if (batchSize < 1) throw new IllegalArgumentException("outbox batch size must be positive");
        this.batchSize = batchSize;
    }

    @Scheduled(
            fixedDelayString = "${outbox.{{property}}.delay:PT1S}",
            initialDelayString = "${outbox.{{property}}.initial-delay:PT1S}")
    public void run() {
        try { runOnce(); }
        catch (RuntimeException infrastructureFailure) {
            log.error("{{usecase}} outbox could not claim work; the schedule continues", infrastructureFailure);
        }
    }

    public void runOnce() {
        for (var batch = outbox.claim(batchSize); !batch.isEmpty(); batch = outbox.claim(batchSize)) {
            batch.forEach(this::publish);
            if (batch.size() < batchSize) return;
        }
    }

    /**
     * Delivers to every sink that has not already accepted this event.
     *
     * <p>The skip is the point. A row is only as atomic as its worst sink, so
     * without it a Kafka publish that succeeded is re-sent on every attempt
     * that a slower sink fails, and consumers see the event once per attempt.
     * Each acceptance is recorded before the next sink is tried.
     */
    private void publish(Jdbc{{usecase}}Outbox.Claimed claimed) {
        String attempted = "?";
        try {
            for (var sink : sinks) {
                if (claimed.delivered().contains(sink.name())) continue;
                attempted = sink.name();
                sink.deliver(claimed.event());
                outbox.delivered(claimed.id(), sink.name());
            }
            outbox.succeed(claimed.id());
        } catch (CompletionException failure) {
            var cause = failure.getCause();
            var recorded = cause instanceof RuntimeException runtime ? runtime : failure;
            outbox.fail(claimed.id(), recorded);
            log.warn("{{usecase}} outbox attempt {} failed at sink {}", claimed.attempt(), attempted, recorded);
        } catch (RuntimeException failure) {
            outbox.fail(claimed.id(), failure);
            log.warn("{{usecase}} outbox attempt {} failed at sink {}", claimed.attempt(), attempted, failure);
        }
    }
}
