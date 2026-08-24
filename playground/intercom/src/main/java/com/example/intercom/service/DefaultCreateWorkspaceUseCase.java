package com.example.intercom.service;

import com.example.intercom.app.WorkspaceRepository;
import com.example.intercom.domain.Workspace;
import java.time.Instant;
import java.util.Objects;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** The conventional implementation generated from the target record's field model. */
@Component
public class DefaultCreateWorkspaceUseCase implements CreateWorkspaceUseCase {

    private final WorkspaceRepository repository;

    public DefaultCreateWorkspaceUseCase(WorkspaceRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Transactional
    @Override
    public Workspace execute(CreateWorkspaceCommand command) {
        Objects.requireNonNull(command, "command is required");
        Workspace workspace = new Workspace(
                command.id(),
                command.name(),
                Instant.now(),
                Instant.now());
        repository.save(workspace);
        return workspace;
    }
}
