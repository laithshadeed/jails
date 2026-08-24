package com.example.intercom.service;

import com.example.intercom.domain.Inbox;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface CreateInboxUseCase {

    Inbox execute(CreateInboxCommand command);
}
