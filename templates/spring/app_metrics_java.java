package {{pkg}};

import io.micrometer.core.instrument.Counter;
import io.micrometer.core.instrument.MeterRegistry;
import io.micrometer.core.instrument.Timer;
import java.util.Objects;
import java.util.function.Supplier;
import org.springframework.stereotype.Component;

/**
 * The application's metrics, named in one place.
 *
 * <p>Not a wrapper for its own sake. A {@link MeterRegistry} injected
 * directly into every class means the name of a metric is a string literal at
 * each call site, and those drift -- {@code orders.created} in one place,
 * {@code order_created} in another -- until a dashboard silently stops
 * matching. Declaring each meter once, here, makes the name a compile-time
 * reference.
 *
 * <p>Meters are created eagerly in the constructor rather than per call.
 * {@code Counter.builder(...).register(...)} is idempotent but not free,
 * and a counter registered on first use does not appear in a scrape until
 * something has happened -- so a dashboard shows a gap rather than a zero,
 * which reads as "broken" instead of "quiet".
 */
@Component
public class AppMetrics {

    private final Counter requestsHandled;
    private final Timer workDuration;

    public AppMetrics(MeterRegistry registry) {
        Objects.requireNonNull(registry, "registry is required");
        // Dot-separated names: Micrometer translates them to each backend's
        // convention (Prometheus wants underscores) and doing that by hand
        // ties the code to one backend.
        this.requestsHandled = Counter.builder("app.requests.handled")
                .description("requests this application finished handling")
                .register(registry);
        this.workDuration = Timer.builder("app.work.duration")
                .description("how long the unit of work took")
                .register(registry);
    }

    public void requestHandled() {
        requestsHandled.increment();
    }

    /** Times {@code work} and returns its result. */
    public <T> T timed(Supplier<T> work) {
        return workDuration.record(work);
    }
}
