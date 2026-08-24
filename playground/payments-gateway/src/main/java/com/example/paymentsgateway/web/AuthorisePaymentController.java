package com.example.paymentsgateway.web;

import com.example.paymentsgateway.ScopeAuthorizer;
import com.example.paymentsgateway.service.AuthorisePaymentCommand;
import com.example.paymentsgateway.service.AuthorisePaymentUseCase;
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
@RequestMapping(AuthorisePaymentController.PATH)
public final class AuthorisePaymentController {

    public static final String PATH = "/actions/authorise-payment";
    private static final String RESOURCE_PATH = "/payments";

    private final AuthorisePaymentUseCase useCase;
    private final ScopeAuthorizer scopeAuthorizer;

    public AuthorisePaymentController(AuthorisePaymentUseCase useCase, ScopeAuthorizer scopeAuthorizer) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public ResponseEntity<PaymentResponse> execute(
            @Valid @RequestBody AuthorisePaymentCommand command,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "merchantId", command.merchantId());
        var created = useCase.execute(command);
        return ResponseEntity.created(URI.create(RESOURCE_PATH + "/" + created.id()))
                .body(PaymentResponse.from(created));
    }
}
