package com.example.demo.web;

import com.example.demo.service.TicketsByStatusCriteria;
import com.example.demo.service.TicketsByStatusQuery;
import jakarta.validation.Valid;
import java.util.List;
import java.util.Objects;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for a typed read-side port. */
@RestController
@RequestMapping(TicketsByStatusQueryController.PATH)
public final class TicketsByStatusQueryController {

    public static final String PATH = "/queries/tickets-by-status";

    private final TicketsByStatusQuery query;

    public TicketsByStatusQueryController(TicketsByStatusQuery query) {
        this.query = Objects.requireNonNull(query, "query is required");

    }

    @PostMapping
    public List<TicketResponse> execute(
            @Valid @RequestBody TicketsByStatusCriteria criteria) {

        return query.execute(criteria).stream().map(TicketResponse::from).toList();
    }
}
