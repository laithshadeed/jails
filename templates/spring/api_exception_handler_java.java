package {{pkg}};

import java.util.LinkedHashMap;
import java.util.Map;
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
{{duplicate_key_import}}

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
{{duplicate_key_handler}}

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
