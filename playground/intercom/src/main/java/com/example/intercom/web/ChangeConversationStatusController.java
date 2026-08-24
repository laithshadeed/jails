package com.example.intercom.web;

import static org.springframework.http.HttpStatus.CONFLICT;
import static org.springframework.http.HttpStatus.NOT_FOUND;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.service.ChangeConversationStatusCommand;
import com.example.intercom.service.ChangeConversationStatusUseCase;
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
@RequestMapping(ChangeConversationStatusController.PATH)
public final class ChangeConversationStatusController {

    public static final String PATH = "/actions/change-conversation-status";
    private final ChangeConversationStatusUseCase useCase;
    private final ScopeAuthorizer scopeAuthorizer;

    public ChangeConversationStatusController(ChangeConversationStatusUseCase useCase, ScopeAuthorizer scopeAuthorizer) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PutMapping
    public ConversationResponse execute(
            @Valid @RequestBody ChangeConversationStatusCommand command,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "workspaceId", command.workspaceId());
        try {
            return ConversationResponse.from(useCase.execute(command));
        } catch (ChangeConversationStatusUseCase.NotFoundException missing) {
            throw new ResponseStatusException(NOT_FOUND, missing.getMessage(), missing);
        } catch (ChangeConversationStatusUseCase.StaleVersionException stale) {
            throw new ResponseStatusException(CONFLICT, stale.getMessage(), stale);
        }
    }
}
