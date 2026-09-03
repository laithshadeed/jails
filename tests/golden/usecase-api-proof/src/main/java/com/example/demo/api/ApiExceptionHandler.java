package com.example.demo.api;

import java.util.LinkedHashMap;
import java.util.Map;
import org.springframework.dao.DuplicateKeyException;
import org.springframework.dao.EmptyResultDataAccessException;
import org.springframework.dao.OptimisticLockingFailureException;
import org.springframework.http.HttpHeaders;
import org.springframework.http.HttpStatus;
import org.springframework.http.HttpStatusCode;
import org.springframework.http.ProblemDetail;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.MethodArgumentNotValidException;
import org.springframework.web.bind.annotation.ExceptionHandler;
import org.springframework.web.bind.annotation.RestControllerAdvice;
import org.springframework.web.context.request.WebRequest;
import org.springframework.web.servlet.mvc.method.annotation.ResponseEntityExceptionHandler;

/**
 * Turns failures into RFC 9457 problem responses, in one place.
 *
 * <p>Extends Spring's own {@link ResponseEntityExceptionHandler} rather than
 * starting from nothing, so every exception the framework already understands
 * -- an unreadable body, a missing parameter, an unsupported media type --
 * keeps the status code Spring chose for it. Only this application's own
 * failures need a mapping, and they are the sealed set in
 * {@link ApiException}.
 *
 * <p>The response body is {@code application/problem+json}: a media type
 * with a specification behind it, rather than a {@code Map<String, String>}
 * shaped differently in each controller.
 */
@RestControllerAdvice
public class ApiExceptionHandler extends ResponseEntityExceptionHandler {

    /**
     * The application's own failures. The switch has no {@code default}:
     * a new {@link ApiException} variant breaks this build until its status
     * is decided here.
     */
    @ExceptionHandler(ApiException.class)
    public ProblemDetail handleApiException(ApiException failure) {
        HttpStatus status =
                switch (failure) {
                    case ApiException.NotFound ignored -> HttpStatus.NOT_FOUND;
                    case ApiException.Conflict ignored -> HttpStatus.CONFLICT;
                    case ApiException.Rejected ignored -> HttpStatus.UNPROCESSABLE_ENTITY;
                };
        return ProblemDetail.forStatusAndDetail(status, failure.getMessage());
    }

    /**
     * A unique constraint the database enforced, as the 409 it is.
     *
     * <p>Without this, a duplicate reaches the client as a 500 -- which is
     * what alerting pages on and what a client library retries, so one
     * duplicate becomes an incident and then a retry storm. The row was not
     * written and never will be; that is a conflict, not a server fault.
     *
     * <p>The detail deliberately does not name the column. Spring's message
     * carries the constraint name from the driver, which is a schema
     * identifier rather than anything a caller can act on -- and echoing it
     * tells an unauthenticated client the shape of your database.
     */
    @ExceptionHandler(DuplicateKeyException.class)
    public ProblemDetail handleDuplicateKey(DuplicateKeyException failure) {
        return ProblemDetail.forStatusAndDetail(
                HttpStatus.CONFLICT, "a resource with those values already exists");
    }

    /**
     * A precondition the caller stated and the row no longer satisfies.
     *
     * <p>412 rather than 409: the caller sent an `If-Match` and it did not
     * match, which is precisely what 412 means. A 500 here is the worse
     * failure it replaces -- alerting pages on it, client libraries retry it,
     * and the retry cannot succeed because the version has moved on.
     */
    @ExceptionHandler(OptimisticLockingFailureException.class)
    public ProblemDetail handleStalePrecondition(OptimisticLockingFailureException failure) {
        return ProblemDetail.forStatusAndDetail(
                HttpStatus.PRECONDITION_FAILED,
                "the resource has changed since the version you sent");
    }

    /**
     * A row the request named and the database does not have.
     *
     * <p>The detail says nothing about which row: an unauthenticated caller
     * learning that an id exists is the difference between 404 and 403, and
     * a generated handler is the wrong place to decide that.
     */
    @ExceptionHandler(EmptyResultDataAccessException.class)
    public ProblemDetail handleMissingRow(EmptyResultDataAccessException failure) {
        return ProblemDetail.forStatusAndDetail(HttpStatus.NOT_FOUND, "no such resource");
    }

    /**
     * Bean-validation failures on a request body or parameter.
     *
     * <p>Spring's default renders these as a 400 with no indication of which
     * field was wrong, which is the single most common reason a client
     * integration stalls. The field errors go into a {@code fields} extension
     * member -- an RFC 9457 problem document is explicitly extensible, so this
     * needs no bespoke error envelope.
     */
    @Override
    protected ResponseEntity<Object> handleMethodArgumentNotValid(
            MethodArgumentNotValidException failure,
            HttpHeaders headers,
            HttpStatusCode status,
            WebRequest request) {
        ProblemDetail problem =
                ProblemDetail.forStatusAndDetail(status, "the request has invalid fields");
        // LinkedHashMap: field order follows declaration order, so the
        // response is stable and diffable between runs.
        Map<String, String> fields = new LinkedHashMap<>();
        failure.getBindingResult()
                .getFieldErrors()
                .forEach(error -> fields.putIfAbsent(error.getField(), message(error.getDefaultMessage())));
        problem.setProperty("fields", fields);
        return handleExceptionInternal(failure, problem, headers, status, request);
    }

    private static String message(String defaultMessage) {
        return defaultMessage == null ? "is invalid" : defaultMessage;
    }
}
