package com.example.demo.web;

import com.example.demo.service.PostNoteCommand;
import com.example.demo.service.PostNoteUseCase;
import jakarta.validation.Valid;
import java.net.URI;
import java.util.Objects;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.ModelAttribute;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for one application use case; the operation itself knows nothing about HTTP. */
@RestController
@RequestMapping(PostNoteController.PATH)
public final class PostNoteController {

    public static final String PATH = "/customer_api/notes";
    private static final String RESOURCE_PATH = "/notes";

    private final PostNoteUseCase useCase;

    public PostNoteController(PostNoteUseCase useCase) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");

    }

    @PostMapping
    public ResponseEntity<NoteResponse> execute(
            @Valid @ModelAttribute PostNoteCommand command) {

        return useCase.execute(command)
                .map(created -> ResponseEntity.created(
                                URI.create(RESOURCE_PATH + "/" + created.id()))
                        .body(NoteResponse.from(created)))
                .orElseGet(() -> ResponseEntity.notFound().build());
    }
}
