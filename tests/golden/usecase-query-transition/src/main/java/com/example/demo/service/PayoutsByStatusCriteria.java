package com.example.demo.service;

import com.example.demo.domain.PayoutStatus;
import java.util.Objects;

/**
 * Typed filters for the PayoutsByStatus query.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record PayoutsByStatusCriteria(PayoutStatus status) {

    public PayoutsByStatusCriteria {
        Objects.requireNonNull(status, "status");
    }
}
