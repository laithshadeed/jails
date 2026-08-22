package com.example.demo;

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

/**
 * Pending cases generated from docs/behaviour.md.
 *
 * <p>This is a todo list the build can read: every case fails loudly rather
 * than passing vacuously, and the class-level @Disabled keeps the suite green
 * meanwhile. Delete one @Disabled, make that case pass, move to the next.
 */
@DisplayName("behaviour")
@Disabled("todo: implement these cases")
class BehaviourTest {

    @Test
    @DisplayName("given a payout that is pending")
    void givenAPayoutThatIsPending() {
        throw new UnsupportedOperationException("todo");
    }

    @Test
    @DisplayName("when the provider confirms it")
    void whenTheProviderConfirmsIt() {
        throw new UnsupportedOperationException("todo");
    }

    @Test
    @DisplayName("then it is settled")
    void thenItIsSettled() {
        throw new UnsupportedOperationException("todo");
    }

    @Test
    @DisplayName("given a payout that is pending")
    void givenAPayoutThatIsPending2() {
        throw new UnsupportedOperationException("todo");
    }

    @Test
    @DisplayName("when the provider declines it")
    void whenTheProviderDeclinesIt() {
        throw new UnsupportedOperationException("todo");
    }

    @Test
    @DisplayName("then it is failed")
    void thenItIsFailed() {
        throw new UnsupportedOperationException("todo");
    }
}
