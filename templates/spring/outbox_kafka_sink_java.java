package {{pkg}};

import org.springframework.core.annotation.Order;
import org.springframework.stereotype.Component;

/** Kafka destination in the same generic sink chain as provider delivery. */
@Component
@Order(0)
public final class {{usecase}}KafkaOutboxSink implements {{usecase}}OutboxSink {
    private final {{publisher}} publisher;

    public {{usecase}}KafkaOutboxSink({{publisher}} publisher) {
        this.publisher = publisher;
    }

    @Override public String name() { return "kafka"; }
    @Override public void deliver({{event}} event) { publisher.publish(event).join(); }
}
