package com.example.demo.web;

import static org.springframework.http.HttpStatus.CONFLICT;
import static org.springframework.http.HttpStatus.NOT_FOUND;

import com.example.demo.jobs.ItemDispatcherQueue;
import com.example.demo.jobs.ItemDispatcherWork;
import jakarta.validation.Valid;
import java.net.URI;
import java.util.UUID;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;

/** HTTP submission/status adapter for durable work. */
@RestController
@RequestMapping(ItemDispatcherJobController.PATH)
public final class ItemDispatcherJobController {

    public static final String PATH = "/jobs/item-dispatcher";
    private final ItemDispatcherQueue queue;

    public ItemDispatcherJobController(ItemDispatcherQueue queue) {
        this.queue = queue;

    }

    @PostMapping
    public ResponseEntity<ItemDispatcherQueue.Status> enqueue(
            @Valid @RequestBody ItemDispatcherWork work) {

        try {
            queue.enqueue(work);
        } catch (ItemDispatcherQueue.IdempotencyConflictException conflict) {
            throw new ResponseStatusException(CONFLICT, conflict.getMessage(), conflict);
        }
        var status = queue.status(work.id()).orElseThrow();
        return ResponseEntity.accepted()
                .location(URI.create(PATH + "/" + work.id()))
                .body(status);
    }

    @GetMapping("/{id}")
    public ItemDispatcherQueue.Status status(@PathVariable UUID id) {
        return queue.status(id)
                .orElseThrow(() -> new ResponseStatusException(NOT_FOUND, "work not found"));
    }
}
