package {{pkg}};

import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.stereotype.Component;

/**
 * The destination a project has until it declares a real one.
 *
 * <p>The relay refuses to start with no sink at all, so something has to be
 * here: staging works from the first compile, and a project that cannot start
 * is worse than one that can. What it must not do is look like delivery. It
 * logs at WARN and says so, because a sink that quietly marked every event
 * SUCCEEDED would drop the whole topic with nothing to read.
 *
 * <p>Replace it: implement {@link {{usecase}}OutboxSink} and delete this class,
 * or {@code jails model eject} it and publish from here.
 */
@Component
public final class {{usecase}}LoggingOutboxSink implements {{usecase}}OutboxSink {

    private static final Logger log = LoggerFactory.getLogger({{usecase}}LoggingOutboxSink.class);

    @Override
    public String name() {
        return "log";
    }

    @Override
    public void deliver({{event}}Event event) {
        log.warn("no destination is configured for the {{usecase}} outbox; {} was written to the log only", event);
    }
}
