package com.example.demo.domain;

import java.util.Objects;
import java.util.UUID;

/** A transaction whose country determines whether domestic rules apply. */
public record Transaction(UUID id, long amount, String country) {

    public Transaction {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(country, "country");
        country = country.trim();
        if (country.isEmpty()) {
            throw new IllegalArgumentException("country must not be blank");
        }
    }

    /** Keeps the generated record's original construction shape for callers without country data. */
    public Transaction(UUID id, long amount) {
        this(id, amount, "GB");
    }
}
