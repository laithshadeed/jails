package com.example.intercom.web;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.service.InboxesByWorkspaceQuery;
import com.example.intercom.service.InboxesByWorkspaceQueryPort;
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
@RequestMapping(InboxesByWorkspaceQueryController.PATH)
public final class InboxesByWorkspaceQueryController {

    public static final String PATH = "/queries/inboxes-by-workspace";

    private final InboxesByWorkspaceQueryPort queryPort;
    private final ScopeAuthorizer scopeAuthorizer;

    public InboxesByWorkspaceQueryController(InboxesByWorkspaceQueryPort queryPort, ScopeAuthorizer scopeAuthorizer) {
        this.queryPort = Objects.requireNonNull(queryPort, "queryPort is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public List<InboxResponse> execute(
            @Valid @RequestBody InboxesByWorkspaceQuery query,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "workspaceId", query.workspaceId());
        return queryPort.execute(query).stream().map(InboxResponse::from).toList();
    }
}
