package com.example.webcrawler.web;

import com.example.webcrawler.jobs.SiteTraversalWorkflow;
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

/** HTTP control plane for the durable SiteTraversal workflow. */
@RestController
@RequestMapping(SiteTraversalWorkflowController.PATH)
public final class SiteTraversalWorkflowController {

    public static final String PATH = "/workflows/site-traversal";
    private final SiteTraversalWorkflow workflow;

    public SiteTraversalWorkflowController(SiteTraversalWorkflow workflow) {
        this.workflow = workflow;
    }

    @PostMapping
    @ResponseStatus(HttpStatus.ACCEPTED)
    public SiteTraversalWorkflow.RunStatus start(@RequestBody SiteTraversalWorkflow.StartRequest request) {
        try { return workflow.start(request); }
        catch (IllegalArgumentException invalid) {
            throw new ResponseStatusException(HttpStatus.BAD_REQUEST, invalid.getMessage(), invalid);
        } catch (SiteTraversalWorkflow.IdempotencyConflictException conflict) {
            throw new ResponseStatusException(HttpStatus.CONFLICT, conflict.getMessage(), conflict);
        }
    }

    @GetMapping("/{id}")
    public SiteTraversalWorkflow.RunStatus status(@PathVariable UUID id) {
        return workflow.status(id).orElseThrow(() ->
                new ResponseStatusException(HttpStatus.NOT_FOUND, "workflow run not found"));
    }

    @GetMapping("/{id}/pages")
    public List<SiteTraversalWorkflow.Page> pages(@PathVariable UUID id) {
        if (workflow.status(id).isEmpty()) {
            throw new ResponseStatusException(HttpStatus.NOT_FOUND, "workflow run not found");
        }
        return workflow.pages(id);
    }

    @DeleteMapping("/{id}")
    public SiteTraversalWorkflow.RunStatus cancel(@PathVariable UUID id) {
        try { return workflow.cancel(id); }
        catch (SiteTraversalWorkflow.NotFoundException missing) {
            throw new ResponseStatusException(HttpStatus.NOT_FOUND, missing.getMessage(), missing);
        }
    }
}
