package com.example.intercom.service;

import com.example.intercom.domain.Conversation;

/** Atomic state change guarded by tenant scope and an optimistic version. */
@FunctionalInterface
public interface ChangeConversationStatusUseCase {

    Conversation execute(ChangeConversationStatusCommand command);

    final class NotFoundException extends RuntimeException {
        public NotFoundException() { super("resource not found in the authorized scope"); }
    }

    final class StaleVersionException extends RuntimeException {
        public StaleVersionException() { super("resource version is stale"); }
    }
}
