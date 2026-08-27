package com.example.demo.web;

import com.example.demo.service.OpenTicketsCriteria;
import com.example.demo.service.OpenTicketsQuery;
import jakarta.validation.Valid;
import java.util.List;
import java.util.Objects;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.ModelAttribute;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for a typed read-side port. */
@RestController
@RequestMapping(OpenTicketsQueryController.PATH)
public final class OpenTicketsQueryController {

    public static final String PATH = "/admin_api/tickets";

    private final OpenTicketsQuery query;

    public OpenTicketsQueryController(OpenTicketsQuery query) {
        this.query = Objects.requireNonNull(query, "query is required");

    }

    @GetMapping
    public List<TicketResponse> execute(
            @Valid @ModelAttribute OpenTicketsCriteria criteria) {

        return query.execute(criteria).stream().map(TicketResponse::from).toList();
    }
}
