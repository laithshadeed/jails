package com.example.demo.service;

import com.example.demo.domain.PayoutStatus;
import java.util.Objects;
import java.util.UUID;

/**
 * Validated input for the ChangePayoutStatus use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record ChangePayoutStatusCommand(UUID id, PayoutStatus status) {

    public ChangePayoutStatusCommand {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(status, "status");
    }
}
