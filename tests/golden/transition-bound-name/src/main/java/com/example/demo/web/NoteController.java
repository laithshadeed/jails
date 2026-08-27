package com.example.demo.web;

import com.example.demo.domain.Note;
import com.example.demo.service.NoteService;
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
 * HTTP for {@link Note}.
 *
 * <p>Speaks in {@link NoteRequest} and {@link NoteResponse} rather
 * than the domain type, so the wire contract and the domain can change
 * independently.
 *
 * <p>{@code @Valid} rejects a malformed body before any application code
 * runs. With {@code jails add api} the rejection is reported as an RFC 9457
 * problem naming each bad field; without it, Spring's default 400 says only
 * that something was wrong.
 */
@RestController
@RequestMapping(NoteController.PATH)
public class NoteController {

    /** The collection this controller serves. */
    public static final String PATH = "/notes";

    private final NoteService service;

    public NoteController(NoteService service) {
        this.service = Objects.requireNonNull(service, "service is required");
    }

    @GetMapping
    public List<NoteResponse> list() {
        return service.findAll().stream().map(NoteResponse::from).toList();
    }

    /** 404 rather than an empty 200: "no such thing" and "here is nothing" differ. */
    @GetMapping("/{id}")
    public ResponseEntity<NoteResponse> byId(@PathVariable Long id) {
        return service.findById(id)
                .map(NoteResponse::from)
                .map(ResponseEntity::ok)
                .orElseGet(() -> ResponseEntity.notFound().build());
    }

    @PostMapping
    public ResponseEntity<NoteResponse> create(@Valid @RequestBody NoteRequest request) {
        Note created = service.create(request.toDomain());
        return ResponseEntity.created(URI.create(PATH + "/" + created.id()))
                .body(NoteResponse.from(created));
    }

    /** 204 when something was removed, 404 when there was nothing to remove. */
    @DeleteMapping("/{id}")
    public ResponseEntity<Void> delete(@PathVariable Long id) {
        return service.deleteById(id)
                ? ResponseEntity.noContent().build()
                : ResponseEntity.notFound().build();
    }
}
