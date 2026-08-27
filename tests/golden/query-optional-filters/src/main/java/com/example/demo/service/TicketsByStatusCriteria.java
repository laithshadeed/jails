package com.example.demo.service;

import java.util.Objects;
import java.util.Optional;

/**
 * Typed filters for the TicketsByStatus query.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 *
 * <p>An {@code Optional} component is absence in the type rather than a
 * null nobody checks. Passing {@code null} for one means absent.
 */
public record TicketsByStatusCriteria(String status, Optional<String> category) {

    public TicketsByStatusCriteria {
        Objects.requireNonNull(status, "status");
        category = Objects.requireNonNullElse(category, Optional.empty());
        status = status.trim();
        if (status.isEmpty()) {
            throw new IllegalArgumentException("status must not be blank");
        }
    }
}
