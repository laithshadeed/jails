package com.example.demo.web;

import com.example.demo.service.OpenTicketCommand;
import com.example.demo.service.OpenTicketUseCase;
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
@RequestMapping(OpenTicketController.PATH)
public final class OpenTicketController {

    public static final String PATH = "/customer_api/open";
    private static final String RESOURCE_PATH = "/tickets";

    private final OpenTicketUseCase useCase;

    public OpenTicketController(OpenTicketUseCase useCase) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");

    }

    @PostMapping
    public ResponseEntity<TicketResponse> execute(
            @Valid @ModelAttribute OpenTicketCommand command) {

        var created = useCase.execute(command);
        return ResponseEntity.created(URI.create(RESOURCE_PATH + "/" + created.id()))
                .body(TicketResponse.from(created));
    }
}
