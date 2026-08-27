package com.example.demo.domain;

import com.fasterxml.jackson.annotation.JsonCreator;
import com.fasterxml.jackson.annotation.JsonValue;

/**
 * The IssuePriority values this application understands.
 *
 * <p>A closed set, so a switch over it is checked for exhaustiveness and
 * adding a constant makes the compiler point at every place that has to
 * handle it.
 *
 * <p>The name and the wire value are two different things: the database
 * stores the name and the check constraint lists those, while a client
 * sees what {@code wire()} returns.
 */
public enum IssuePriority {
    NONE("-"),
    HIGH("!"),
    URGENT("!!");

    private final String wire;

    IssuePriority(String wire) {
        this.wire = wire;
    }

    /** What this constant is called outside the application. */
    @JsonValue
    public String wire() {
        return this.wire;
    }

    /**
     * The IssuePriority a client named, by wire value.
     *
     * <p>An unknown value throws, listing what it would have taken. A null
     * return here would be a request body that binds to null and fails
     * somewhere else entirely.
     */
    @JsonCreator
    public static IssuePriority fromWire(String value) {
        for (IssuePriority candidate : values()) {
            if (candidate.wire.equals(value)) {
                return candidate;
            }
        }
        throw new IllegalArgumentException(
                "no IssuePriority with wire value '" + value + "'; expected one of -, !, !!");
    }
}
