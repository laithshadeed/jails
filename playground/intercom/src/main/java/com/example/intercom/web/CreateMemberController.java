package com.example.intercom.web;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.service.CreateMemberCommand;
import com.example.intercom.service.CreateMemberUseCase;
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
@RequestMapping(CreateMemberController.PATH)
public final class CreateMemberController {

    public static final String PATH = "/actions/create-member";
    private static final String RESOURCE_PATH = "/members";

    private final CreateMemberUseCase useCase;
    private final ScopeAuthorizer scopeAuthorizer;

    public CreateMemberController(CreateMemberUseCase useCase, ScopeAuthorizer scopeAuthorizer) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public ResponseEntity<MemberResponse> execute(
            @Valid @RequestBody CreateMemberCommand command,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "workspaceId", command.workspaceId());
        var created = useCase.execute(command);
        return ResponseEntity.created(URI.create(RESOURCE_PATH + "/" + created.id()))
                .body(MemberResponse.from(created));
    }
}
