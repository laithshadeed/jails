package com.example.ledgercli.domain;

import java.util.Objects;

/**
 * A validated Money value.
 *
 * <p>All validation lives in the compact constructor, which runs before the
 * components are assigned -- so there is no way to reach an instance that
 * skipped it, not even through deserialisation or a copy.
 *
 * <p>Text marked {@code !} is trimmed and then required to be non-blank: a
 * present-but-empty value passes every null check downstream, which is
 * exactly why it is worth rejecting here instead.
 */
public record Money(long amountMinor, String currency) {

    public Money {
        Objects.requireNonNull(currency, "currency is required");
        currency = currency.trim();
        if (currency.isEmpty()) {
            throw new IllegalArgumentException("currency must not be blank");
        }
    }

    /**
     * Builds a Money. Identical to the constructor today; it exists so that
     * parsing, defaulting or a cache can be added later without changing a
     * single call site.
     */
    public static Money of(long amountMinor, String currency) {
        return new Money(amountMinor, currency);
    }
}
