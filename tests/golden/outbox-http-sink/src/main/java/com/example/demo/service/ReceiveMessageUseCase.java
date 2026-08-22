package com.example.demo.service;

import com.example.demo.domain.Message;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface ReceiveMessageUseCase {

    Message execute(ReceiveMessageCommand command);
}
