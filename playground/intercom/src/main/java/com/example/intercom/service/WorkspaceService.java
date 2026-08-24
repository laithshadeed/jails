package com.example.intercom.service;

import com.example.intercom.app.WorkspaceRepository;
import com.example.intercom.domain.Workspace;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Workspace}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class WorkspaceService {

    private final WorkspaceRepository repository;

    public WorkspaceService(WorkspaceRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Workspace> findAll() {
        return repository.findAll();
    }

    public Optional<Workspace> findById(String id) {
        return repository.findById(id);
    }

    public Workspace create(Workspace workspace) {
        repository.save(workspace);
        return workspace;
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(String id) {
        return repository.deleteById(id);
    }
}
