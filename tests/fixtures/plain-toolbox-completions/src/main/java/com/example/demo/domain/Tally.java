package com.example.demo.domain;

/** Counts hits and their accumulated total, neither of which may be negative. */
public record Tally(int hits, long total) {

    public Tally {
        if (hits < 0) {
            throw new IllegalArgumentException("hits must be nonnegative");
        }
        if (total < 0) {
            throw new IllegalArgumentException("total must be nonnegative");
        }
    }
}
