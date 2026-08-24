package com.example.intercom.service;

import com.example.intercom.domain.ConversationAssignment;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface AssignConversationUseCase {

    ConversationAssignment execute(AssignConversationCommand command);
}
