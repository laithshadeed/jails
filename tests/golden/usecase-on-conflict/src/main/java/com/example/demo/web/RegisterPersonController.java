package com.example.demo.web;

import com.example.demo.service.RegisterPersonCommand;
import com.example.demo.service.RegisterPersonUseCase;
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
@RequestMapping(RegisterPersonController.PATH)
public final class RegisterPersonController {

    public static final String PATH = "/actions/register-person";
    private static final String RESOURCE_PATH = "/people";

    private final RegisterPersonUseCase useCase;

    public RegisterPersonController(RegisterPersonUseCase useCase) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");

    }

    @PostMapping
    public ResponseEntity<PersonResponse> execute(
            @Valid @RequestBody RegisterPersonCommand command) {

        var created = useCase.execute(command);
        return ResponseEntity.created(URI.create(RESOURCE_PATH + "/" + created.id()))
                .body(PersonResponse.from(created));
    }
}
