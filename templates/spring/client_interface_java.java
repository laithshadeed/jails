package {{pkg}};

import java.util.List;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.service.annotation.GetExchange;

/**
 * The {{name}} service, as this application uses it.
 *
 * <p>An interface and nothing else: Spring builds the implementation. There is
 * no base URL here on purpose -- the client belongs to a group (see
 * {@link HttpClientsConfig}) whose URL comes from
 * {@code spring.http.serviceclient.*.base-url}, so pointing the client at a
 * stub, a staging host or production is configuration rather than a code
 * change.
 *
 * <p>Return domain types, not {@code ResponseEntity}: a non-2xx response
 * already becomes an exception, so unwrapping one by hand at every call site
 * buys nothing.
 */
public interface {{name}}Client {

    /** @return every item the upstream service knows about. */
    @GetExchange("{{path}}")
    List<{{name}}Payload> findAll();

    /** @return one item by id. A 404 upstream surfaces as an exception. */
    @GetExchange("{{path}}/{id}")
    {{name}}Payload findById(@PathVariable String id);

    /**
     * What the upstream service returns. A record of its own rather than a
     * domain type: the shape belongs to them and will change on their
     * schedule, and letting it reach the domain directly is how an external
     * rename becomes a refactor here.
     */
    record {{name}}Payload(String id, String name) {}
}
