package com.example.paymentsgateway.web;

import com.example.paymentsgateway.ScopeAuthorizer;
import com.example.paymentsgateway.service.RefundPaymentRequestCommand;
import com.example.paymentsgateway.service.RefundPaymentRequestUseCase;
import jakarta.validation.Valid;
import java.net.URI;
import java.util.Objects;
import org.springframework.http.ResponseEntity;
import org.springframework.security.core.Authentication;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for one application use case; the operation itself knows nothing about HTTP. */
@RestController
@RequestMapping(RefundPaymentRequestController.PATH)
public final class RefundPaymentRequestController {

    public static final String PATH = "/actions/refund-payment-request";
    private static final String RESOURCE_PATH = "/refunds";

    private final RefundPaymentRequestUseCase useCase;
    private final ScopeAuthorizer scopeAuthorizer;

    public RefundPaymentRequestController(RefundPaymentRequestUseCase useCase, ScopeAuthorizer scopeAuthorizer) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public ResponseEntity<RefundResponse> execute(
            @Valid @RequestBody RefundPaymentRequestCommand command,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "merchantId", command.merchantId());
        var created = useCase.execute(command);
        return ResponseEntity.created(URI.create(RESOURCE_PATH + "/" + created.id()))
                .body(RefundResponse.from(created));
    }
}
