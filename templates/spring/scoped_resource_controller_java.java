package {{pkg}};

{{extra}}{{scope_import}}{{location_import}}import {{validation}}.validation.Valid;
import java.util.Objects;
{{status_import}}import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/**
 * Scope-safe creation endpoint for {@link {{name}}}.
 *
 * <p>The broad list, id lookup and delete routes are intentionally absent:
 * a plain repository operation cannot prove a tenant boundary. Generate an
 * {@code @scope} query or use case for each authorized operation instead.
 */
@RestController
@RequestMapping({{name}}Controller.PATH)
public class {{name}}Controller {

    public static final String PATH = "{{path}}";

    private final {{name}}Service service;
{{scope_field}}

    public {{name}}Controller({{name}}Service service{{scope_constructor}}) {
        this.service = Objects.requireNonNull(service, "service is required");
{{scope_assignment}}
    }

    @PostMapping
    public ResponseEntity<{{name}}Response> create(
            @Valid @RequestBody {{name}}Request request{{scope_parameter}}) {
{{scope_checks}}
        {{name}} created = service.create(request.toDomain());
{{created}}
    }
}
