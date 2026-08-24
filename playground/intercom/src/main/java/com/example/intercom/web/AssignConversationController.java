package com.example.intercom.web;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.service.AssignConversationCommand;
import com.example.intercom.service.AssignConversationUseCase;
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
@RequestMapping(AssignConversationController.PATH)
public final class AssignConversationController {

    public static final String PATH = "/actions/assign-conversation";
    private static final String RESOURCE_PATH = "/conversation-assignments";

    private final AssignConversationUseCase useCase;
    private final ScopeAuthorizer scopeAuthorizer;

    public AssignConversationController(AssignConversationUseCase useCase, ScopeAuthorizer scopeAuthorizer) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public ResponseEntity<ConversationAssignmentResponse> execute(
            @Valid @RequestBody AssignConversationCommand command,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "workspaceId", command.workspaceId());
        var created = useCase.execute(command);
        return ResponseEntity.created(URI.create(RESOURCE_PATH + "/" + created.id()))
                .body(ConversationAssignmentResponse.from(created));
    }
}
