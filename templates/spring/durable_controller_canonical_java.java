package {{web}};

{{queue_import}}{{input_import}}import java.util.UUID;
import org.springframework.http.HttpStatus;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.ResponseStatus;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;

/**
 * The control plane for the durable {{name}} queue.
 *
 * <p>Accepting returns 202 and a location to poll, never the result: the whole
 * point of the queue is that the work has not happened yet, and a 200 with a
 * body would be a lie a caller would build on.
 *
 * <p><strong>The caller supplies the id.</strong> That is what makes a retried
 * request the same request -- a client that times out and sends again gets the
 * same item rather than a second one -- and it is why a reused id with a
 * different payload is a 409 rather than an overwrite.
 */
@RestController
@RequestMapping({{name}}JobController.PATH)
public final class {{name}}JobController {

    public static final String PATH = "{{path}}";

    private final {{name}}Queue queue;

    public {{name}}JobController({{name}}Queue queue) {
        this.queue = queue;
    }

    @PostMapping("/{id}")
    @ResponseStatus(HttpStatus.ACCEPTED)
    public void enqueue(@PathVariable UUID id, @RequestBody {{usecase}}Command.Input work) {
        try {
            queue.enqueue(id, work);
        } catch ({{name}}Queue.IdempotencyConflictException conflict) {
            throw new ResponseStatusException(HttpStatus.CONFLICT, conflict.getMessage(), conflict);
        }
    }

    @GetMapping("/{id}")
    public {{name}}Queue.Status status(@PathVariable UUID id) {
        return queue.status(id)
                .orElseThrow(() -> new ResponseStatusException(HttpStatus.NOT_FOUND, "no such work item"));
    }
}
