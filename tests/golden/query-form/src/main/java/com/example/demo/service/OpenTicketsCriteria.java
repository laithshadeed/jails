package com.example.demo.service;

import java.util.Objects;
import java.util.Optional;

/**
 * Typed filters for the OpenTickets query.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 *
 * <p>An {@code Optional} component is absence in the type rather than a
 * null nobody checks. Passing {@code null} for one means absent.
 */
public record OpenTicketsCriteria(Optional<String> status) {

    public OpenTicketsCriteria {
        status = Objects.requireNonNullElse(status, Optional.empty());
    }
}
