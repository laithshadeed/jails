package com.example.roster.model;

import java.time.Instant;

/** This project calls its domain layer `model`, which is in the synonym table. */
public record Shift(String id, Instant startsAt, Instant endsAt) {

    public Shift {
        if (endsAt.isBefore(startsAt)) {
            throw new IllegalArgumentException("a shift ends after it starts");
        }
    }
}
