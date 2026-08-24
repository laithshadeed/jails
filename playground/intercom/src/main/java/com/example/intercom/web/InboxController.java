package com.example.intercom.web;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.domain.Inbox;
import com.example.intercom.service.InboxService;
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
 * Scope-safe creation endpoint for {@link Inbox}.
 *
 * <p>The broad list, id lookup and delete routes are intentionally absent:
 * a plain repository operation cannot prove a tenant boundary. Generate an
 * {@code @scope} query or use case for each authorized operation instead.
 */
@RestController
@RequestMapping(InboxController.PATH)
public class InboxController {

    public static final String PATH = "/inboxes";

    private final InboxService service;
    private final ScopeAuthorizer scopeAuthorizer;

    public InboxController(InboxService service, ScopeAuthorizer scopeAuthorizer) {
        this.service = Objects.requireNonNull(service, "service is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public ResponseEntity<InboxResponse> create(
            @Valid @RequestBody InboxRequest request,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "workspaceId", request.workspaceId());
        Inbox created = service.create(request.toDomain());
        return ResponseEntity.created(URI.create(PATH + "/" + created.id()))
                 .body(InboxResponse.from(created));
    }
}
