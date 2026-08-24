package com.example.paymentsgateway.web;

import com.example.paymentsgateway.ScopeAuthorizer;
import com.example.paymentsgateway.service.PaymentsByStatusQuery;
import com.example.paymentsgateway.service.PaymentsByStatusQueryPort;
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
@RequestMapping(PaymentsByStatusQueryController.PATH)
public final class PaymentsByStatusQueryController {

    public static final String PATH = "/queries/payments-by-status";

    private final PaymentsByStatusQueryPort queryPort;
    private final ScopeAuthorizer scopeAuthorizer;

    public PaymentsByStatusQueryController(PaymentsByStatusQueryPort queryPort, ScopeAuthorizer scopeAuthorizer) {
        this.queryPort = Objects.requireNonNull(queryPort, "queryPort is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public List<PaymentResponse> execute(
            @Valid @RequestBody PaymentsByStatusQuery query,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "merchantId", query.merchantId());
        return queryPort.execute(query).stream().map(PaymentResponse::from).toList();
    }
}
