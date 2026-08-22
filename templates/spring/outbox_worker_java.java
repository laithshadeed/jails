package {{pkg}};

import java.util.List;
import java.util.concurrent.CompletionException;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

/** Leased outbox relay; success means every configured sink acknowledged the event. */
@Component
public final class {{usecase}}OutboxWorker {

    private static final Logger log = LoggerFactory.getLogger({{usecase}}OutboxWorker.class);
    private final Jdbc{{usecase}}Outbox outbox;
    private final List<{{usecase}}OutboxSink> sinks;

    public {{usecase}}OutboxWorker(Jdbc{{usecase}}Outbox outbox, List<{{usecase}}OutboxSink> sinks) {
        this.outbox = outbox;
        this.sinks = List.copyOf(sinks);
        if (sinks.isEmpty()) throw new IllegalStateException("outbox needs at least one sink");
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

    public void runOnce() { outbox.claim().ifPresent(this::publish); }

    private void publish(Jdbc{{usecase}}Outbox.Claimed claimed) {
        try {
            for (var sink : sinks) sink.deliver(claimed.event());
            outbox.succeed(claimed.id());
        } catch (CompletionException failure) {
            var cause = failure.getCause();
            var recorded = cause instanceof RuntimeException runtime ? runtime : failure;
            outbox.fail(claimed.id(), recorded);
            log.warn("{{usecase}} outbox attempt {} failed", claimed.attempt(), recorded);
        } catch (RuntimeException failure) {
            outbox.fail(claimed.id(), failure);
            log.warn("{{usecase}} outbox attempt {} failed", claimed.attempt(), failure);
        }
    }
}
