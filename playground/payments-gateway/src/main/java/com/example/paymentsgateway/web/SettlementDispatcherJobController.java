package com.example.paymentsgateway.web;

import static org.springframework.http.HttpStatus.CONFLICT;
import static org.springframework.http.HttpStatus.NOT_FOUND;

import com.example.paymentsgateway.ScopeAuthorizer;
import com.example.paymentsgateway.jobs.SettlementDispatcherQueue;
import com.example.paymentsgateway.jobs.SettlementDispatcherWork;
import jakarta.validation.Valid;
import java.net.URI;
import java.util.UUID;
import org.springframework.http.ResponseEntity;
import org.springframework.security.core.Authentication;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;

/** HTTP submission/status adapter for durable work. */
@RestController
@RequestMapping(SettlementDispatcherJobController.PATH)
public final class SettlementDispatcherJobController {

    public static final String PATH = "/jobs/settlement-dispatcher";
    private final SettlementDispatcherQueue queue;
    private final ScopeAuthorizer scopeAuthorizer;

    public SettlementDispatcherJobController(SettlementDispatcherQueue queue, ScopeAuthorizer scopeAuthorizer) {
        this.queue = queue;
        this.scopeAuthorizer = scopeAuthorizer;
    }

    @PostMapping
    public ResponseEntity<SettlementDispatcherQueue.Status> enqueue(
            @Valid @RequestBody SettlementDispatcherWork work,
            Authentication authentication) {
        scopeAuthorizer.require(authentication, "merchantId", work.merchantId());
        try {
            queue.enqueue(work);
        } catch (SettlementDispatcherQueue.IdempotencyConflictException conflict) {
            throw new ResponseStatusException(CONFLICT, conflict.getMessage(), conflict);
        }
        var status = queue.status(work.id()).orElseThrow();
        return ResponseEntity.accepted()
                .location(URI.create(PATH + "/" + work.id()))
                .body(status);
    }

    @GetMapping("/{id}")
    public SettlementDispatcherQueue.Status status(@PathVariable UUID id) {
        return queue.status(id)
                .orElseThrow(() -> new ResponseStatusException(NOT_FOUND, "work not found"));
    }
}
