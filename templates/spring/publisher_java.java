package {{pkg}};

import java.util.concurrent.CompletableFuture;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.kafka.core.KafkaTemplate;
import org.springframework.kafka.support.SendResult;
import org.springframework.stereotype.Component;

/**
 * Publishes {@link {{name}}Event}.
 *
 * <p>The topic is a property, not a constant: the same jar has to run against
 * a local broker, a shared staging one and production, and those rarely agree
 * on names.
 *
{{ordering}} */
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

    /** The returned acknowledgement lets durable callers mark success only after Kafka accepts it. */
    public CompletableFuture<SendResult<String, {{name}}Event>> publish({{name}}Event event) {
        return kafka.send(topic, {{key}}, event);
    }
}
