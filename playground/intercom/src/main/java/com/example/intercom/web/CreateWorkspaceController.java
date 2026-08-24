package com.example.intercom.web;

import com.example.intercom.ScopeAuthorizer;
import com.example.intercom.service.CreateWorkspaceCommand;
import com.example.intercom.service.CreateWorkspaceUseCase;
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
@RequestMapping(CreateWorkspaceController.PATH)
public final class CreateWorkspaceController {

    public static final String PATH = "/actions/create-workspace";
    private static final String RESOURCE_PATH = "/workspaces";

    private final CreateWorkspaceUseCase useCase;
    private final ScopeAuthorizer scopeAuthorizer;

    public CreateWorkspaceController(CreateWorkspaceUseCase useCase, ScopeAuthorizer scopeAuthorizer) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public ResponseEntity<WorkspaceResponse> execute(
            @Valid @RequestBody CreateWorkspaceCommand command,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "id", command.id());
        var created = useCase.execute(command);
        return ResponseEntity.created(URI.create(RESOURCE_PATH + "/" + created.id()))
                .body(WorkspaceResponse.from(created));
    }
}
