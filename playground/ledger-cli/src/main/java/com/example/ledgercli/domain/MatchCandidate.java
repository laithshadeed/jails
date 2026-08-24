package com.example.ledgercli.domain;

import java.util.Objects;

/**
 * An immutable MatchCandidate value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record MatchCandidate(Entry left, Entry right) {

    public MatchCandidate {
        Objects.requireNonNull(left, "left");
        Objects.requireNonNull(right, "right");
    }
}
