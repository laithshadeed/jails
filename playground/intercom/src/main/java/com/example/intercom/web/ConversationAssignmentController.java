package com.example.intercom.web;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.domain.ConversationAssignment;
import com.example.intercom.service.ConversationAssignmentService;
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
 * Scope-safe creation endpoint for {@link ConversationAssignment}.
 *
 * <p>The broad list, id lookup and delete routes are intentionally absent:
 * a plain repository operation cannot prove a tenant boundary. Generate an
 * {@code @scope} query or use case for each authorized operation instead.
 */
@RestController
@RequestMapping(ConversationAssignmentController.PATH)
public class ConversationAssignmentController {

    public static final String PATH = "/conversation-assignments";

    private final ConversationAssignmentService service;
    private final ScopeAuthorizer scopeAuthorizer;

    public ConversationAssignmentController(ConversationAssignmentService service, ScopeAuthorizer scopeAuthorizer) {
        this.service = Objects.requireNonNull(service, "service is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public ResponseEntity<ConversationAssignmentResponse> create(
            @Valid @RequestBody ConversationAssignmentRequest request,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "workspaceId", request.workspaceId());
        ConversationAssignment created = service.create(request.toDomain());
        return ResponseEntity.created(URI.create(PATH + "/" + created.id()))
                 .body(ConversationAssignmentResponse.from(created));
    }
}
