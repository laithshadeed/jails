package {{web}};

{{queue_import}}{{work_import}}{{scope_import}}import jakarta.validation.Valid;
import java.net.URI;
import java.util.UUID;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;

import static org.springframework.http.HttpStatus.CONFLICT;
import static org.springframework.http.HttpStatus.NOT_FOUND;

/** HTTP submission/status adapter for durable work. */
@RestController
@RequestMapping({{name}}JobController.PATH)
public final class {{name}}JobController {

    public static final String PATH = "{{path}}";
    private final {{name}}Queue queue;
{{scope_field}}

    public {{name}}JobController({{name}}Queue queue{{scope_constructor}}) {
        this.queue = queue;
{{scope_assignment}}
    }

    @PostMapping
    public ResponseEntity<{{name}}Queue.Status> enqueue(
            @Valid @RequestBody {{name}}Work work{{scope_parameter}}) {
{{scope_checks}}
        try {
            queue.enqueue(work);
        } catch ({{name}}Queue.IdempotencyConflictException conflict) {
            throw new ResponseStatusException(CONFLICT, conflict.getMessage(), conflict);
        }
        var status = queue.status(work.id()).orElseThrow();
        return ResponseEntity.accepted()
                .location(URI.create(PATH + "/" + work.id()))
                .body(status);
    }

    @GetMapping("/{id}")
    public {{name}}Queue.Status status(@PathVariable UUID id) {
        return queue.status(id)
                .orElseThrow(() -> new ResponseStatusException(NOT_FOUND, "work not found"));
    }
}
