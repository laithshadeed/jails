package com.example.intercom.web;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.domain.Workspace;
import com.example.intercom.service.WorkspaceService;
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
 * Scope-safe creation endpoint for {@link Workspace}.
 *
 * <p>The broad list, id lookup and delete routes are intentionally absent:
 * a plain repository operation cannot prove a tenant boundary. Generate an
 * {@code @scope} query or use case for each authorized operation instead.
 */
@RestController
@RequestMapping(WorkspaceController.PATH)
public class WorkspaceController {

    public static final String PATH = "/workspaces";

    private final WorkspaceService service;
    private final ScopeAuthorizer scopeAuthorizer;

    public WorkspaceController(WorkspaceService service, ScopeAuthorizer scopeAuthorizer) {
        this.service = Objects.requireNonNull(service, "service is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public ResponseEntity<WorkspaceResponse> create(
            @Valid @RequestBody WorkspaceRequest request,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "id", request.id());
        Workspace created = service.create(request.toDomain());
        return ResponseEntity.created(URI.create(PATH + "/" + created.id()))
                 .body(WorkspaceResponse.from(created));
    }
}
