package com.example.demo.service;

import com.example.demo.domain.Payout;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface RequestPayoutUseCase {

    Payout execute(RequestPayoutCommand command);
}
