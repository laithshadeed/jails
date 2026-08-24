package com.example.intercom.web;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.service.MessagesByConversationQuery;
import com.example.intercom.service.MessagesByConversationQueryPort;
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
@RequestMapping(MessagesByConversationQueryController.PATH)
public final class MessagesByConversationQueryController {

    public static final String PATH = "/queries/messages-by-conversation";

    private final MessagesByConversationQueryPort queryPort;
    private final ScopeAuthorizer scopeAuthorizer;

    public MessagesByConversationQueryController(MessagesByConversationQueryPort queryPort, ScopeAuthorizer scopeAuthorizer) {
        this.queryPort = Objects.requireNonNull(queryPort, "queryPort is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public List<MessageResponse> execute(
            @Valid @RequestBody MessagesByConversationQuery query,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "workspaceId", query.workspaceId());
        return queryPort.execute(query).stream().map(MessageResponse::from).toList();
    }
}
