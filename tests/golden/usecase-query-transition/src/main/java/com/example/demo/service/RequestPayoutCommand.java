package com.example.demo.service;

import java.util.Objects;
import java.util.UUID;

/**
 * Validated input for the RequestPayout use case.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record RequestPayoutCommand(UUID id, long amount) {

    public RequestPayoutCommand {
        Objects.requireNonNull(id, "id");
    }
}
