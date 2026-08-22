package com.example.demo.web;

import com.example.demo.service.AddItemCommand;
import com.example.demo.service.AddItemUseCase;
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
@RequestMapping(AddItemController.PATH)
public final class AddItemController {

    public static final String PATH = "/actions/add-item";
    private static final String RESOURCE_PATH = "/items";

    private final AddItemUseCase useCase;

    public AddItemController(AddItemUseCase useCase) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");

    }

    @PostMapping
    public ResponseEntity<ItemResponse> execute(
            @Valid @RequestBody AddItemCommand command) {

        var created = useCase.execute(command);
        return ResponseEntity.created(URI.create(RESOURCE_PATH + "/" + created.id()))
                .body(ItemResponse.from(created));
    }
}
