package {{pkg}};

{{extra}}{{key_import}}{{location_import}}import {{validation}}.validation.Valid;
import java.util.List;
import java.util.Objects;
{{status_import}}import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.DeleteMapping;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/**
 * HTTP for {@link {{name}}}.
 *
 * <p>Speaks in {@link {{name}}Request} and {@link {{name}}Response} rather
 * than the domain type, so the wire contract and the domain can change
 * independently.
 *
 * <p>{@code @Valid} rejects a malformed body before any application code
 * runs. With {@code jails add api} the rejection is reported as an RFC 9457
 * problem naming each bad field; without it, Spring's default 400 says only
 * that something was wrong.
 */
@RestController
@RequestMapping({{name}}Controller.PATH)
public class {{name}}Controller {

    /** The collection this controller serves. */
    public static final String PATH = "{{path}}";

    private final {{name}}Service service;

    public {{name}}Controller({{name}}Service service) {
        this.service = Objects.requireNonNull(service, "service is required");
    }

    @GetMapping
    public List<{{name}}Response> list() {
        return service.findAll().stream().map({{name}}Response::from).toList();
    }

    /** 404 rather than an empty 200: "no such thing" and "here is nothing" differ. */
    @GetMapping("/{id}")
    public ResponseEntity<{{name}}Response> byId(@PathVariable {{key}} id) {
        return service.findById(id)
                .map({{name}}Response::from)
                .map(ResponseEntity::ok)
                .orElseGet(() -> ResponseEntity.notFound().build());
    }

    @PostMapping
    public ResponseEntity<{{name}}Response> create(@Valid @RequestBody {{name}}Request request) {
        {{name}} created = service.create(request.toDomain());
{{created}}
    }

    /** 204 when something was removed, 404 when there was nothing to remove. */
    @DeleteMapping("/{id}")
    public ResponseEntity<Void> delete(@PathVariable {{key}} id) {
        return service.deleteById(id)
                ? ResponseEntity.noContent().build()
                : ResponseEntity.notFound().build();
    }
}
