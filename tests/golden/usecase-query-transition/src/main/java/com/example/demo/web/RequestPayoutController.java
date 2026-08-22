package com.example.demo.web;

import com.example.demo.service.RequestPayoutCommand;
import com.example.demo.service.RequestPayoutUseCase;
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
@RequestMapping(RequestPayoutController.PATH)
public final class RequestPayoutController {

    public static final String PATH = "/actions/request-payout";
    private static final String RESOURCE_PATH = "/payouts";

    private final RequestPayoutUseCase useCase;

    public RequestPayoutController(RequestPayoutUseCase useCase) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");

    }

    @PostMapping
    public ResponseEntity<PayoutResponse> execute(
            @Valid @RequestBody RequestPayoutCommand command) {

        var created = useCase.execute(command);
        return ResponseEntity.created(URI.create(RESOURCE_PATH + "/" + created.id()))
                .body(PayoutResponse.from(created));
    }
}
