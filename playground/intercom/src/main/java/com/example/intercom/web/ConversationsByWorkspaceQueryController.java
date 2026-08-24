package com.example.intercom.web;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.service.ConversationsByWorkspaceQuery;
import com.example.intercom.service.ConversationsByWorkspaceQueryPort;
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
@RequestMapping(ConversationsByWorkspaceQueryController.PATH)
public final class ConversationsByWorkspaceQueryController {

    public static final String PATH = "/queries/conversations-by-workspace";

    private final ConversationsByWorkspaceQueryPort queryPort;
    private final ScopeAuthorizer scopeAuthorizer;

    public ConversationsByWorkspaceQueryController(ConversationsByWorkspaceQueryPort queryPort, ScopeAuthorizer scopeAuthorizer) {
        this.queryPort = Objects.requireNonNull(queryPort, "queryPort is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public List<ConversationResponse> execute(
            @Valid @RequestBody ConversationsByWorkspaceQuery query,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "workspaceId", query.workspaceId());
        return queryPort.execute(query).stream().map(ConversationResponse::from).toList();
    }
}
