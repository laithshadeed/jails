package {{pkg}};

import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.time.ZoneId;
import java.time.ZoneOffset;

/**
 * Deterministic clocks.
 *
 * <p>These only work on code that accepts a {@link Clock} rather than calling
 * {@code Instant.now()}. That is the point: taking the clock as a parameter is
 * what makes a timestamp assertable at all.
 *
 * <p>{@code Clock.fixed} is already in the JDK, so only the stepping clock --
 * for asserting that events are ordered and distinct -- needs writing.
 */
public final class Clocks {

    /** An arbitrary, memorable instant. Deterministic is the only requirement. */
    public static final Instant DEFAULT_START = Instant.parse("2026-01-01T00:00:00Z");

    private Clocks() {}

    public static Clock fixed(Instant instant) {
        return Clock.fixed(instant, ZoneOffset.UTC);
    }

    public static Clock fixed() {
        return fixed(DEFAULT_START);
    }

    /** A clock that advances by {@code step} on every read. */
    public static Clock stepping(Instant start, Duration step) {
        return new SteppingClock(start, step, ZoneOffset.UTC);
    }

    public static Clock stepping() {
        return stepping(DEFAULT_START, Duration.ofSeconds(1));
    }

    private static final class SteppingClock extends Clock {

        private final Duration step;
        private final ZoneId zone;
        private Instant current;

        private SteppingClock(Instant start, Duration step, ZoneId zone) {
            this.current = start;
            this.step = step;
            this.zone = zone;
        }

        @Override
        public ZoneId getZone() {
            return zone;
        }

        @Override
        public Clock withZone(ZoneId other) {
            return new SteppingClock(current, step, other);
        }

        @Override
        public synchronized Instant instant() {
            var value = current;
            current = current.plus(step);
            return value;
        }
    }
}
