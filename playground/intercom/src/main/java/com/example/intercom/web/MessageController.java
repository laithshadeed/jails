package com.example.intercom.web;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.domain.Message;
import com.example.intercom.service.MessageService;
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
 * Scope-safe creation endpoint for {@link Message}.
 *
 * <p>The broad list, id lookup and delete routes are intentionally absent:
 * a plain repository operation cannot prove a tenant boundary. Generate an
 * {@code @scope} query or use case for each authorized operation instead.
 */
@RestController
@RequestMapping(MessageController.PATH)
public class MessageController {

    public static final String PATH = "/messages";

    private final MessageService service;
    private final ScopeAuthorizer scopeAuthorizer;

    public MessageController(MessageService service, ScopeAuthorizer scopeAuthorizer) {
        this.service = Objects.requireNonNull(service, "service is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public ResponseEntity<MessageResponse> create(
            @Valid @RequestBody MessageRequest request,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "workspaceId", request.workspaceId());
        Message created = service.create(request.toDomain());
        return ResponseEntity.created(URI.create(PATH + "/" + created.id()))
                 .body(MessageResponse.from(created));
    }
}
