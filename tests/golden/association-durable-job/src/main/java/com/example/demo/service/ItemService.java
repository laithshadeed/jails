package com.example.demo.service;

import com.example.demo.app.ItemRepository;
import com.example.demo.domain.Item;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
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

    public Optional<Item> findById(String id) {
        return repository.findById(id);
    }

    public Item create(Item item) {
        repository.save(item);
        return item;
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(String id) {
        return repository.deleteById(id);
    }
}
