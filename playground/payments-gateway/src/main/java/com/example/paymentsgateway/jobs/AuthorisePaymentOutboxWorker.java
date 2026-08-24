package com.example.paymentsgateway.jobs;

import java.util.List;
import java.util.concurrent.CompletionException;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

/** Leased outbox relay; success means every configured sink acknowledged the event. */
@Component
public final class AuthorisePaymentOutboxWorker {

    private static final Logger log = LoggerFactory.getLogger(AuthorisePaymentOutboxWorker.class);
    private final JdbcAuthorisePaymentOutbox outbox;
    private final List<AuthorisePaymentOutboxSink> sinks;

    public AuthorisePaymentOutboxWorker(JdbcAuthorisePaymentOutbox outbox, List<AuthorisePaymentOutboxSink> sinks) {
        this.outbox = outbox;
        this.sinks = List.copyOf(sinks);
        if (sinks.isEmpty()) throw new IllegalStateException("outbox needs at least one sink");
    }

    @Scheduled(
            fixedDelayString = "${outbox.authorise-payment.delay:PT1S}",
            initialDelayString = "${outbox.authorise-payment.initial-delay:PT1S}")
    public void run() {
        try { runOnce(); }
        catch (RuntimeException infrastructureFailure) {
            log.error("AuthorisePayment outbox could not claim work; the schedule continues", infrastructureFailure);
        }
    }

    public void runOnce() { outbox.claim().ifPresent(this::publish); }

    private void publish(JdbcAuthorisePaymentOutbox.Claimed claimed) {
        try {
            for (var sink : sinks) sink.deliver(claimed.event());
            outbox.succeed(claimed.id());
        } catch (CompletionException failure) {
            var cause = failure.getCause();
            var recorded = cause instanceof RuntimeException runtime ? runtime : failure;
            outbox.fail(claimed.id(), recorded);
            log.warn("AuthorisePayment outbox attempt {} failed", claimed.attempt(), recorded);
        } catch (RuntimeException failure) {
            outbox.fail(claimed.id(), failure);
            log.warn("AuthorisePayment outbox attempt {} failed", claimed.attempt(), failure);
        }
    }
}
