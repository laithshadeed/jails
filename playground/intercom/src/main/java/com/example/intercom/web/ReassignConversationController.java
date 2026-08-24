package com.example.intercom.web;

import static org.springframework.http.HttpStatus.CONFLICT;
import static org.springframework.http.HttpStatus.NOT_FOUND;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.service.ReassignConversationCommand;
import com.example.intercom.service.ReassignConversationUseCase;
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
@RequestMapping(ReassignConversationController.PATH)
public final class ReassignConversationController {

    public static final String PATH = "/actions/reassign-conversation";
    private final ReassignConversationUseCase useCase;
    private final ScopeAuthorizer scopeAuthorizer;

    public ReassignConversationController(ReassignConversationUseCase useCase, ScopeAuthorizer scopeAuthorizer) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PutMapping
    public ConversationAssignmentResponse execute(
            @Valid @RequestBody ReassignConversationCommand command,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "workspaceId", command.workspaceId());
        try {
            return ConversationAssignmentResponse.from(useCase.execute(command));
        } catch (ReassignConversationUseCase.NotFoundException missing) {
            throw new ResponseStatusException(NOT_FOUND, missing.getMessage(), missing);
        } catch (ReassignConversationUseCase.StaleVersionException stale) {
            throw new ResponseStatusException(CONFLICT, stale.getMessage(), stale);
        }
    }
}
