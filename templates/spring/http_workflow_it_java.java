package {{pkg}};

import {{clients}}.{{fetcher}}Fetcher;
import {{clients}}.{{fetcher}}Fetcher.FetchException;
import {{clients}}.{{fetcher}}Fetcher.FetchedResource;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Primary;
import org.springframework.context.annotation.Import;
import org.springframework.jdbc.core.simple.JdbcClient;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

@Import({{name}}WorkflowIT.Fakes.class)
@SpringBootTest(properties = {
        "jails.http-workflows.{{property}}.initial-delay=PT1H",
        "jails.http-workflows.{{property}}.max-attempts=2"
})
@org.springframework.transaction.annotation.Transactional
class {{name}}WorkflowIT {

    @Autowired private {{name}}Workflow workflow;
    @Autowired private JdbcClient db;

    @Test
    void robotsCyclesDuplicatesDepthAndPageBoundsAreEnforced() {
        UUID id = UUID.fromString("10000000-0000-0000-0000-000000000001");
        workflow.start(new {{name}}Workflow.StartRequest(id, URI.create("http://example.test/"), 3, 1));

        drain(id, 12);

        assertThat(workflow.status(id)).get().satisfies(status -> {
            assertThat(status.state()).isEqualTo({{name}}Workflow.RunState.SUCCEEDED);
            assertThat(status.pagesVisited()).isEqualTo(3);
        });
        assertThat(workflow.pages(id)).extracting(page -> page.url().toString())
                .containsExactlyInAnyOrder(
                        "http://example.test/",
                        "http://example.test/a",
                        "http://example.test/deep")
                .doesNotContain("http://example.test/private", "http://example.test/too-deep");
    }

    @Test
    void cancellationIsPersistentAndIdempotencyConflictsAreVisible() {
        UUID id = UUID.fromString("20000000-0000-0000-0000-000000000002");
        var request = new {{name}}Workflow.StartRequest(id, URI.create("http://example.test/"), 10, 2);
        workflow.start(request);

        assertThat(workflow.start(request).id()).isEqualTo(id);
        assertThatThrownBy(() -> workflow.start(new {{name}}Workflow.StartRequest(
                id, URI.create("http://example.test/other"), 10, 2)))
                .isInstanceOf({{name}}Workflow.IdempotencyConflictException.class);

        assertThat(workflow.cancel(id).state()).isEqualTo({{name}}Workflow.RunState.CANCELLED);
        workflow.runOnce();
        assertThat(workflow.status(id)).get().extracting({{name}}Workflow.RunStatus::pagesVisited)
                .isEqualTo(0);
    }

    @Test
    void retryableFailureKeepsTheFrontierAndEventuallyCompletes() {
        UUID id = UUID.fromString("30000000-0000-0000-0000-000000000003");
        workflow.start(new {{name}}Workflow.StartRequest(
                id, URI.create("http://example.test/flaky"), 1, 0));

        workflow.runOnce(); // robots
        workflow.runOnce(); // first page attempt fails
        assertThat(workflow.status(id)).get().extracting({{name}}Workflow.RunStatus::state)
                .isEqualTo({{name}}Workflow.RunState.RUNNING);
        db.sql("update {{table}}_frontier set next_attempt_at = now() where run_id = :id")
                .param("id", id).update();
        workflow.runOnce();

        assertThat(workflow.status(id)).get().satisfies(status -> {
            assertThat(status.state()).isEqualTo({{name}}Workflow.RunState.SUCCEEDED);
            assertThat(status.pagesVisited()).isEqualTo(1);
        });
    }

    private void drain(UUID id, int limit) {
        for (int attempt = 0; attempt < limit; attempt++) {
            var state = workflow.status(id).orElseThrow().state();
            if (state == {{name}}Workflow.RunState.SUCCEEDED
                    || state == {{name}}Workflow.RunState.FAILED
                    || state == {{name}}Workflow.RunState.CANCELLED) return;
            workflow.runOnce();
        }
        throw new AssertionError("workflow did not finish within " + limit + " items");
    }

    @TestConfiguration(proxyBeanMethods = false)
    static class Fakes {
        private static final ConcurrentHashMap<String, Integer> CALLS = new ConcurrentHashMap<>();

        @Bean
        @Primary
        {{fetcher}}Fetcher workflowFetcher() {
            return uri -> {
                String path = uri.getPath();
                if (path.equals("/robots.txt")) {
                    return response(uri, "User-agent: *\nDisallow: /private\n");
                }
                if (path.equals("/flaky") && CALLS.merge(uri.toString(), 1, Integer::sum) == 1) {
                    throw new FetchException("temporary outage", true);
                }
                String html = switch (path) {
                    case "/" -> """
                            <a href='/a'>a</a><a href='/a#duplicate'>again</a>
                            <a href='/private'>blocked</a><a href='/deep'>deep</a>
                            <a href='http://other.test/outside'>outside</a>
                            """;
                    case "/a" -> "<a href='/'>cycle</a>";
                    case "/deep" -> "<a href='/too-deep'>bounded</a>";
                    default -> "<p>done</p>";
                };
                return response(uri, html);
            };
        }

        private static FetchedResource response(URI uri, String body) {
            return new FetchedResource(uri, 200, "text/html", body.getBytes(StandardCharsets.UTF_8));
        }
    }
}
