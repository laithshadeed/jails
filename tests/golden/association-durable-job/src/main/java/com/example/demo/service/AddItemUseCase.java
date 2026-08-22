package com.example.demo.service;

import com.example.demo.domain.Item;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface AddItemUseCase {

    Item execute(AddItemCommand command);
}
