package com.example.demo.service;

import com.example.demo.app.WidgetRepository;
import com.example.demo.domain.TimeOrderedUuid;
import com.example.demo.domain.Widget;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Widget}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class WidgetService {

    private final WidgetRepository repository;

    public WidgetService(WidgetRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Widget> findAll() {
        return repository.findAll();
    }

    public Optional<Widget> findById(UUID id) {
        return repository.findById(id);
    }

    /**
     * Creates the resource, assigning whatever the caller does not.
     *
     * <p>The key is minted here rather than in the request record: deciding
     * what a row is called is an application decision, and the web layer
     * translates.
     */
    public Widget create(Widget widget) {
        return repository.save(new Widget(
                TimeOrderedUuid.next(),
                widget.name()));
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(UUID id) {
        return repository.deleteById(id);
    }
}
