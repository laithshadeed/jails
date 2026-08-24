package com.example.intercom.service;

import com.example.intercom.domain.Contact;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface CreateContactUseCase {

    Contact execute(CreateContactCommand command);
}
