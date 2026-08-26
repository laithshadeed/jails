package com.example.demo.service;

import com.example.demo.app.ItemRepository;
import com.example.demo.domain.Item;
import java.time.Instant;
import java.util.Objects;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/**
 * The implementation that stores the resource and does nothing else.
 *
 * <p>Named for what it does rather than for its position. `Default` is what
 * you call a class when you have not decided what it is, and it gave the
 * reader no way to tell this apart from {@code OutboxAddItemUseCase}, which
 * stores the resource <em>and</em> stages its event.
 */
@Component
public class StoringAddItemUseCase implements AddItemUseCase {

    private final ItemRepository repository;

    public StoringAddItemUseCase(ItemRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Transactional
    @Override
    public Item execute(AddItemCommand command) {
        Objects.requireNonNull(command, "command is required");
        Item item = new Item(
                command.id(),
                command.ownerId(),
                command.name(),
                Instant.now());
        return repository.save(item);
    }
}
