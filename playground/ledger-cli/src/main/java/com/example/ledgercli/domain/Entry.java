package com.example.ledgercli.domain;

import java.time.LocalDate;
import java.util.Objects;
import java.util.Optional;

/**
 * An immutable Entry value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 *
 * <p>An {@code Optional} component is absence in the type rather than a
 * null nobody checks. Passing {@code null} for one means absent.
 */
public record Entry(String reference, LocalDate postedAt, Money amount, Optional<String> memo) {

    public Entry {
        Objects.requireNonNull(reference, "reference");
        Objects.requireNonNull(postedAt, "postedAt");
        Objects.requireNonNull(amount, "amount");
        memo = Objects.requireNonNullElse(memo, Optional.empty());
        reference = reference.trim();
        if (reference.isEmpty()) {
            throw new IllegalArgumentException("reference must not be blank");
        }
    }
}
