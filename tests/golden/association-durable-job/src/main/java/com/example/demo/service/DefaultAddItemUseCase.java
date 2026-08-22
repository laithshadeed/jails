package com.example.demo.service;

import com.example.demo.app.ItemRepository;
import com.example.demo.domain.Item;
import java.time.Instant;
import java.util.Objects;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** The conventional implementation generated from the target record's field model. */
@Component
public class DefaultAddItemUseCase implements AddItemUseCase {

    private final ItemRepository repository;

    public DefaultAddItemUseCase(ItemRepository repository) {
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
        repository.save(item);
        return item;
    }
}
