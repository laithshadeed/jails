package {{pkg}};

import java.util.List;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.kafka.annotation.KafkaListener;
import org.springframework.stereotype.Component;

/**
 * Consumes {@link {{name}}Event} and hands it to every {@link {{name}}Handler}.
 *
 * <p>The listener is deliberately thin. Business logic inside a listener is
 * unreachable from any test that does not start a broker, and unreusable from
 * any other entry point -- so the reaction lives behind the handler port and
 * this class only routes to it.
 *
 * <p>Spring injects every {@code @Component} implementing the port. With none
 * registered the list is empty, and the first record says so at {@code WARN}
 * rather than being logged as received and then dropped: a consumer that
 * silently discards a topic is indistinguishable from one that is working.
 *
 * <p>Nothing here catches exceptions. That is the right default -- a thrown
 * exception means the offset is not committed, so the message is retried and
 * eventually goes to a dead-letter topic if one is configured. Swallowing it
 * would acknowledge a message that was never processed, which is data loss
 * that looks like success.
 */
@Component
public class {{name}}Listener {

    private static final Logger log = LoggerFactory.getLogger({{name}}Listener.class);

    private final List<{{name}}Handler> handlers;

    public {{name}}Listener(List<{{name}}Handler> handlers) {
        this.handlers = List.copyOf(handlers);
    }

    @KafkaListener(topics = "${topics.{{topic}}:{{topic}}}")
    public void on({{name}}Event event) {
        if (handlers.isEmpty()) {
            // The whole record, not its id: an event is not required to carry
            // one, and a listener that names a component the payload may not
            // have is a listener that does not compile.
            log.warn("no {{name}}Handler is registered: {} was consumed and discarded", event);
            return;
        }
        for ({{name}}Handler handler : handlers) {
            handler.handle(event);
        }
    }
}
