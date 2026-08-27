package com.example.demo.web;

import com.example.demo.service.ItemsByOwnerEmailCriteria;
import com.example.demo.service.ItemsByOwnerEmailQuery;
import jakarta.validation.Valid;
import java.util.List;
import java.util.Objects;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for a typed read-side port. */
@RestController
@RequestMapping(ItemsByOwnerEmailQueryController.PATH)
public final class ItemsByOwnerEmailQueryController {

    public static final String PATH = "/queries/items-by-owner-email";

    private final ItemsByOwnerEmailQuery query;

    public ItemsByOwnerEmailQueryController(ItemsByOwnerEmailQuery query) {
        this.query = Objects.requireNonNull(query, "query is required");

    }

    @PostMapping
    public List<ItemResponse> execute(
            @Valid @RequestBody ItemsByOwnerEmailCriteria criteria) {

        return query.execute(criteria).stream().map(ItemResponse::from).toList();
    }
}
