package com.example.demo.service;

import com.example.demo.domain.Payout;

/** Atomic state change guarded by tenant scope and an optimistic version. */
@FunctionalInterface
public interface ChangePayoutStatusUseCase {

    Payout execute(ChangePayoutStatusCommand command);

    final class NotFoundException extends RuntimeException {
        public NotFoundException() { super("resource not found in the authorized scope"); }
    }

    final class StaleVersionException extends RuntimeException {
        public StaleVersionException() { super("resource version is stale"); }
    }
}
