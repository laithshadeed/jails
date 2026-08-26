package com.example.demo.web;

import com.example.demo.service.PayoutsByStatusCriteria;
import com.example.demo.service.PayoutsByStatusQuery;
import jakarta.validation.Valid;
import java.util.List;
import java.util.Objects;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for a typed read-side port. */
@RestController
@RequestMapping(PayoutsByStatusQueryController.PATH)
public final class PayoutsByStatusQueryController {

    public static final String PATH = "/queries/payouts-by-status";

    private final PayoutsByStatusQuery query;

    public PayoutsByStatusQueryController(PayoutsByStatusQuery query) {
        this.query = Objects.requireNonNull(query, "query is required");

    }

    @PostMapping
    public List<PayoutResponse> execute(
            @Valid @RequestBody PayoutsByStatusCriteria criteria) {

        return query.execute(criteria).stream().map(PayoutResponse::from).toList();
    }
}
