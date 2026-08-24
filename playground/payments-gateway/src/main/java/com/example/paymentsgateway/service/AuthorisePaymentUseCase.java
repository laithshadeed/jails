package com.example.paymentsgateway.service;

import com.example.paymentsgateway.domain.Payment;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface AuthorisePaymentUseCase {

    Payment execute(AuthorisePaymentCommand command);
}
