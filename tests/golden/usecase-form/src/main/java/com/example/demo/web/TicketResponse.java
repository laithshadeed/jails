package com.example.demo.web;

import com.example.demo.domain.Ticket;

/**
 * What this application returns. Deliberately not Ticket itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record TicketResponse(
        Long id,
        String subject) {

    /** @return the response describing {@code ticket}. */
    public static TicketResponse from(Ticket ticket) {
        return new TicketResponse(
                ticket.id(),
                ticket.subject());
    }
}
