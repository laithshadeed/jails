package com.example.demo.web;

import static org.springframework.http.HttpStatus.CONFLICT;
import static org.springframework.http.HttpStatus.NOT_FOUND;

import com.example.demo.service.ChangePayoutStatusCommand;
import com.example.demo.service.ChangePayoutStatusUseCase;
import jakarta.validation.Valid;
import java.util.Objects;
import org.springframework.web.bind.annotation.PutMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;

/** HTTP adapter for one optimistic state transition. */
@RestController
@RequestMapping(ChangePayoutStatusController.PATH)
public final class ChangePayoutStatusController {

    public static final String PATH = "/actions/change-payout-status";
    private final ChangePayoutStatusUseCase useCase;

    public ChangePayoutStatusController(ChangePayoutStatusUseCase useCase) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");

    }

    @PutMapping
    public PayoutResponse execute(
            @Valid @RequestBody ChangePayoutStatusCommand command) {

        try {
            return PayoutResponse.from(useCase.execute(command));
        } catch (ChangePayoutStatusUseCase.NotFoundException missing) {
            throw new ResponseStatusException(NOT_FOUND, missing.getMessage(), missing);
        } catch (ChangePayoutStatusUseCase.StaleVersionException stale) {
            throw new ResponseStatusException(CONFLICT, stale.getMessage(), stale);
        }
    }
}
