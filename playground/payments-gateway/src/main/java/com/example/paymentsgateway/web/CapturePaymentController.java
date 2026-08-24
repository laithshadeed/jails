package com.example.paymentsgateway.web;

import static org.springframework.http.HttpStatus.CONFLICT;
import static org.springframework.http.HttpStatus.NOT_FOUND;

import com.example.paymentsgateway.ScopeAuthorizer;
import com.example.paymentsgateway.service.CapturePaymentCommand;
import com.example.paymentsgateway.service.CapturePaymentUseCase;
import jakarta.validation.Valid;
import java.util.Objects;
import org.springframework.security.core.Authentication;
import org.springframework.web.bind.annotation.PutMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;

/** HTTP adapter for one optimistic state transition. */
@RestController
@RequestMapping(CapturePaymentController.PATH)
public final class CapturePaymentController {

    public static final String PATH = "/actions/capture-payment";
    private final CapturePaymentUseCase useCase;
    private final ScopeAuthorizer scopeAuthorizer;

    public CapturePaymentController(CapturePaymentUseCase useCase, ScopeAuthorizer scopeAuthorizer) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PutMapping
    public PaymentResponse execute(
            @Valid @RequestBody CapturePaymentCommand command,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "merchantId", command.merchantId());
        try {
            return PaymentResponse.from(useCase.execute(command));
        } catch (CapturePaymentUseCase.NotFoundException missing) {
            throw new ResponseStatusException(NOT_FOUND, missing.getMessage(), missing);
        } catch (CapturePaymentUseCase.StaleVersionException stale) {
            throw new ResponseStatusException(CONFLICT, stale.getMessage(), stale);
        }
    }
}
