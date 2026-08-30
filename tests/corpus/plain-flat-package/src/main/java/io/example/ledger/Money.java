package io.example.ledger;

/**
 * Hand-written, in the same flat package everything else lands in.
 *
 * <p>Kept deliberately un-jails-shaped -- a class with a body, a static
 * factory and no annotations -- because the question this corpus asks is what
 * jails does *around* code it did not write.
 */
public record Money(long minor, String currency) {

    public static Money of(long minor, String currency) {
        return new Money(minor, currency);
    }

    public Money plus(Money other) {
        if (!currency.equals(other.currency)) {
            throw new IllegalArgumentException("cannot add " + currency + " to " + other.currency);
        }
        return new Money(minor + other.minor, currency);
    }
}
