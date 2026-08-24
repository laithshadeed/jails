package com.example.paymentsgateway.domain;

/**
 * The PaymentStatus values this application understands.
 *
 * <p>A closed set, so a switch over it is checked for exhaustiveness and
 * adding a constant makes the compiler point at every place that has to
 * handle it.
 */
public enum PaymentStatus {
    AUTHORISED,
    CAPTURED,
    REFUNDED,
    REVERSED,
    DECLINED,
    FAILED
}
