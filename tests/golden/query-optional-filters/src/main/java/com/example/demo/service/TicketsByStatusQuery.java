package com.example.demo.service;

import com.example.demo.domain.Ticket;
import java.util.List;

/** Read-side port; the application contract contains no JDBC or HTTP types. */
@FunctionalInterface
public interface TicketsByStatusQuery {

    List<Ticket> execute(TicketsByStatusCriteria criteria);
}
