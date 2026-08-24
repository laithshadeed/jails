package com.example.ledgercli.testkit;

import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Supplier;

/**
 * Deterministic identifiers.
 *
 * <p>The counterpart to {@link Clocks}: code that takes a
 * {@code Supplier<String>} instead of calling {@code UUID.randomUUID()} can
 * have its output asserted in full, identifiers included.
 */
public final class Ids {

    private Ids() {}

    /** Yields {@code prefix-1}, {@code prefix-2}, ... */
    public static Supplier<String> sequential(String prefix, int start) {
        var next = new AtomicInteger(start);
        return () -> prefix + "-" + next.getAndIncrement();
    }

    public static Supplier<String> sequential(String prefix) {
        return sequential(prefix, 1);
    }
}
