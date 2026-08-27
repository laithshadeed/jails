package com.example.demo.web;

import com.example.demo.service.SetTopicSubjectCommand;
import com.example.demo.service.SetTopicSubjectUseCase;
import jakarta.validation.Valid;
import java.util.Objects;
import org.springframework.http.HttpHeaders;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.PatchMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestHeader;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;

/**
 * HTTP for one optimistic state transition.
 *
 * <p>The version travels as {@code If-Match} and comes back as an
 * {@code ETag}. It used to be a field in the request body, which is a bespoke
 * spelling of a thing HTTP already has -- and one that no cache, proxy or
 * client library understands.
 */
@RestController
@RequestMapping(SetTopicSubjectController.PATH)
public final class SetTopicSubjectController {

    public static final String PATH = "/admin_api/topics/{userId}/subject";
    private final SetTopicSubjectUseCase useCase;

    public SetTopicSubjectController(SetTopicSubjectUseCase useCase) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");

    }

    @PatchMapping
    public ResponseEntity<TopicResponse> execute(
            @RequestHeader(HttpHeaders.IF_MATCH) String ifMatch,
            @PathVariable Long userId,
            @Valid @RequestBody SetTopicSubjectCommand command) {

        long expected = expectedVersion(ifMatch);
        // No `default`: the port's outcomes are sealed, so a fourth one stops
        // this file compiling rather than falling through to a status nobody
        // chose.
        return switch (useCase.execute(userId, command, expected)) {
            case SetTopicSubjectUseCase.Result.Applied(var resource) ->
                    ResponseEntity.ok()
                            .eTag(String.valueOf(resource.version()))
                            .body(TopicResponse.from(resource));
            case SetTopicSubjectUseCase.Result.StaleVersion(var current) ->
                    ResponseEntity.status(HttpStatus.PRECONDITION_FAILED)
                            .eTag(String.valueOf(current.version()))
                            .body(TopicResponse.from(current));
            case SetTopicSubjectUseCase.Result.NotFound(var id) -> ResponseEntity.notFound().build();
        };
    }

    /**
     * The version the caller believes the row is at.
     *
     * <p>Accepts the weak-validator prefix and the quotes RFC 9110 requires,
     * because that is what a client library sends back after reading the
     * {@code ETag} this controller issued.
     */
    private static long expectedVersion(String ifMatch) {
        String value = ifMatch.trim();
        if (value.startsWith("W/")) {
            value = value.substring(2);
        }
        if (value.length() >= 2 && value.startsWith("\"") && value.endsWith("\"")) {
            value = value.substring(1, value.length() - 1);
        }
        try {
            return Long.parseLong(value);
        } catch (NumberFormatException malformed) {
            throw new ResponseStatusException(
                    HttpStatus.BAD_REQUEST,
                    "If-Match is not a version this resource issued: " + ifMatch,
                    malformed);
        }
    }
}
