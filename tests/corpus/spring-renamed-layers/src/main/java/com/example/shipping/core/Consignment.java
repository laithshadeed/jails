package com.example.shipping.core;

/** This codebase calls its domain layer `core`, and has since before jails. */
public record Consignment(String reference, int parcels) {

    public Consignment {
        if (parcels < 1) {
            throw new IllegalArgumentException("a consignment has at least one parcel");
        }
    }
}
