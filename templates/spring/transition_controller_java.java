package {{web}};

{{command_import}}{{usecase_import}}{{target_import}}{{scope_import}}{{failure_imports}}import {{validation}}.validation.Valid;
import java.util.Objects;
import org.springframework.http.HttpHeaders;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.{{mapping}};
{{binding_import}}import org.springframework.web.bind.annotation.RequestHeader;
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
@RequestMapping({{name}}Controller.PATH)
public final class {{name}}Controller {

    public static final String PATH = "{{path}}";
    private final {{name}}UseCase useCase;
{{scope_field}}

    public {{name}}Controller({{name}}UseCase useCase{{scope_constructor}}) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");
{{scope_assignment}}
    }

    @{{mapping}}
    public ResponseEntity<{{target}}Response> execute(
            @RequestHeader(HttpHeaders.IF_MATCH) String ifMatch,
            @Valid @{{binding}} {{name}}Command command{{scope_parameter}}) {
{{scope_checks}}
        {{version_type}} expected = expectedVersion(ifMatch);
        // No `default`: the port's outcomes are sealed, so a fourth one stops
        // this file compiling rather than falling through to a status nobody
        // chose.
        return switch (useCase.execute(command, expected)) {
{{arms}}
        };
    }

    /**
     * The version the caller believes the row is at.
     *
     * <p>Accepts the weak-validator prefix and the quotes RFC 9110 requires,
     * because that is what a client library sends back after reading the
     * {@code ETag} this controller issued.
     */
    private static {{version_type}} expectedVersion(String ifMatch) {
        String value = ifMatch.trim();
        if (value.startsWith("W/")) {
            value = value.substring(2);
        }
        if (value.length() >= 2 && value.startsWith("\"") && value.endsWith("\"")) {
            value = value.substring(1, value.length() - 1);
        }
        try {
            return {{parse}}(value);
        } catch (NumberFormatException malformed) {
            throw new ResponseStatusException(
                    HttpStatus.BAD_REQUEST,
                    "If-Match is not a version this resource issued: " + ifMatch,
                    malformed);
        }
    }
}
