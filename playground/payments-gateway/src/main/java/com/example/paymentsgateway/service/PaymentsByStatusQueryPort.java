package com.example.paymentsgateway.service;

import com.example.paymentsgateway.domain.Payment;
import java.util.List;

/** Read-side port; the application contract contains no JDBC or HTTP types. */
@FunctionalInterface
public interface PaymentsByStatusQueryPort {

    List<Payment> execute(PaymentsByStatusQuery query);
}
