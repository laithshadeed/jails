package com.example.intercom.web;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.domain.InboxMember;
import com.example.intercom.service.InboxMemberService;
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
 * Scope-safe creation endpoint for {@link InboxMember}.
 *
 * <p>The broad list, id lookup and delete routes are intentionally absent:
 * a plain repository operation cannot prove a tenant boundary. Generate an
 * {@code @scope} query or use case for each authorized operation instead.
 */
@RestController
@RequestMapping(InboxMemberController.PATH)
public class InboxMemberController {

    public static final String PATH = "/inbox-members";

    private final InboxMemberService service;
    private final ScopeAuthorizer scopeAuthorizer;

    public InboxMemberController(InboxMemberService service, ScopeAuthorizer scopeAuthorizer) {
        this.service = Objects.requireNonNull(service, "service is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public ResponseEntity<InboxMemberResponse> create(
            @Valid @RequestBody InboxMemberRequest request,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "workspaceId", request.workspaceId());
        InboxMember created = service.create(request.toDomain());
        return ResponseEntity.created(URI.create(PATH + "/" + created.id()))
                 .body(InboxMemberResponse.from(created));
    }
}
