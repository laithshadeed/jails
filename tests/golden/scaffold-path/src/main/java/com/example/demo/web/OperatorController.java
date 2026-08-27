package com.example.demo.web;

import com.example.demo.domain.Operator;
import com.example.demo.service.OperatorService;
import jakarta.validation.Valid;
import java.net.URI;
import java.util.List;
import java.util.Objects;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.DeleteMapping;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/**
 * HTTP for {@link Operator}.
 *
 * <p>Speaks in {@link OperatorRequest} and {@link OperatorResponse} rather
 * than the domain type, so the wire contract and the domain can change
 * independently.
 *
 * <p>{@code @Valid} rejects a malformed body before any application code
 * runs. With {@code jails add api} the rejection is reported as an RFC 9457
 * problem naming each bad field; without it, Spring's default 400 says only
 * that something was wrong.
 */
@RestController
@RequestMapping(OperatorController.PATH)
public class OperatorController {

    /** The collection this controller serves. */
    public static final String PATH = "/admin_api/operators";

    private final OperatorService service;

    public OperatorController(OperatorService service) {
        this.service = Objects.requireNonNull(service, "service is required");
    }

    @GetMapping
    public List<OperatorResponse> list() {
        return service.findAll().stream().map(OperatorResponse::from).toList();
    }

    /** 404 rather than an empty 200: "no such thing" and "here is nothing" differ. */
    @GetMapping("/{id}")
    public ResponseEntity<OperatorResponse> byId(@PathVariable Long id) {
        return service.findById(id)
                .map(OperatorResponse::from)
                .map(ResponseEntity::ok)
                .orElseGet(() -> ResponseEntity.notFound().build());
    }

    @PostMapping
    public ResponseEntity<OperatorResponse> create(@Valid @RequestBody OperatorRequest request) {
        Operator created = service.create(request.toDomain());
        return ResponseEntity.created(URI.create(PATH + "/" + created.id()))
                .body(OperatorResponse.from(created));
    }

    /** 204 when something was removed, 404 when there was nothing to remove. */
    @DeleteMapping("/{id}")
    public ResponseEntity<Void> delete(@PathVariable Long id) {
        return service.deleteById(id)
                ? ResponseEntity.noContent().build()
                : ResponseEntity.notFound().build();
    }
}
