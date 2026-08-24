package com.example.intercom.web;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.service.InboxMembersByInboxQuery;
import com.example.intercom.service.InboxMembersByInboxQueryPort;
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
@RequestMapping(InboxMembersByInboxQueryController.PATH)
public final class InboxMembersByInboxQueryController {

    public static final String PATH = "/queries/inbox-members-by-inbox";

    private final InboxMembersByInboxQueryPort queryPort;
    private final ScopeAuthorizer scopeAuthorizer;

    public InboxMembersByInboxQueryController(InboxMembersByInboxQueryPort queryPort, ScopeAuthorizer scopeAuthorizer) {
        this.queryPort = Objects.requireNonNull(queryPort, "queryPort is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public List<InboxMemberResponse> execute(
            @Valid @RequestBody InboxMembersByInboxQuery query,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "workspaceId", query.workspaceId());
        return queryPort.execute(query).stream().map(InboxMemberResponse::from).toList();
    }
}
