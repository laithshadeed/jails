package com.example.demo.messaging;

import static org.assertj.core.api.Assertions.assertThat;

import io.micrometer.core.instrument.MeterRegistry;
import java.lang.reflect.Proxy;
import org.apache.kafka.clients.consumer.ConsumerRecord;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.ObjectProvider;
import org.springframework.kafka.core.KafkaOperations;
import org.springframework.kafka.listener.DefaultErrorHandler;
import org.springframework.kafka.support.serializer.DeserializationException;

class KafkaConfigTest {

    /**
     * Builds the handler the way a context with no {@code MeterRegistry}
     * would -- i.e. a project that has not run {@code jails add observability}.
     * If this throws, the optional-registry wiring is wrong and every such
     * project fails at startup.
     */
    @SuppressWarnings("unchecked")
    private static DefaultErrorHandler handlerWithoutRegistry() {
        KafkaOperations<Object, Object> template = noCalls(KafkaOperations.class);
        ObjectProvider<MeterRegistry> noRegistry = noCalls(ObjectProvider.class);
        return new KafkaConfig().errorHandler(template, noRegistry);
    }

    /** A JDK proxy is enough here; no Byte Buddy agent or self-attachment. */
    @SuppressWarnings("unchecked")
    private static <T> T noCalls(Class<T> type) {
        return (T)
                Proxy.newProxyInstance(
                        type.getClassLoader(),
                        new Class<?>[] {type},
                        (proxy, method, arguments) -> {
                            Class<?> result = method.getReturnType();
                            if (result == boolean.class) return false;
                            if (result == byte.class) return (byte) 0;
                            if (result == short.class) return (short) 0;
                            if (result == int.class) return 0;
                            if (result == long.class) return 0L;
                            if (result == float.class) return 0F;
                            if (result == double.class) return 0D;
                            if (result == char.class) return '\0';
                            return null;
                        });
    }

    /**
     * The classification, which is the part that matters.
     *
     * <p>{@code removeClassification} returns the classification it removed,
     * which is the only public way to read one back. It mutates the handler,
     * so this test builds its own.
     */
    @Test
    void aRecordThatCanNeverSucceedIsNotRetried() {
        var handler = handlerWithoutRegistry();

        assertThat(handler.removeClassification(NonRetryableException.class))
                .as("the domain said this record can never be processed")
                .isFalse();
        // Not added by this config -- inherited from
        // ExceptionClassifier.defaultFatalExceptionsList(). Pinned because the
        // generated config deliberately relies on it instead of restating it.
        assertThat(handler.removeClassification(DeserializationException.class))
                .as("a record that cannot be parsed will not parse on retry either")
                .isFalse();
    }

    /**
     * The deliberate omission, pinned so nobody "helpfully" adds it back.
     *
     * <p>A {@code NullPointerException} is a bug in this service, not a bad
     * record. Classifying it permanent would dead-letter it and commit the
     * offset, turning a loud repeating failure into a silent one.
     *
     * <p>{@code removeClassification} is a map removal, so {@code null} means
     * "never classified either way" -- which is the assertion wanted here. It
     * falls through to the classifier's default and is retried.
     */
    @Test
    void aBugInTheListenerStaysRetryableAndStaysLoud() {
        assertThat(handlerWithoutRegistry().removeClassification(NullPointerException.class))
                .as("an NPE is a defect to fix, not a record to quarantine")
                .isNull();
    }

    /**
     * Pins the dead-letter destination against the recoverer's own default,
     * which is `-dlt` and a matching partition number. A project that declares
     * `<topic>.DLT` and consumes it would otherwise find it empty.
     */
    @Test
    void deadLetterRecordsGoToTheDotDltTopic() {
        var record = new ConsumerRecord<>("orders", 2, 0L, "k", "v");
        assertThat(record.topic() + KafkaConfig.DEAD_LETTER_SUFFIX).isEqualTo("orders.DLT");
    }
}
