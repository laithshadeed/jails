package com.example.intercom.web;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.service.MembersByWorkspaceQuery;
import com.example.intercom.service.MembersByWorkspaceQueryPort;
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
@RequestMapping(MembersByWorkspaceQueryController.PATH)
public final class MembersByWorkspaceQueryController {

    public static final String PATH = "/queries/members-by-workspace";

    private final MembersByWorkspaceQueryPort queryPort;
    private final ScopeAuthorizer scopeAuthorizer;

    public MembersByWorkspaceQueryController(MembersByWorkspaceQueryPort queryPort, ScopeAuthorizer scopeAuthorizer) {
        this.queryPort = Objects.requireNonNull(queryPort, "queryPort is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public List<MemberResponse> execute(
            @Valid @RequestBody MembersByWorkspaceQuery query,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "workspaceId", query.workspaceId());
        return queryPort.execute(query).stream().map(MemberResponse::from).toList();
    }
}
