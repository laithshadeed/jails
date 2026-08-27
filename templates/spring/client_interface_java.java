package {{pkg}};

import java.util.List;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.service.annotation.GetExchange;

/**
 * The {{name}} service, as this application uses it.
 *
 * <p>An interface and nothing else: Spring builds the implementation, and the
 * base URL is configuration (see {@link {{name}}ClientConfig}), so pointing it
 * at a stub, staging or production is not a code change. It returns domain
 * types rather than {@code ResponseEntity} because a non-2xx response is
 * already an exception.
 */
public interface {{name}}Client {

    /** @return every item the upstream service knows about. */
    @GetExchange("{{path}}")
    List<{{name}}Payload> findAll();

    /** @return one item by id. A 404 upstream surfaces as an exception. */
    @GetExchange("{{path}}/{id}")
    {{name}}Payload findById(@PathVariable String id);

    /** Theirs, not yours: an external rename must not become a refactor here. */
    record {{name}}Payload(String id, String name) {}
}
