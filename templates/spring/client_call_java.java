package {{pkg}};

{{imports}}{{body_import}}import org.springframework.web.service.annotation.{{exchange}};

/**
 * The one call this application makes to the {{name}} service.
 *
 * <p>An interface and nothing else: Spring builds the implementation, and the
 * base URL is configuration (see {@link {{name}}ClientConfig}), so pointing it
 * at a stub, staging or production is not a code change. It returns a domain
 * type rather than {@code ResponseEntity} because a non-2xx response is
 * already an exception.
 */
public interface {{name}}Client {

    @{{exchange}}("{{path}}")
    {{returns}} call({{parameter}});
}
