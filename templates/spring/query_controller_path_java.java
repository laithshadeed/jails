package {{web}};

{{query_import}}{{port_import}}{{scope_import}}{{imports}}import java.util.List;
import java.util.Objects;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/**
 * HTTP adapter for a typed read-side port, addressed by its filters.
 *
 * <p>A GET with no body: every filter this query takes is in the URL, so
 * there is nothing left for a request body to carry. The criteria record is
 * still what the port takes -- the path variables are bound and handed to it
 * here, so the port never learns that some of its input came from a URL.
 */
@RestController
@RequestMapping({{name}}QueryController.PATH)
public final class {{name}}QueryController {

    public static final String PATH = "{{path}}";

    private final {{name}}Query query;
{{scope_field}}

    public {{name}}QueryController({{name}}Query query{{scope_constructor}}) {
        this.query = Objects.requireNonNull(query, "query is required");
{{scope_assignment}}
    }

    @GetMapping
    public List<{{target}}Response> execute({{path_parameters}}{{scope_parameter}}) {
{{scope_checks}}
        var criteria = new {{name}}Criteria({{criteria_arguments}});
        return query.execute(criteria).stream().map({{target}}Response::from).toList();
    }
}
