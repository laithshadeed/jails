package com.example.demo.domain;

import java.util.Objects;

/**
 * A validated Money value.
 *
 * <p>All validation lives in the compact constructor, which runs before the
 * components are assigned -- so there is no way to reach an instance that
 * skipped it, not even through deserialisation or a copy.
 */
public record Money(long amount, String currency) {

    public Money {
        Objects.requireNonNull(currency, "currency is required");
    }

    /**
     * Builds a Money. Identical to the constructor today; it exists so that
     * parsing, defaulting or a cache can be added later without changing a
     * single call site.
     */
    public static Money of(long amount, String currency) {
        return new Money(amount, currency);
    }
}
