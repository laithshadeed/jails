package com.example.ledgercli.domain;

import static org.assertj.core.api.Assertions.assertThat;

import org.junit.jupiter.api.Test;

/**
 * The switch below has no {@code default} on purpose: adding a variant
 * should break this test at compile time, which is the whole reason to seal
 * the type in the first place.
 */
class LedgerErrorTest {

    private String describe(LedgerError result) {
        return switch (result) {
            case LedgerError.MalformedRow v -> "malformedrow";
            case LedgerError.UnknownCurrency v -> "unknowncurrency";
            case LedgerError.DuplicateReference v -> "duplicatereference";
        };
    }

    @Test
    void describesMalformedRow() {
        assertThat(describe(new LedgerError.MalformedRow())).isEqualTo("malformedrow");
    }

    @Test
    void describesUnknownCurrency() {
        assertThat(describe(new LedgerError.UnknownCurrency())).isEqualTo("unknowncurrency");
    }

    @Test
    void describesDuplicateReference() {
        assertThat(describe(new LedgerError.DuplicateReference())).isEqualTo("duplicatereference");
    }
}
