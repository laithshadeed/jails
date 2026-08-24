package com.example.paymentsgateway.service;

import com.example.paymentsgateway.domain.Refund;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface RefundPaymentRequestUseCase {

    Refund execute(RefundPaymentRequestCommand command);
}
