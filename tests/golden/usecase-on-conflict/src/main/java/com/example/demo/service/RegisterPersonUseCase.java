package com.example.demo.service;

import com.example.demo.domain.Person;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface RegisterPersonUseCase {

    Person execute(RegisterPersonCommand command);
}
