package com.example.webcrawler.web;

import static org.springframework.http.HttpStatus.CONFLICT;
import static org.springframework.http.HttpStatus.NOT_FOUND;

import com.example.webcrawler.jobs.CrawlDispatcherQueue;
import com.example.webcrawler.jobs.CrawlDispatcherWork;
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
@RequestMapping(CrawlDispatcherJobController.PATH)
public final class CrawlDispatcherJobController {

    public static final String PATH = "/jobs/crawl-dispatcher";
    private final CrawlDispatcherQueue queue;

    public CrawlDispatcherJobController(CrawlDispatcherQueue queue) {
        this.queue = queue;

    }

    @PostMapping
    public ResponseEntity<CrawlDispatcherQueue.Status> enqueue(
            @Valid @RequestBody CrawlDispatcherWork work) {

        try {
            queue.enqueue(work);
        } catch (CrawlDispatcherQueue.IdempotencyConflictException conflict) {
            throw new ResponseStatusException(CONFLICT, conflict.getMessage(), conflict);
        }
        var status = queue.status(work.id()).orElseThrow();
        return ResponseEntity.accepted()
                .location(URI.create(PATH + "/" + work.id()))
                .body(status);
    }

    @GetMapping("/{id}")
    public CrawlDispatcherQueue.Status status(@PathVariable UUID id) {
        return queue.status(id)
                .orElseThrow(() -> new ResponseStatusException(NOT_FOUND, "work not found"));
    }
}
