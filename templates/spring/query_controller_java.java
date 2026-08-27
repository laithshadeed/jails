package {{web}};

{{query_import}}{{port_import}}{{scope_import}}import {{validation}}.validation.Valid;
import java.util.List;
import java.util.Objects;
import org.springframework.web.bind.annotation.PostMapping;
{{binding_import}}import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for a typed read-side port. */
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

    @PostMapping
    public List<{{target}}Response> execute(
            @Valid @{{binding}} {{name}}Criteria criteria{{scope_parameter}}) {
{{scope_checks}}
        return query.execute(criteria).stream().map({{target}}Response::from).toList();
    }
}
