package com.example.paymentsgateway.web;

import com.example.paymentsgateway.ScopeAuthorizer;
import com.example.paymentsgateway.domain.Payment;
import com.example.paymentsgateway.service.PaymentService;
import jakarta.validation.Valid;
import java.net.URI;
import java.util.Objects;
import org.springframework.http.ResponseEntity;
import org.springframework.security.core.Authentication;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/**
 * Scope-safe creation endpoint for {@link Payment}.
 *
 * <p>The broad list, id lookup and delete routes are intentionally absent:
 * a plain repository operation cannot prove a tenant boundary. Generate an
 * {@code @scope} query or use case for each authorized operation instead.
 */
@RestController
@RequestMapping(PaymentController.PATH)
public class PaymentController {

    public static final String PATH = "/payments";

    private final PaymentService service;
    private final ScopeAuthorizer scopeAuthorizer;

    public PaymentController(PaymentService service, ScopeAuthorizer scopeAuthorizer) {
        this.service = Objects.requireNonNull(service, "service is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public ResponseEntity<PaymentResponse> create(
            @Valid @RequestBody PaymentRequest request,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "merchantId", request.merchantId());
        Payment created = service.create(request.toDomain());
        return ResponseEntity.created(URI.create(PATH + "/" + created.id()))
                 .body(PaymentResponse.from(created));
    }
}
