package com.example.intercom.messaging;

import io.micrometer.core.instrument.MeterRegistry;
import org.apache.kafka.common.TopicPartition;
import org.springframework.beans.factory.ObjectProvider;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.kafka.core.KafkaOperations;
import org.springframework.kafka.listener.DeadLetterPublishingRecoverer;
import org.springframework.kafka.listener.DefaultErrorHandler;
import org.springframework.kafka.support.ExponentialBackOffWithMaxRetries;

/**
 * What happens to a record that does not process cleanly.
 *
 * <p>Without a defined poison-message path, one bad record blocks its
 * partition forever and the only symptom is consumer lag -- there is no
 * repeated error to find, because the first one already scrolled away.
 */
@Configuration(proxyBeanMethods = false)
class KafkaConfig {

    /** The suffix this service's dead-letter topics use. See {@link #errorHandler}. */
    static final String DEAD_LETTER_SUFFIX = ".DLT";

    /**
     * Counts records routed to a dead-letter topic, tagged by source topic.
     *
     * <p>A dead-letter topic nothing alerts on is silent discard with extra
     * steps, and this is the number a depth alarm is built from.
     *
     * <p>Two things to know before writing that alarm. The series does not
     * exist until the first record dead-letters, because the topic tag is only
     * known once there is a record to tag -- so alert on presence, not on
     * {@code rate() == 0}. And it counts *routing attempts*, not records
     * durably in the topic: it increments before the publish is confirmed, so a
     * failed publish means a redelivery and a second increment.
     */
    static final String DEAD_LETTER_METRIC = "kafka.dlt";

    /**
     * Retries a transient failure with exponential backoff, and sends a
     * permanent one straight to the dead-letter topic.
     *
     * <p>The destination is named explicitly. {@code
     * DeadLetterPublishingRecoverer}'s own default appends {@code -dlt} and
     * uses the *same* partition number as the source record, so a project that
     * declares a {@code .DLT} topic and ships a consumer for it gets neither:
     * the records land on an auto-created {@code -dlt} topic, and the only
     * trace is a WARN.
     *
     * <p>The {@code MeterRegistry} is optional on purpose. Spring Kafka
     * declares Micrometer as an optional dependency, so a project that has not
     * run {@code jails add observability} has the API on the classpath but no
     * registry bean; injecting one directly would fail the context at startup.
     * {@code ObjectProvider} makes the counter appear when a registry does and
     * cost nothing when it does not.
     */
    @Bean
    DefaultErrorHandler errorHandler(
            KafkaOperations<Object, Object> template, ObjectProvider<MeterRegistry> registries) {
        var backOff = new ExponentialBackOffWithMaxRetries(3);
        backOff.setInitialInterval(200);
        backOff.setMultiplier(2.0);
        var registry = registries.getIfAvailable();
        var recoverer = new DeadLetterPublishingRecoverer(template, (record, exception) -> {
            if (registry != null) {
                registry.counter(DEAD_LETTER_METRIC, "topic", record.topic()).increment();
            }
            return new TopicPartition(record.topic() + DEAD_LETTER_SUFFIX, -1);
        });
        var handler = new DefaultErrorHandler(recoverer, backOff);
        // Spring already classifies DeserializationException,
        // MessageConversionException, ConversionException,
        // MethodArgumentResolutionException and ClassCastException as fatal --
        // see ExceptionClassifier.defaultFatalExceptionsList(). This adds the
        // one thing the framework cannot infer: the domain's own "no retry will
        // ever fix this". Deliberately *not* NullPointerException -- that is a
        // bug in this service, not a bad record, and dead-lettering it commits
        // the offset and buries it.
        handler.addNotRetryableExceptions(NonRetryableException.class);
        return handler;
    }
}
