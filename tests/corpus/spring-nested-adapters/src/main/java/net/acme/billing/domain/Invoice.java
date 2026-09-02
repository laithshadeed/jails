package net.acme.billing.domain;

import java.math.BigDecimal;

/** An invoice as this codebase already models it. */
public record Invoice(String reference, BigDecimal net, BigDecimal vat) {

    public Invoice {
        if (reference == null || reference.isBlank()) {
            throw new IllegalArgumentException("reference is required");
        }
    }

    /** Hand-written, and nothing jails generates may remove it. */
    public BigDecimal gross() {
        return net.add(vat);
    }
}
