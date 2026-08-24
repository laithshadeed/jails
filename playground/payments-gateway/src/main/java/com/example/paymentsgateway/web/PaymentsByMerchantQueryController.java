package com.example.paymentsgateway.web;

import com.example.paymentsgateway.ScopeAuthorizer;
import com.example.paymentsgateway.service.PaymentsByMerchantQuery;
import com.example.paymentsgateway.service.PaymentsByMerchantQueryPort;
import jakarta.validation.Valid;
import java.util.List;
import java.util.Objects;
import org.springframework.security.core.Authentication;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for a typed read-side port. */
@RestController
@RequestMapping(PaymentsByMerchantQueryController.PATH)
public final class PaymentsByMerchantQueryController {

    public static final String PATH = "/queries/payments-by-merchant";

    private final PaymentsByMerchantQueryPort queryPort;
    private final ScopeAuthorizer scopeAuthorizer;

    public PaymentsByMerchantQueryController(PaymentsByMerchantQueryPort queryPort, ScopeAuthorizer scopeAuthorizer) {
        this.queryPort = Objects.requireNonNull(queryPort, "queryPort is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public List<PaymentResponse> execute(
            @Valid @RequestBody PaymentsByMerchantQuery query,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "merchantId", query.merchantId());
        return queryPort.execute(query).stream().map(PaymentResponse::from).toList();
    }
}
