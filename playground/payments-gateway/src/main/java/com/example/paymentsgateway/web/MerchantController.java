package com.example.paymentsgateway.web;

import com.example.paymentsgateway.ScopeAuthorizer;
import com.example.paymentsgateway.domain.Merchant;
import com.example.paymentsgateway.service.MerchantService;
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
 * Scope-safe creation endpoint for {@link Merchant}.
 *
 * <p>The broad list, id lookup and delete routes are intentionally absent:
 * a plain repository operation cannot prove a tenant boundary. Generate an
 * {@code @scope} query or use case for each authorized operation instead.
 */
@RestController
@RequestMapping(MerchantController.PATH)
public class MerchantController {

    public static final String PATH = "/merchants";

    private final MerchantService service;
    private final ScopeAuthorizer scopeAuthorizer;

    public MerchantController(MerchantService service, ScopeAuthorizer scopeAuthorizer) {
        this.service = Objects.requireNonNull(service, "service is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public ResponseEntity<MerchantResponse> create(
            @Valid @RequestBody MerchantRequest request,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "id", request.id());
        Merchant created = service.create(request.toDomain());
        return ResponseEntity.created(URI.create(PATH + "/" + created.id()))
                 .body(MerchantResponse.from(created));
    }
}
