package {{pkg}};

import org.springframework.beans.factory.annotation.Value;
import org.springframework.kafka.core.KafkaTemplate;
import org.springframework.stereotype.Component;

/**
 * Publishes {@link {{name}}Event}.
 *
 * <p>The topic is a property, not a constant: the same jar has to run against
 * a local broker, a shared staging one and production, and those rarely agree
 * on names.
 *
 * <p>The key is the event id, which is what gives ordering per entity --
 * Kafka only guarantees order within a partition, and a null key round-robins
 * across all of them. Getting this wrong produces a system that works until
 * it has traffic.
 */
@Component
public class {{name}}Publisher {

    private final KafkaTemplate<String, {{name}}Event> kafka;
    private final String topic;

    public {{name}}Publisher(
            KafkaTemplate<String, {{name}}Event> kafka,
            @Value("${topics.{{topic}}:{{topic}}}") String topic) {
        this.kafka = kafka;
        this.topic = topic;
    }

    /** Publishes asynchronously; the send is in flight when this returns. */
    public void publish({{name}}Event event) {
        kafka.send(topic, event.id(), event);
    }
}
