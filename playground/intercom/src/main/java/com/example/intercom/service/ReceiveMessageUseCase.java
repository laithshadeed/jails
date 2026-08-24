package com.example.intercom.service;

import com.example.intercom.domain.Message;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface ReceiveMessageUseCase {

    Message execute(ReceiveMessageCommand command);
}
