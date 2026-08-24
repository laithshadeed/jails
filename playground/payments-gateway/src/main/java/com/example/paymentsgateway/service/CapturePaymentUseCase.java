package com.example.paymentsgateway.service;

import com.example.paymentsgateway.domain.Payment;

/** Atomic state change guarded by tenant scope and an optimistic version. */
@FunctionalInterface
public interface CapturePaymentUseCase {

    Payment execute(CapturePaymentCommand command);

    final class NotFoundException extends RuntimeException {
        public NotFoundException() { super("resource not found in the authorized scope"); }
    }

    final class StaleVersionException extends RuntimeException {
        public StaleVersionException() { super("resource version is stale"); }
    }
}
