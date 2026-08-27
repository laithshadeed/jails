package com.example.demo.web;

import com.example.demo.service.PostAdminNoteCommand;
import com.example.demo.service.PostAdminNoteUseCase;
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
@RequestMapping(PostAdminNoteController.PATH)
public final class PostAdminNoteController {

    public static final String PATH = "/admin_api/notes";
    private static final String RESOURCE_PATH = "/notes";

    private final PostAdminNoteUseCase useCase;

    public PostAdminNoteController(PostAdminNoteUseCase useCase) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");

    }

    @PostMapping
    public ResponseEntity<NoteResponse> execute(
            @Valid @ModelAttribute PostAdminNoteCommand command) {

        var created = useCase.execute(command);
        return ResponseEntity.created(URI.create(RESOURCE_PATH + "/" + created.id()))
                .body(NoteResponse.from(created));
    }
}
