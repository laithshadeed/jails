package {{web}};

import {{pkg}}.{{name}}Workflow;
import java.util.List;
import java.util.UUID;
import org.springframework.http.HttpStatus;
import org.springframework.web.bind.annotation.DeleteMapping;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.ResponseStatus;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;

/** HTTP control plane for the durable {{name}} workflow. */
@RestController
@RequestMapping({{name}}WorkflowController.PATH)
public final class {{name}}WorkflowController {

    public static final String PATH = "/workflows/{{property}}";
    private final {{name}}Workflow workflow;

    public {{name}}WorkflowController({{name}}Workflow workflow) {
        this.workflow = workflow;
    }

    @PostMapping
    @ResponseStatus(HttpStatus.ACCEPTED)
    public {{name}}Workflow.RunStatus start(@RequestBody {{name}}Workflow.StartRequest request) {
        try { return workflow.start(request); }
        catch (IllegalArgumentException invalid) {
            throw new ResponseStatusException(HttpStatus.BAD_REQUEST, invalid.getMessage(), invalid);
        } catch ({{name}}Workflow.IdempotencyConflictException conflict) {
            throw new ResponseStatusException(HttpStatus.CONFLICT, conflict.getMessage(), conflict);
        }
    }

    @GetMapping("/{id}")
    public {{name}}Workflow.RunStatus status(@PathVariable UUID id) {
        return workflow.status(id).orElseThrow(() ->
                new ResponseStatusException(HttpStatus.NOT_FOUND, "workflow run not found"));
    }

    @GetMapping("/{id}/pages")
    public List<{{name}}Workflow.Page> pages(@PathVariable UUID id) {
        if (workflow.status(id).isEmpty()) {
            throw new ResponseStatusException(HttpStatus.NOT_FOUND, "workflow run not found");
        }
        return workflow.pages(id);
    }

    @DeleteMapping("/{id}")
    public {{name}}Workflow.RunStatus cancel(@PathVariable UUID id) {
        try { return workflow.cancel(id); }
        catch ({{name}}Workflow.NotFoundException missing) {
            throw new ResponseStatusException(HttpStatus.NOT_FOUND, missing.getMessage(), missing);
        }
    }
}
