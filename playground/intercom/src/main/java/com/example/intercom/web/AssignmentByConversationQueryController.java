package com.example.intercom.web;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.service.AssignmentByConversationQuery;
import com.example.intercom.service.AssignmentByConversationQueryPort;
import jakarta.validation.Valid;
import java.util.List;
import java.util.Objects;
import org.springframework.security.core.Authentication;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for a typed read-side port. */
@RestController
@RequestMapping(AssignmentByConversationQueryController.PATH)
public final class AssignmentByConversationQueryController {

    public static final String PATH = "/queries/assignment-by-conversation";

    private final AssignmentByConversationQueryPort queryPort;
    private final ScopeAuthorizer scopeAuthorizer;

    public AssignmentByConversationQueryController(AssignmentByConversationQueryPort queryPort, ScopeAuthorizer scopeAuthorizer) {
        this.queryPort = Objects.requireNonNull(queryPort, "queryPort is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public List<ConversationAssignmentResponse> execute(
            @Valid @RequestBody AssignmentByConversationQuery query,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "workspaceId", query.workspaceId());
        return queryPort.execute(query).stream().map(ConversationAssignmentResponse::from).toList();
    }
}
