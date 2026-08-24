package com.example.ledgercli.domain;

/**
 * The MatchOutcome values this application understands.
 *
 * <p>A closed set, so a switch over it is checked for exhaustiveness and
 * adding a constant makes the compiler point at every place that has to
 * handle it.
 */
public enum MatchOutcome {
    MATCHED,
    AMOUNT_DIFFERS,
    DATE_DIFFERS,
    UNMATCHED
}
