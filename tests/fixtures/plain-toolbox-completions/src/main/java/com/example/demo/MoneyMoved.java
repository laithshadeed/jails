package com.example.demo;

/** A movement of a strictly positive amount of money. */
public final class MoneyMoved {

    private final long amountMinor;

    public MoneyMoved(long amountMinor) {
        if (amountMinor <= 0) {
            throw new IllegalArgumentException("amountMinor must be positive");
        }
        this.amountMinor = amountMinor;
    }

    public long amountMinor() {
        return amountMinor;
    }
}
