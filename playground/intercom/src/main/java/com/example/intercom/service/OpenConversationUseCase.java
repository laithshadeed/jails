package com.example.intercom.service;

import com.example.intercom.domain.Conversation;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface OpenConversationUseCase {

    Conversation execute(OpenConversationCommand command);
}
