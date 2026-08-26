package com.example.demo.service;

import com.example.demo.app.ItemRepository;
import com.example.demo.domain.Item;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Item}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class ItemService {

    private final ItemRepository repository;

    public ItemService(ItemRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Item> findAll() {
        return repository.findAll();
    }

    public Optional<Item> findById(UUID id) {
        return repository.findById(id);
    }

    /**
     * Creates the resource, assigning whatever the caller does not.
     *
     * <p>The key is minted here rather than in the request record: deciding
     * what a row is called is an application decision, and the web layer
     * translates.
     */
    public Item create(Item item) {
        return repository.save(new Item(
                UUID.randomUUID(),
                item.ownerId(),
                item.name(),
                item.createdAt()));
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(UUID id) {
        return repository.deleteById(id);
    }
}
