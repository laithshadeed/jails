package com.example.demo.web;

import com.example.demo.service.ReceiveMessageCommand;
import com.example.demo.service.ReceiveMessageUseCase;
import jakarta.validation.Valid;
import java.net.URI;
import java.util.Objects;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for one application use case; the operation itself knows nothing about HTTP. */
@RestController
@RequestMapping(ReceiveMessageController.PATH)
public final class ReceiveMessageController {

    public static final String PATH = "/actions/receive-message";
    private static final String RESOURCE_PATH = "/messages";

    private final ReceiveMessageUseCase useCase;

    public ReceiveMessageController(ReceiveMessageUseCase useCase) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");

    }

    @PostMapping
    public ResponseEntity<MessageResponse> execute(
            @Valid @RequestBody ReceiveMessageCommand command) {

        var created = useCase.execute(command);
        return ResponseEntity.created(URI.create(RESOURCE_PATH + "/" + created.id()))
                .body(MessageResponse.from(created));
    }
}
