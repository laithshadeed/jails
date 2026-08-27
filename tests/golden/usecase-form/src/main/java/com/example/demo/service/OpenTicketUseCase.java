package com.example.demo.service;

import com.example.demo.domain.Ticket;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface OpenTicketUseCase {

    Ticket execute(OpenTicketCommand command);
}
